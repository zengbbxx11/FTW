use std::{
    env,
    path::{Path, PathBuf},
    process::ExitCode,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDCANCEL, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_YESNOCANCEL,
};

fn message(text: &str, flags: u32) -> i32 {
    let text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let title: Vec<u16> = "FTW 更新".encode_utf16().chain(Some(0)).collect();
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), flags) }
}

fn choose_target(source: &Path) -> ftw_updater::Result<Option<PathBuf>> {
    loop {
        let targets = ftw_updater::detect_workbenches(source)?;
        if targets.len() == 1 {
            return Ok(targets.into_iter().next());
        }
        if targets.is_empty() {
            let answer = message(
                "请先打开需要更新的旧版 FTW，再点击“是”重新查找。\n如果暂不更新，请点击“取消”。",
                MB_YESNOCANCEL | MB_ICONINFORMATION,
            );
            if answer != IDYES {
                return Ok(None);
            }
            continue;
        }
        for target in targets {
            let answer = message(
                &format!("是否更新这个 FTW 工作台？\n{}", target.display()),
                MB_YESNOCANCEL | MB_ICONINFORMATION,
            );
            if answer == IDYES {
                return Ok(Some(target));
            }
            if answer == IDCANCEL {
                return Ok(None);
            }
        }
    }
}

fn run() -> ftw_updater::Result<()> {
    let executable = env::current_exe()?;
    let source = executable.parent().ok_or("updater directory unavailable")?;
    let mut args = env::args_os().skip(1);
    let (target, legacy_transition) = match args
        .next()
        .and_then(|arg| arg.into_string().ok())
        .as_deref()
    {
        Some("--apply") => (
            PathBuf::from(args.next().ok_or("missing workbench directory")?),
            false,
        ),
        None => {
            let Some(target) = choose_target(source)? else {
                return Ok(());
            };
            message("已识别原工作台。请先完成邮件发送任务、保存正在编辑的内容，关闭 FTW、WorkBuddy 和其他 FTW 连接器，然后点击“确定”。\n更新程序会等待旧版邮件队列完成；后台 Core 空闲退出可能需要几分钟。", MB_OK | MB_ICONINFORMATION);
            (target, true)
        }
        _ => return Err("unsupported update command".into()),
    };
    ftw_updater::apply(&target, source, legacy_transition)?;
    message(
        "FTW 已更新并重新打开。原有数据和邮箱配置已保留。",
        MB_OK | MB_ICONINFORMATION,
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            message(&format!("更新未完成，原工作台数据未被覆盖。\n请确认 FTW 和 WorkBuddy 已关闭后重试。\n\n{error}"), MB_OK | MB_ICONERROR);
            ExitCode::FAILURE
        }
    }
}
