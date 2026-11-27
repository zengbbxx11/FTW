use super::{
    failure, sha256, Manifest, Result, ARCHIVE_NAME, MANIFEST_NAME, PUBLIC_KEY, SIGNATURE_NAME,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use ring::{
    aead, pbkdf2,
    rand::{SecureRandom, SystemRandom},
    signature::{Ed25519KeyPair, KeyPair},
};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    num::NonZeroU32,
    path::Path,
};

const BACKUP_ITERATIONS: u32 = 600_000;
const BACKUP_CONTEXT: &[u8] = b"FTW release key backup v1";

#[derive(Deserialize, Serialize)]
struct KeyBackup {
    version: u8,
    salt: String,
    nonce: String,
    ciphertext: String,
}

fn backup_cipher(password: &str, salt: &[u8]) -> Result<aead::LessSafeKey> {
    let mut derived = [0u8; 32];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(BACKUP_ITERATIONS).unwrap(),
        salt,
        password.as_bytes(),
        &mut derived,
    );
    let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, &derived)
        .map_err(|_| failure("release_backup_failed"))?;
    derived.fill(0);
    Ok(aead::LessSafeKey::new(unbound))
}

pub fn backup_key(private_path: &Path, destination: &Path, password: &str) -> Result<()> {
    if password.len() < 12 {
        return Err(failure("release_backup_password_too_short"));
    }
    let encoded = ftw_ipc::credentials::read(&fs::read(private_path)?)?;
    let mut private = STANDARD.decode(encoded)?;
    let key = Ed25519KeyPair::from_pkcs8(&private).map_err(|_| failure("release_key_invalid"))?;
    if STANDARD.encode(key.public_key().as_ref()) != PUBLIC_KEY.trim() {
        return Err(failure("release_key_mismatch"));
    }
    let random = SystemRandom::new();
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 12];
    random
        .fill(&mut salt)
        .map_err(|_| failure("release_backup_failed"))?;
    random
        .fill(&mut nonce)
        .map_err(|_| failure("release_backup_failed"))?;
    backup_cipher(password, &salt)?
        .seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(BACKUP_CONTEXT),
            &mut private,
        )
        .map_err(|_| failure("release_backup_failed"))?;
    let backup = KeyBackup {
        version: 1,
        salt: STANDARD.encode(salt),
        nonce: STANDARD.encode(nonce),
        ciphertext: STANDARD.encode(&private),
    };
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    file.write_all(&serde_json::to_vec_pretty(&backup)?)?;
    Ok(())
}

pub fn restore_key(backup_path: &Path, destination: &Path, password: &str) -> Result<()> {
    let backup: KeyBackup = serde_json::from_slice(&fs::read(backup_path)?)?;
    let salt = STANDARD.decode(backup.salt)?;
    let nonce: [u8; 12] = STANDARD
        .decode(backup.nonce)?
        .try_into()
        .map_err(|_| failure("release_backup_invalid"))?;
    if backup.version != 1 || salt.len() != 16 {
        return Err(failure("release_backup_invalid"));
    }
    let mut encrypted = STANDARD.decode(backup.ciphertext)?;
    let private = backup_cipher(password, &salt)?
        .open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(BACKUP_CONTEXT),
            &mut encrypted,
        )
        .map_err(|_| failure("release_backup_password_invalid"))?;
    let key = Ed25519KeyPair::from_pkcs8(private).map_err(|_| failure("release_key_invalid"))?;
    if STANDARD.encode(key.public_key().as_ref()) != PUBLIC_KEY.trim() {
        return Err(failure("release_key_mismatch"));
    }
    let protected = ftw_ipc::credentials::protect(&STANDARD.encode(private))?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    file.write_all(&protected)?;
    Ok(())
}

pub fn initialize_key(private_path: &Path, public_path: &Path) -> Result<()> {
    if private_path.exists()
        || fs::read_to_string(public_path).is_ok_and(|key| key.trim() != "UNCONFIGURED")
    {
        return Err(failure("release_key_already_exists"));
    }
    let private = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
        .map_err(|_| failure("release_key_generation_failed"))?;
    let key = Ed25519KeyPair::from_pkcs8(private.as_ref())
        .map_err(|_| failure("release_key_generation_failed"))?;
    let protected = ftw_ipc::credentials::protect(&STANDARD.encode(private.as_ref()))?;
    if let Some(parent) = private_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(private_path)?;
    file.write_all(&protected)?;
    fs::write(
        public_path,
        format!("{}\n", STANDARD.encode(key.public_key().as_ref())),
    )?;
    Ok(())
}

pub fn sign_release(archive: &Path, version: &str, private_path: &Path) -> Result<()> {
    let parsed = Version::parse(version)?;
    if !parsed.pre.is_empty()
        || !parsed.build.is_empty()
        || archive.file_name().and_then(|name| name.to_str()) != Some(ARCHIVE_NAME)
    {
        return Err(failure("release_version_or_archive_invalid"));
    }
    let protected = fs::read(private_path)?;
    let encoded = ftw_ipc::credentials::read(&protected)?;
    let private = STANDARD.decode(encoded)?;
    let key = Ed25519KeyPair::from_pkcs8(&private).map_err(|_| failure("release_key_invalid"))?;
    if STANDARD.encode(key.public_key().as_ref()) != PUBLIC_KEY.trim() {
        return Err(failure("release_key_mismatch"));
    }
    let manifest = Manifest {
        version: version.to_owned(),
        platform: "windows-x86_64".to_owned(),
        asset: ARCHIVE_NAME.to_owned(),
        sha256: sha256(archive)?,
    };
    let contents = serde_json::to_vec_pretty(&manifest)?;
    let signature = key.sign(&contents);
    let directory = archive
        .parent()
        .ok_or_else(|| failure("release_directory_invalid"))?;
    fs::write(directory.join(MANIFEST_NAME), &contents)?;
    fs::write(
        directory.join(SIGNATURE_NAME),
        STANDARD.encode(signature.as_ref()),
    )?;
    Ok(())
}
