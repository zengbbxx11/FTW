mod install;
mod release;

pub use install::{apply, detect_workbenches, running_adapters};
pub use release::{backup_key, initialize_key, restore_key, sign_release};

use base64::{engine::general_purpose::STANDARD, Engine};
use ring::{digest, signature};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    io::{self, Read},
    os::windows::process::CommandExt,
    path::{Component, Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

const CREATE_NO_WINDOW: u32 = 0x08000000;
const REPOSITORY: &str = "https://github.com/zengbbxx11/FTW";
const RELEASE_API: &str = "https://api.github.com/repos/zengbbxx11/FTW/releases/latest";
pub const ARCHIVE_NAME: &str = "FTW-Portable-win-x64.zip";
pub const MANIFEST_NAME: &str = "FTW-Update.json";
pub const SIGNATURE_NAME: &str = "FTW-Update.json.sig";
pub const UPDATE_FILES: &[&str] = &[
    "workbench.exe",
    "ftw-core.exe",
    "ftw-mcp.exe",
    "FTW-Update.exe",
    "README.txt",
    "FTW-Agent-Quickstart.txt",
    "FTW-Agent-Workflows.txt",
    "THIRD_PARTY_UI.txt",
    "FTW-Diagnostics.cmd",
];
const PUBLIC_KEY: &str = include_str!("update-public-key.txt");

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Manifest {
    pub version: String,
    pub platform: String,
    pub asset: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: String,
    pub release_url: String,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    body: Option<String>,
    draft: bool,
    prerelease: bool,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

struct VerifiedRelease {
    info: UpdateInfo,
    manifest: Manifest,
    archive_url: String,
}

pub struct PreparedUpdate {
    pub directory: PathBuf,
    pub version: String,
}

fn failure(message: &'static str) -> Box<dyn std::error::Error + Send + Sync> {
    io::Error::other(message).into()
}

fn system_tool(name: &str) -> PathBuf {
    PathBuf::from(env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
        .join("System32")
        .join(name)
}

fn download(url: &str, destination: &Path, max_size: u64) -> Result<()> {
    if !url.starts_with("https://") {
        return Err(failure("update_download_invalid_url"));
    }
    let status = Command::new(system_tool("curl.exe"))
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "15",
            "--max-time",
            "300",
            "--max-filesize",
            &max_size.to_string(),
            "--user-agent",
            "FTW-Updater",
            "--header",
            "Accept: application/vnd.github+json",
            "--write-out",
            "%{http_code}",
            "--output",
        ])
        .arg(destination)
        .arg(url)
        .output()?;
    if !status.status.success() {
        if status.stdout == b"404" {
            return Err(failure("update_release_unavailable"));
        }
        return Err(failure("update_download_failed"));
    }
    if fs::metadata(destination)?.len() > max_size {
        return Err(failure("update_download_failed"));
    }
    Ok(())
}

fn stage_directory() -> Result<PathBuf> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let path = env::temp_dir().join(format!("ftw-update-{}-{stamp}", std::process::id()));
    fs::create_dir(&path)?;
    Ok(path)
}

fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(failure("update_metadata_too_large"));
    }
    Ok(bytes)
}

fn verified_release(current: &str, directory: &Path) -> Result<Option<VerifiedRelease>> {
    let release_path = directory.join("release.json");
    download(RELEASE_API, &release_path, 1024 * 1024)?;
    let release: GithubRelease =
        serde_json::from_slice(&read_limited(&release_path, 1024 * 1024)?)?;
    let release_version = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )?;
    let current_version = Version::parse(current)?;
    if release.draft
        || release.prerelease
        || !release_version.pre.is_empty()
        || !release_version.build.is_empty()
        || release_version <= current_version
    {
        return Ok(None);
    }

    let prefix = format!("{REPOSITORY}/releases/download/{}/", release.tag_name);
    let asset_url = |name: &str| -> Result<String> {
        let item = release
            .assets
            .iter()
            .find(|item| item.name == name)
            .ok_or_else(|| failure("update_asset_missing"))?;
        if item.browser_download_url != format!("{prefix}{name}") {
            return Err(failure("update_asset_invalid_url"));
        }
        Ok(item.browser_download_url.clone())
    };
    let manifest_path = directory.join(MANIFEST_NAME);
    let signature_path = directory.join(SIGNATURE_NAME);
    download(&asset_url(MANIFEST_NAME)?, &manifest_path, 64 * 1024)?;
    download(&asset_url(SIGNATURE_NAME)?, &signature_path, 4096)?;
    let contents = read_limited(&manifest_path, 64 * 1024)?;
    let signature =
        STANDARD.decode(String::from_utf8(read_limited(&signature_path, 4096)?)?.trim())?;
    let key = STANDARD
        .decode(PUBLIC_KEY.trim())
        .map_err(|_| failure("update_key_unavailable"))?;
    signature::UnparsedPublicKey::new(&signature::ED25519, key)
        .verify(&contents, &signature)
        .map_err(|_| failure("update_signature_invalid"))?;
    let manifest: Manifest = serde_json::from_slice(&contents)?;
    if Version::parse(&manifest.version)? != release_version
        || manifest.platform != "windows-x86_64"
        || manifest.asset != ARCHIVE_NAME
        || manifest.sha256.len() != 64
        || !manifest.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(failure("update_manifest_invalid"));
    }
    let archive_url = asset_url(ARCHIVE_NAME)?;
    Ok(Some(VerifiedRelease {
        info: UpdateInfo {
            version: manifest.version.clone(),
            notes: release
                .body
                .unwrap_or_default()
                .chars()
                .take(4000)
                .collect(),
            release_url: format!("{REPOSITORY}/releases/tag/{}", release.tag_name),
        },
        archive_url,
        manifest,
    }))
}

pub fn check_latest(current: &str) -> Result<Option<UpdateInfo>> {
    let directory = stage_directory()?;
    let result = verified_release(current, &directory).map(|release| release.map(|item| item.info));
    for name in ["release.json", MANIFEST_NAME, SIGNATURE_NAME] {
        let _ = fs::remove_file(directory.join(name));
    }
    let _ = fs::remove_dir(&directory);
    result
}

pub fn sha256(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut context = digest::Context::new(&digest::SHA256);
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let length = file.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        context.update(&buffer[..length]);
    }
    Ok(context
        .finish()
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

pub fn prepare_latest(current: &str, expected: &str) -> Result<PreparedUpdate> {
    let directory = stage_directory()?;
    let release =
        verified_release(current, &directory)?.ok_or_else(|| failure("update_not_available"))?;
    if release.info.version != expected {
        return Err(failure("update_version_changed"));
    }
    let archive = directory.join(ARCHIVE_NAME);
    download(&release.archive_url, &archive, 512 * 1024 * 1024)?;
    if !sha256(&archive)?.eq_ignore_ascii_case(&release.manifest.sha256) {
        return Err(failure("update_checksum_invalid"));
    }
    let contents = directory.join("files");
    fs::create_dir(&contents)?;
    extract_release(&archive, &contents)?;
    for name in ["release.json", MANIFEST_NAME, SIGNATURE_NAME, ARCHIVE_NAME] {
        let _ = fs::remove_file(directory.join(name));
    }
    Ok(PreparedUpdate {
        directory: contents,
        version: release.info.version,
    })
}

fn extract_release(archive: &Path, destination: &Path) -> Result<()> {
    let listing = Command::new(system_tool("tar.exe"))
        .creation_flags(CREATE_NO_WINDOW)
        .arg("-tf")
        .arg(archive)
        .output()?;
    if !listing.status.success() {
        return Err(failure("update_archive_invalid"));
    }
    let listing = String::from_utf8(listing.stdout)?;
    let mut selected = Vec::new();
    let mut root: Option<String> = None;
    for line in listing.lines() {
        let path = Path::new(line);
        if path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(failure("update_archive_invalid"));
        }
        let parts: Vec<_> = line.split('/').collect();
        if parts.len() == 2 && UPDATE_FILES.contains(&parts[1]) {
            if root.as_deref().is_some_and(|value| value != parts[0]) {
                return Err(failure("update_archive_invalid"));
            }
            root = Some(parts[0].to_owned());
            selected.push((parts[1].to_owned(), line.to_owned()));
        }
    }
    for required in [
        "workbench.exe",
        "ftw-core.exe",
        "ftw-mcp.exe",
        "FTW-Update.exe",
    ] {
        if selected.iter().filter(|(name, _)| name == required).count() != 1 {
            return Err(failure("update_archive_invalid"));
        }
    }
    let mut command = Command::new(system_tool("tar.exe"));
    command
        .creation_flags(CREATE_NO_WINDOW)
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(destination)
        .arg("--strip-components=1");
    for (_, path) in &selected {
        command.arg(path);
    }
    if !command.status()?.success() {
        return Err(failure("update_archive_invalid"));
    }
    for (name, _) in selected {
        if !destination.join(name).is_file() {
            return Err(failure("update_archive_invalid"));
        }
    }
    Ok(())
}
