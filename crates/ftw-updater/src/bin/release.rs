use std::{
    env,
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};
use windows_sys::Win32::System::Console::{
    GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_ECHO_INPUT, STD_INPUT_HANDLE,
};

fn password() -> ftw_updater::Result<String> {
    let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    let mut mode = 0;
    if unsafe { GetConsoleMode(handle, &mut mode) } == 0 {
        return Err("a Windows console is required".into());
    }
    eprint!("Backup password (at least 12 characters): ");
    io::stderr().flush()?;
    if unsafe { SetConsoleMode(handle, mode & !ENABLE_ECHO_INPUT) } == 0 {
        return Err("could not hide password input".into());
    }
    let mut value = String::new();
    let read = io::stdin().read_line(&mut value);
    unsafe { SetConsoleMode(handle, mode) };
    eprintln!();
    read?;
    Ok(value.trim_end_matches(['\r', '\n']).to_owned())
}

fn run() -> ftw_updater::Result<()> {
    let mut args = env::args_os().skip(1);
    match args.next().and_then(|value| value.into_string().ok()).as_deref() {
        Some("init-key") => {
            let private = PathBuf::from(args.next().ok_or("missing protected key path")?);
            let public = PathBuf::from(args.next().ok_or("missing public key path")?);
            ftw_updater::initialize_key(&private, &public)?;
            println!("Release signing key initialized. Keep the protected key and its Windows account safe.");
        }
        Some("sign") => {
            let archive = PathBuf::from(args.next().ok_or("missing archive path")?);
            let version = args.next().ok_or("missing version")?.into_string().map_err(|_| "invalid version")?;
            let private = PathBuf::from(args.next().ok_or("missing protected key path")?);
            ftw_updater::sign_release(&archive, &version, &private)?;
            println!("Release manifest and signature generated.");
        }
        Some("backup") => {
            let private = PathBuf::from(args.next().ok_or("missing protected key path")?);
            let destination = PathBuf::from(args.next().ok_or("missing encrypted backup path")?);
            let secret = password()?;
            eprint!("Enter the same password again. ");
            let repeated = password()?;
            if secret != repeated { return Err("passwords do not match".into()); }
            ftw_updater::backup_key(&private, &destination, &secret)?;
            println!("Encrypted signing key backup created.");
        }
        Some("restore") => {
            let backup = PathBuf::from(args.next().ok_or("missing encrypted backup path")?);
            let destination = PathBuf::from(args.next().ok_or("missing protected key path")?);
            ftw_updater::restore_key(&backup, &destination, &password()?)?;
            println!("Signing key restored for the current Windows account.");
        }
        _ => return Err("usage: ftw-release init-key PRIVATE PUBLIC | sign ARCHIVE VERSION PRIVATE | backup PRIVATE BACKUP | restore BACKUP PRIVATE".into()),
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Release preparation failed: {error}");
            ExitCode::FAILURE
        }
    }
}
