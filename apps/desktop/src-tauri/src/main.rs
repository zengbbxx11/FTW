#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod diagnostics;
mod file_picker;
use ftw_ipc::mail;

use ftw_ipc::{
    crm,
    diagnostics::{Component, Log},
    Health, Operation, PipeEndpoint, PipeOpenError, Profile, Response, CORE_ALREADY_RUNNING,
};
use serde::{Deserialize, Serialize};
use std::{
    env, io,
    path::{Path, PathBuf},
    process::{ExitCode, Stdio},
    time::Duration,
};
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};
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

struct CoreAccess {
    gate: Mutex<()>,
    diagnostics: Log,
}

#[derive(Serialize)]
struct Issue {
    code: String,
    detail: String,
    #[serde(flatten)]
    guidance: ftw_ipc::receipt::Guidance,
}

impl Issue {
    fn new(code: impl Into<String>, detail: impl ToString) -> Self {
        let code = code.into();
        Self {
            guidance: ftw_ipc::receipt::guidance(&code),
            code,
            detail: detail.to_string(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopHealth {
    #[serde(flatten)]
    core: Health,
    webview_version: Option<String>,
    architecture: &'static str,
}

#[tauri::command]
async fn core_health(access: State<'_, CoreAccess>) -> Result<DesktopHealth, Issue> {
    match call_core(&access, &Operation::Health).await? {
        Response::Ready { health } => Ok(DesktopHealth {
            core: health,
            webview_version: tauri::webview_version().ok(),
            architecture: env::consts::ARCH,
        }),
        _ => Err(Issue::new("core_protocol", "Unexpected Core response")),
    }
}

#[tauri::command]
async fn mcp_status(access: State<'_, CoreAccess>) -> Result<diagnostics::McpStatus, Issue> {
    let result = diagnostics::check_mcp().await;
    access.diagnostics.event(
        "mcp_probe",
        result
            .as_ref()
            .map_or_else(|error| error.code.as_str(), |_| "ok"),
    );
    result
}

#[tauri::command]
async fn export_support_report(window: tauri::WebviewWindow) -> Result<Option<String>, Issue> {
    let Some(destination) = choose_export_path(&window, "FTW-diagnostics.json".to_owned()).await?
    else {
        return Ok(None);
    };
    if !Path::new(&destination)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    {
        return Err(Issue::new(
            "invalid_diagnostics_extension",
            "Save the report as a JSON file",
        ));
    }
    tokio::task::spawn_blocking(move || -> io::Result<String> {
        let report = ftw_ipc::diagnostics::report(&env::current_exe()?, development_root())?;
        std::fs::write(&destination, report)?;
        Ok(destination)
    })
    .await
    .map_err(|_| Issue::new("diagnostics_export_failed", "Report worker unavailable"))?
    .map(Some)
    .map_err(|_| {
        Issue::new(
            "diagnostics_export_failed",
            "Could not write diagnostics report",
        )
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum WebviewFailure {
    ScriptError,
    UnhandledRejection,
}

#[tauri::command]
async fn report_webview_failure(
    access: State<'_, CoreAccess>,
    failure: WebviewFailure,
) -> Result<(), ()> {
    let code = match failure {
        WebviewFailure::ScriptError => "script_error",
        WebviewFailure::UnhandledRejection => "unhandled_rejection",
    };
    access.diagnostics.event("webview", code);
    Ok(())
}

#[tauri::command]
async fn core_changes(access: State<'_, CoreAccess>) -> Result<ftw_ipc::ChangeVersions, Issue> {
    match call_core(&access, &Operation::Changes).await? {
        Response::Changes { versions } => Ok(versions),
        _ => Err(Issue::new("core_protocol", "Unexpected change response")),
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

#[tauri::command]
async fn choose_mail_assets(
    window: tauri::WebviewWindow,
    access: tauri::State<'_, CoreAccess>,
) -> Result<Vec<mail::Asset>, Issue> {
    let paths = choose_local_files(&window).await?;
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    match call_core(
        &access,
        &Operation::Mail(Box::new(mail::Command::ImportAssets { paths })),
    )
    .await?
    {
        Response::Mail { data } => match *data {
            mail::Result::Assets { rows } => Ok(rows),
            _ => Err(Issue::new(
                "core_protocol",
                "Unexpected attachment response",
            )),
        },
        _ => Err(Issue::new(
            "core_protocol",
            "Unexpected attachment response",
        )),
    }
}

async fn choose_local_files(window: &tauri::WebviewWindow) -> Result<Vec<String>, Issue> {
    let handle = window
        .hwnd()
        .map_err(|error| Issue::new("attachment_picker_failed", error))?
        .0 as isize;
    tokio::task::spawn_blocking(move || file_picker::choose(handle))
        .await
        .map_err(|error| Issue::new("attachment_picker_failed", error))?
        .map_err(|error| Issue::new("attachment_picker_failed", error))
}

#[tauri::command]
async fn import_customer_documents(
    window: tauri::WebviewWindow,
    access: State<'_, CoreAccess>,
    contact_id: String,
) -> Result<Option<usize>, Issue> {
    let paths = choose_local_files(&window).await?;
    if paths.is_empty() {
        return Ok(None);
    }
    match call_core(
        &access,
        &Operation::Mail(Box::new(mail::Command::ImportCustomerDocuments {
            contact_id,
            paths,
        })),
    )
    .await?
    {
        Response::Mail { data } => match *data {
            mail::Result::ImportedDocuments { count } => Ok(Some(count)),
            _ => Err(Issue::new("core_protocol", "Unexpected document response")),
        },
        _ => Err(Issue::new("core_protocol", "Unexpected document response")),
    }
}

#[tauri::command]
async fn mail_request(
    window: tauri::WebviewWindow,
    access: State<'_, CoreAccess>,
    command: ftw_ipc::mail::Command,
) -> Result<Box<ftw_ipc::mail::Result>, Issue> {
    if matches!(
        command,
        mail::Command::ImportAssets { .. }
            | mail::Command::ImportCustomerDocuments { .. }
            | mail::Command::ExportCustomerDocument { .. }
            | mail::Command::ExportOutboundAttachment { .. }
    ) {
        return Err(Issue::new(
            "attachment_picker_failed",
            "A Windows file dialog is required",
        ));
    }
    let owner = window
        .hwnd()
        .map_err(|error| Issue::new("mail_window_unavailable", error))?
        .0 as isize;
    let operation = Operation::DesktopMail {
        owner,
        command: Box::new(command),
    };
    match call_core(&access, &operation).await? {
        Response::Mail { data } => Ok(data),
        _ => Err(Issue::new("core_protocol", "Unexpected mail response")),
    }
}

#[tauri::command]
async fn save_inbox_attachment(
    window: tauri::WebviewWindow,
    access: State<'_, CoreAccess>,
    id: String,
    index: u16,
    filename: String,
) -> Result<bool, Issue> {
    let Some(destination) = choose_export_path(&window, filename).await? else {
        return Ok(false);
    };
    match call_core(
        &access,
        &Operation::Inbox(Box::new(ftw_ipc::inbox::Command::SaveAttachment {
            id,
            index,
            destination,
        })),
    )
    .await?
    {
        Response::Inbox { data } if matches!(*data, ftw_ipc::inbox::Result::Saved { .. }) => {
            Ok(true)
        }
        _ => Err(Issue::new(
            "core_protocol",
            "Unexpected attachment response",
        )),
    }
}

#[tauri::command]
async fn save_outbound_attachment(
    window: tauri::WebviewWindow,
    access: State<'_, CoreAccess>,
    id: String,
    index: u16,
    filename: String,
) -> Result<bool, Issue> {
    let Some(destination) = choose_export_path(&window, filename).await? else {
        return Ok(false);
    };
    match call_core(
        &access,
        &Operation::Mail(Box::new(mail::Command::ExportOutboundAttachment {
            id,
            index,
            destination,
        })),
    )
    .await?
    {
        Response::Mail { data } if matches!(*data, mail::Result::Saved { .. }) => Ok(true),
        _ => Err(Issue::new(
            "core_protocol",
            "Unexpected attachment response",
        )),
    }
}

#[tauri::command]
async fn export_customer_document(
    window: tauri::WebviewWindow,
    access: State<'_, CoreAccess>,
    id: String,
    filename: String,
) -> Result<bool, Issue> {
    let Some(destination) = choose_export_path(&window, filename).await? else {
        return Ok(false);
    };
    match call_core(
        &access,
        &Operation::Mail(Box::new(mail::Command::ExportCustomerDocument {
            id,
            destination,
        })),
    )
    .await?
    {
        Response::Mail { data } if matches!(*data, mail::Result::Saved { .. }) => Ok(true),
        _ => Err(Issue::new("core_protocol", "Unexpected document response")),
    }
}

async fn choose_export_path(
    window: &tauri::WebviewWindow,
    filename: String,
) -> Result<Option<String>, Issue> {
    let handle = window
        .hwnd()
        .map_err(|error| Issue::new("attachment_picker_failed", error))?
        .0 as isize;
    let filename = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("attachment")
        .replace([':', '*', '?', '"', '<', '>', '|', '\r', '\n'], "_");
    tokio::task::spawn_blocking(move || file_picker::save(handle, &filename))
        .await
        .map_err(|error| Issue::new("attachment_picker_failed", error))?
        .map_err(|error| Issue::new("attachment_picker_failed", error))
}

#[tauri::command]
async fn inbox_request(
    access: State<'_, CoreAccess>,
    command: ftw_ipc::inbox::Command,
) -> Result<Box<ftw_ipc::inbox::Result>, Issue> {
    match call_core(&access, &Operation::Inbox(Box::new(command))).await? {
        Response::Inbox { data } => Ok(data),
        _ => Err(Issue::new("core_protocol", "Unexpected inbox response")),
    }
}

#[tauri::command]
async fn data_request(
    access: State<'_, CoreAccess>,
    command: ftw_ipc::data::Command,
) -> Result<Box<ftw_ipc::data::Result>, Issue> {
    match call_core(&access, &Operation::Data(Box::new(command))).await? {
        Response::Data { data } => Ok(data),
        _ => Err(Issue::new("core_protocol", "Unexpected data response")),
    }
}

async fn call_core(access: &CoreAccess, operation: &Operation) -> Result<Response, Issue> {
    let _guard = access.gate.lock().await;
    let result = call_profile_core(operation).await;
    if let Err(error) = &result {
        access
            .diagnostics
            .event(operation.diagnostic_group(), &error.code);
    }
    result
}

async fn call_profile_core(operation: &Operation) -> Result<Response, Issue> {
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
    let Ok(executable) = env::current_exe() else {
        return ExitCode::FAILURE;
    };
    let diagnostics = Log::new(&executable, development_root(), Component::Desktop);
    diagnostics.install_panic_hook();
    diagnostics.event("startup", "starting");
    if tauri::webview_version().is_err() {
        diagnostics.event("startup", "webview2_unavailable");
        show_startup_error(
            "未找到 Microsoft Edge WebView2 Runtime。安装 Evergreen Runtime 后，请重新打开 FTW。",
        );
        return ExitCode::FAILURE;
    }

    let window_log = diagnostics.clone();
    let app = tauri::Builder::default()
        .manage(CoreAccess {
            gate: Mutex::new(()),
            diagnostics: diagnostics.clone(),
        })
        .invoke_handler(tauri::generate_handler![
            core_health,
            mcp_status,
            export_support_report,
            report_webview_failure,
            core_changes,
            crm_request,
            mail_request,
            choose_mail_assets,
            import_customer_documents,
            export_customer_document,
            inbox_request,
            save_inbox_attachment,
            save_outbound_attachment,
            data_request
        ])
        .setup(move |app| {
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
                window_log.event("startup", "window_creation_failed");
                show_startup_error(&format!("无法打开 FTW 窗口。\n{error}"));
                app.handle().exit(1);
            }
            if app.get_webview_window("main").is_some() {
                window_log.event("startup", "window_ready");
            }
            Ok(())
        })
        .run(tauri::generate_context!());

    if let Err(error) = app {
        diagnostics.event("process", "tauri_run_failed");
        show_startup_error(&format!("无法打开 FTW 窗口。\n{error}"));
        return ExitCode::FAILURE;
    }
    diagnostics.event("process", "stopped");
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
