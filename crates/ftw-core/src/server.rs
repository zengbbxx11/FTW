use crate::{crm, mail::MailService, storage, CoreError};
use ftw_ipc::{
    diagnostics::Log, display_path, read_frame, write_frame, Health, Operation, PipeEndpoint,
    Profile, Request, Response, PROTOCOL_VERSION,
};
use sqlx::SqlitePool;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::windows::fs::OpenOptionsExt,
    process,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{io::AsyncReadExt, net::windows::named_pipe::NamedPipeServer};

const IDLE_LIMIT: Duration = Duration::from_secs(300);
const REQUEST_LIMIT: Duration = Duration::from_secs(10);
const ERROR_SHARING_VIOLATION: i32 = 32;
const ERROR_PIPE_NOT_CONNECTED: i32 = 233;

pub async fn serve(profile: Profile, diagnostics: &Log, idle_exit: bool) -> Result<(), CoreError> {
    let _lock = lock_data(&profile)?;
    let log_dir = profile.root.join("logs");
    let _ = fs::create_dir_all(&log_dir);
    let old_path = log_dir.join("core.log");
    if fs::metadata(&old_path).is_ok_and(|info| info.len() >= 256 * 1024) {
        let previous = log_dir.join("core.previous.log");
        let _ = fs::remove_file(&previous);
        let _ = fs::rename(&old_path, previous);
    }
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(old_path)
        .ok();

    let result = run(&profile, &mut log, diagnostics, idle_exit).await;
    if let Err(error) = &result {
        log_event(
            &mut log,
            &format!("stopped with error: {}", error.diagnostic_code()),
        );
    }
    result
}

fn lock_data(profile: &Profile) -> Result<File, CoreError> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(profile.data.join("core.lock"))
        .map_err(|error| {
            if error.raw_os_error() == Some(ERROR_SHARING_VIOLATION) {
                return CoreError::AlreadyRunning;
            }
            CoreError::Io("data_unavailable", error)
        })
}

async fn run(
    profile: &Profile,
    log: &mut Option<File>,
    diagnostics: &Log,
    idle_exit: bool,
) -> Result<(), CoreError> {
    let database = storage::open(&profile.data, &profile.root).await?;
    let endpoint = PipeEndpoint::for_data_directory(&profile.data)
        .map_err(|error| CoreError::Io("pipe_unavailable", error))?;
    let mut pipe = endpoint
        .listen()
        .map_err(|error| CoreError::Io("pipe_unavailable", error))?;

    let schema = storage::schema_version(&database).await?;
    log_event(log, &format!("ready schema_version={schema}"));

    let mail = MailService::new(database.clone())
        .await?
        .with_send_ledger(&profile.data)
        .await
        .with_diagnostics(diagnostics.clone());
    diagnostics.ready(schema);
    let worker = mail.start_worker();
    let result = run_requests(&mut pipe, &database, &mail, profile, diagnostics, idle_exit).await;
    worker.abort();
    let _ = worker.await;
    drop(pipe);
    database.close().await;
    result?;
    log_event(log, "stopped");
    diagnostics.event("process", "stopped");
    Ok(())
}

async fn run_requests(
    pipe: &mut NamedPipeServer,
    database: &SqlitePool,
    mail: &MailService,
    profile: &Profile,
    diagnostics: &Log,
    idle_exit: bool,
) -> Result<(), CoreError> {
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    loop {
        let idle = tokio::time::sleep(IDLE_LIMIT);
        tokio::select! {
            signal = &mut shutdown => {
                signal.map_err(|error| CoreError::Io("shutdown_unavailable", error))?;
                return Ok(());
            },
            () = idle, if idle_exit => { if !mail.approval_in_progress() { return Ok(()); } continue; },
            client = pipe.connect() => client.map_err(|error| CoreError::Io("pipe_unavailable", error))?,
        }

        tokio::select! {
            signal = &mut shutdown => {
                signal.map_err(|error| CoreError::Io("shutdown_unavailable", error))?;
                return Ok(());
            },
            result = tokio::time::timeout(REQUEST_LIMIT, respond(pipe, database, mail, profile, diagnostics)) => {
                match result {
                    Err(_) => diagnostics.event("ipc", "request_timeout"),
                    Ok(Err(error)) if error.raw_os_error() != Some(ERROR_PIPE_NOT_CONNECTED)
                        && error.kind() != io::ErrorKind::UnexpectedEof => diagnostics.event("ipc", "exchange_failed"),
                    _ => {}
                }
            }
        }

        match pipe.disconnect() {
            Ok(()) => {}
            Err(error) if error.raw_os_error() == Some(ERROR_PIPE_NOT_CONNECTED) => {}
            Err(error) => return Err(CoreError::Io("pipe_unavailable", error)),
        }
    }
}

async fn respond(
    pipe: &mut NamedPipeServer,
    database: &SqlitePool,
    mail: &MailService,
    profile: &Profile,
    diagnostics: &Log,
) -> io::Result<()> {
    let mut group = "ipc";
    let response = match read_frame::<Request>(pipe).await {
        Ok(request) if request.version == PROTOCOL_VERSION => {
            group = request.operation.diagnostic_group();
            match request.operation {
                Operation::Health => match storage::schema_version(database).await {
                    Ok(schema_version) => Response::Ready {
                        health: Health {
                            core_version: env!("CARGO_PKG_VERSION").to_owned(),
                            schema_version,
                            storage_mode: profile.mode,
                            data_root: display_path(&profile.data),
                            process_id: process::id(),
                        },
                    },
                    Err(_) => error_response("database_unavailable"),
                },
                Operation::Changes => match storage::changes(database).await {
                    Ok(versions) => Response::Changes { versions },
                    Err(_) => error_response("database_unavailable"),
                },
                Operation::Crm(command) => {
                    crm::dispatch(database, *command, crm::Actor::DESKTOP).await
                }
                Operation::Mail(command) => mail.desktop(*command, 0).await,
                Operation::DesktopMail { owner, command } => mail.desktop(*command, owner).await,
                Operation::Agent(command) => mail.agent(*command).await,
                Operation::RegisteredAgentMail { token, command } => {
                    mail.agent_session(Some(&token.0), *command).await
                }
                Operation::Inbox(command) => mail.inbox(*command).await,
                Operation::AgentInbox(command) => mail.inbox_session(None, *command).await,
                Operation::RegisteredAgentInbox { token, command } => {
                    mail.inbox_session(Some(&token.0), *command).await
                }
                Operation::AgentCrm(command) => mail.collaborate_crm(None, *command).await,
                Operation::RegisteredAgentCrm { token, command } => {
                    mail.collaborate_crm(Some(&token.0), *command).await
                }
                Operation::AgentCollaboration { token, command } => {
                    mail.collaborate(&token.0, *command).await
                }
                Operation::Data(command) => {
                    crate::data::dispatch(database, mail, profile, *command).await
                }
            }
        }
        Ok(_) => error_response("unsupported_protocol_version"),
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            error_response("invalid_request")
        }
        Err(error) => return Err(error),
    };

    if let Response::Error { code, .. } = &response {
        diagnostics.event(group, code);
    }

    write_frame(pipe, &response).await?;
    // DisconnectNamedPipe can discard unread bytes. Wait until the client has the response.
    let _ = tokio::time::timeout(Duration::from_millis(400), pipe.read_u8()).await;
    Ok(())
}

fn error_response(code: &str) -> Response {
    Response::Error {
        code: code.into(),
        detail: None,
    }
}

fn log_event(file: &mut Option<File>, event: &str) {
    let Some(file) = file else {
        return;
    };
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let _ = writeln!(file, "unix_ms={timestamp} pid={} {event}", process::id());
}
