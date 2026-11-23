#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use ftw_ipc::{
    crm, Health, Operation, PipeEndpoint, PipeOpenError, Profile, Response, CORE_ALREADY_RUNNING,
};
use serde::Serialize;
use std::{
    env, io,
    path::{Path, PathBuf},
    process::{ExitCode, Stdio},
    time::Duration,
};
use tauri::{State, WebviewUrl, WebviewWindowBuilder};
use tokio::{
    io::AsyncReadExt,
    process::{Child, Command},
    sync::Mutex,
    time::{sleep, timeout, Instant},
};
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

const CREATE_NO_WINDOW: u32 = 0x08000000;
const STARTUP_LIMIT: Duration = Duration::from_secs(15);
const QUERY_LIMIT: Duration = Duration::from_secs(12);

#[derive(Default)]
struct CoreAccess(Mutex<()>);

#[derive(Serialize)]
struct Issue {
    code: String,
    detail: String,
}

impl Issue {
    fn new(code: impl Into<String>, detail: impl ToString) -> Self {
        Self {
            code: code.into(),
            detail: detail.to_string(),
        }
    }
}

#[tauri::command]
async fn core_health(access: State<'_, CoreAccess>) -> Result<Health, Issue> {
    match call_core(&access, &Operation::Health).await? {
        Response::Ready { health } => Ok(health),
        _ => Err(Issue::new("core_protocol", "Unexpected Core response")),
    }
}

#[tauri::command]
async fn crm_request(
    access: State<'_, CoreAccess>,
    command: crm::Command,
) -> Result<Box<crm::Result>, Issue> {
    match call_core(&access, &Operation::Crm(Box::new(command))).await? {
        Response::Crm { data } => Ok(data),
        _ => Err(Issue::new("core_protocol", "Unexpected CRM response")),
    }
}

async fn call_core(access: &CoreAccess, operation: &Operation) -> Result<Response, Issue> {
    let _guard = access.0.lock().await;
    let exe = env::current_exe().map_err(|error| Issue::new("profile_unavailable", error))?;
    let dev_root = development_root();
    let profile = Profile::resolve(&exe, dev_root.clone())
        .map_err(|error| Issue::new("profile_unavailable", error))?;
    let endpoint = PipeEndpoint::for_data_directory(&profile.data)
        .map_err(|error| Issue::new("pipe_unavailable", error))?;
    connect(
        &endpoint,
        &exe.with_file_name("ftw-core.exe"),
        dev_root.as_deref(),
        operation,
    )
    .await
}

fn development_root() -> Option<PathBuf> {
    if cfg!(debug_assertions) {
        return env::var_os("FTW_DEV_ROOT").map(PathBuf::from);
    }
    None
}

async fn connect(
    endpoint: &PipeEndpoint,
    core_exe: &Path,
    dev_root: Option<&Path>,
    operation: &Operation,
) -> Result<Response, Issue> {
    let deadline = Instant::now() + STARTUP_LIMIT;
    let mut started = false;
    let mut child: Option<Child> = None;

    loop {
        let time_left = deadline.saturating_duration_since(Instant::now());
        if time_left.is_zero() {
            return Err(Issue::new(
                "core_timeout",
                "Core startup exceeded 15 seconds",
            ));
        }

        match timeout(time_left.min(QUERY_LIMIT), endpoint.request(operation)).await {
            Ok(Ok(Response::Error { code, detail })) => {
                return Err(Issue::new(code, detail.unwrap_or_default()))
            }
            Ok(Ok(response)) => return Ok(response),
            Ok(Err(PipeOpenError::Absent)) if !started => {
                child = Some(spawn_core(core_exe, dev_root)?);
                started = true;
            }
            Ok(Err(PipeOpenError::Absent)) => {}
            Ok(Err(PipeOpenError::Io(error))) => return Err(Issue::new("pipe_unavailable", error)),
            Err(_) => {
                return Err(Issue::new(
                    "core_timeout",
                    "Core did not answer the request",
                ))
            }
        }

        if let Some(process) = child.as_mut() {
            if let Some(status) = process
                .try_wait()
                .map_err(|error| Issue::new("core_start_failed", error))?
            {
                if status.code() != Some(CORE_ALREADY_RUNNING) {
                    let mut detail = String::new();
                    if let Some(stderr) = process.stderr.take() {
                        let _ = stderr.take(4096).read_to_string(&mut detail).await;
                    }
                    if detail.trim().is_empty() {
                        detail = format!("Core process exited with {status}");
                    }
                    return Err(Issue::new("core_start_failed", detail.trim()));
                }

                // Another launcher acquired the data lock. Keep waiting for its pipe.
                child = None;
            }
        }

        sleep(Duration::from_millis(120)).await;
    }
}

fn spawn_core(executable: &Path, dev_root: Option<&Path>) -> Result<Child, Issue> {
    let mut command = Command::new(executable);
    command.args(["serve", "--idle-exit"]);
    if let Some(path) = dev_root {
        command.arg("--dev-root").arg(path);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|error| {
            let code = if error.kind() == io::ErrorKind::NotFound {
                "core_missing"
            } else {
                "core_start_failed"
            };
            Issue::new(
                code,
                format!("{}: {error}", ftw_ipc::display_path(executable)),
            )
        })
}

fn main() -> ExitCode {
    if tauri::webview_version().is_err() {
        show_startup_error(
            "未找到 Microsoft Edge WebView2 Runtime。安装 Evergreen Runtime 后，请重新打开 FTW。",
        );
        return ExitCode::FAILURE;
    }

    let app = tauri::Builder::default()
        .manage(CoreAccess::default())
        .invoke_handler(tauri::generate_handler![core_health, crm_request])
        .setup(|app| {
            let mut window =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title("FTW 外贸智能工作台")
                    .inner_size(1000.0, 710.0)
                    .min_inner_size(540.0, 520.0);
            if let Some(root) = development_root() {
                // WebView2 cannot share an active profile between environments with different options.
                window = window.data_directory(root.join("webview"));
            }
            if let Err(error) = window.build() {
                show_startup_error(&format!("无法打开 FTW 窗口。\n{error}"));
                app.handle().exit(1);
            }
            Ok(())
        })
        .run(tauri::generate_context!());

    if let Err(error) = app {
        show_startup_error(&format!("无法打开 FTW 窗口。\n{error}"));
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn show_startup_error(message: &str) {
    let text: Vec<_> = message.encode_utf16().chain(Some(0)).collect();
    let title: Vec<_> = "FTW 外贸智能工作台".encode_utf16().chain(Some(0)).collect();
    // The two UTF-16 buffers stay alive while Windows displays the error dialog.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        )
    };
}
