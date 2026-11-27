use super::{failure, Result, UPDATE_FILES};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, OpenOptions},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::OpenOptionsExt,
    },
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
    Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION},
    },
};

fn pending_mail(target: &Path) -> Result<bool> {
    let profile = ftw_ipc::Profile::resolve(&target.join("workbench.exe"), None)?;
    let endpoint = ftw_ipc::PipeEndpoint::for_data_directory(&profile.data)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let response = runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(5),
            endpoint.request(&ftw_ipc::Operation::Mail(Box::new(
                ftw_ipc::mail::Command::QueueSummary { contact_id: None },
            ))),
        )
        .await
    })??;
    match response {
        ftw_ipc::Response::Mail { data } => match *data {
            ftw_ipc::mail::Result::QueueSummary { queue, .. } => Ok(queue != 0),
            _ => Err(failure("update_queue_unavailable")),
        },
        _ => Err(failure("update_queue_unavailable")),
    }
}

fn matching_path(left: &Path, right: &Path) -> bool {
    ftw_ipc::display_path(left).eq_ignore_ascii_case(&ftw_ipc::display_path(right))
}

fn process_images(names: &[&str]) -> Result<Vec<PathBuf>> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut images = Vec::new();
    let mut found = unsafe { Process32FirstW(snapshot, &mut entry) };
    while found != 0 {
        let end = entry
            .szExeFile
            .iter()
            .position(|&value| value == 0)
            .unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
        if names
            .iter()
            .any(|candidate| name.eq_ignore_ascii_case(candidate))
        {
            let handle =
                unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.th32ProcessID) };
            if !handle.is_null() {
                let mut buffer = vec![0u16; 32768];
                let mut length = buffer.len() as u32;
                if unsafe {
                    QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut length)
                } != 0
                {
                    images.push(PathBuf::from(OsString::from_wide(
                        &buffer[..length as usize],
                    )));
                }
                unsafe { CloseHandle(handle) };
            }
        }
        found = unsafe { Process32NextW(snapshot, &mut entry) };
    }
    unsafe { CloseHandle(snapshot) };
    Ok(images)
}

fn running_in(target: &Path, names: &[&str]) -> Result<bool> {
    Ok(process_images(names)?.iter().any(|image| {
        image
            .parent()
            .is_some_and(|parent| matching_path(parent, target))
    }))
}

pub fn running_adapters(target: &Path) -> Result<bool> {
    running_in(target, &["ftw-mcp.exe"])
}

pub fn detect_workbenches(source: &Path) -> Result<Vec<PathBuf>> {
    let mut found = BTreeMap::new();
    for image in process_images(&["workbench.exe"])? {
        if let Some(directory) = image.parent() {
            if !matching_path(directory, source)
                && directory.join("portable.flag").is_file()
                && directory.join("ftw-core.exe").is_file()
                && directory.join("ftw-mcp.exe").is_file()
            {
                found.insert(
                    directory.to_string_lossy().to_lowercase(),
                    directory.to_owned(),
                );
            }
        }
    }
    Ok(found.into_values().collect())
}

fn replace(source: &Path, destination: &Path) -> Result<()> {
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

pub fn apply(target: &Path, source: &Path, legacy_transition: bool) -> Result<()> {
    let target = fs::canonicalize(target)?;
    let source = fs::canonicalize(source)?;
    if matching_path(&target, &source) || !target.join("portable.flag").is_file() {
        return Err(failure("update_target_invalid"));
    }
    for name in [
        "workbench.exe",
        "ftw-core.exe",
        "ftw-mcp.exe",
        "FTW-Update.exe",
    ] {
        if !source.join(name).is_file() {
            return Err(failure("update_file_missing"));
        }
    }
    println!("正在等待 FTW、连接器和后台退出，现有数据会保留。\n旧版后台空闲退出可能需要几分钟，请保持此窗口打开。");
    let deadline = Instant::now() + Duration::from_secs(600);
    let mut next_notice = Instant::now() + Duration::from_secs(30);
    let mut checked_queue = !legacy_transition;
    let lock = loop {
        let clients = running_in(&target, &["workbench.exe", "ftw-mcp.exe"])?;
        let core = running_in(&target, &["ftw-core.exe"])?;
        if clients && legacy_transition {
            checked_queue = false;
        } else if !clients && core && !checked_queue {
            if let Ok(false) = pending_mail(&target) {
                checked_queue = true;
                println!("邮件队列已完成，正在等待旧版后台退出。");
            }
        }
        if !clients && !core {
            let lock_path = target.join("data").join("core.lock");
            if let Ok(lock) = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .share_mode(0)
                .open(lock_path)
            {
                if !running_in(&target, &["workbench.exe", "ftw-mcp.exe", "ftw-core.exe"])? {
                    break lock;
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(failure("update_processes_busy"));
        }
        if Instant::now() >= next_notice {
            println!("仍在等待：请确认 FTW、WorkBuddy 和其他连接器已关闭，发送队列已完成。");
            next_notice = Instant::now() + Duration::from_secs(30);
        }
        thread::sleep(Duration::from_secs(2));
    };

    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let backup = target
        .join("backups")
        .join(format!("program-update-{stamp}-{}", std::process::id()));
    fs::create_dir_all(&backup)?;
    let mut files = Vec::new();
    for name in UPDATE_FILES {
        let next = source.join(name);
        if !next.is_file() {
            continue;
        }
        let old = target.join(name);
        let existed = old.is_file();
        if existed {
            fs::copy(&old, backup.join(name))?;
        }
        let pending = target.join(format!(".ftw-update-{}-{name}", std::process::id()));
        fs::copy(&next, &pending)?;
        files.push((*name, existed, pending));
    }
    let mut installed: Vec<(&str, bool)> = Vec::new();
    for (name, existed, pending) in &files {
        if let Err(error) = replace(pending, &target.join(name)) {
            for (previous, had_file) in installed.iter().rev() {
                if *had_file {
                    let rollback =
                        target.join(format!(".ftw-rollback-{}-{previous}", std::process::id()));
                    let _ = fs::copy(backup.join(previous), &rollback).and_then(|_| {
                        replace(&rollback, &target.join(previous)).map_err(std::io::Error::other)
                    });
                } else {
                    let _ = fs::remove_file(target.join(previous));
                }
            }
            return Err(error);
        }
        installed.push((name, *existed));
    }
    drop(lock);
    Command::new(target.join("workbench.exe"))
        .current_dir(&target)
        .spawn()?;
    Ok(())
}
