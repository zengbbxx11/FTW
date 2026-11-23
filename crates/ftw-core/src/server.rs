use crate::{storage, CoreError};
use ftw_ipc::{
    display_path, read_frame, write_frame, Health, Operation, PipeEndpoint, Profile, Request,
    Response, PROTOCOL_VERSION,
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
const REQUEST_LIMIT: Duration = Duration::from_secs(2);
const ERROR_SHARING_VIOLATION: i32 = 32;
const ERROR_PIPE_NOT_CONNECTED: i32 = 233;

pub async fn serve(profile: Profile, idle_exit: bool) -> Result<(), CoreError> {
    let _lock = lock_data(&profile)?;
    let log_dir = profile.root.join("logs");
    fs::create_dir_all(&log_dir).map_err(|error| CoreError::Io("log_unavailable", error))?;
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("core.log"))
        .map_err(|error| CoreError::Io("log_unavailable", error))?;

    let result = run(&profile, &mut log, idle_exit).await;
    if let Err(error) = &result {
        let _ = log_event(&mut log, &format!("stopped with error: {error}"));
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

async fn run(profile: &Profile, log: &mut File, idle_exit: bool) -> Result<(), CoreError> {
    let database = storage::open(&profile.data).await?;
    let endpoint = PipeEndpoint::for_data_directory(&profile.data)
        .map_err(|error| CoreError::Io("pipe_unavailable", error))?;
    let mut pipe = endpoint
        .listen()
        .map_err(|error| CoreError::Io("pipe_unavailable", error))?;

    let schema = storage::schema_version(&database).await?;
    log_event(log, &format!("ready schema_version={schema}"))
        .map_err(|error| CoreError::Io("log_unavailable", error))?;

    let result = run_requests(&mut pipe, &database, profile, idle_exit).await;
    drop(pipe);
    database.close().await;
    result?;
    log_event(log, "stopped").map_err(|error| CoreError::Io("log_unavailable", error))
}

async fn run_requests(
    pipe: &mut NamedPipeServer,
    database: &SqlitePool,
    profile: &Profile,
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
            () = idle, if idle_exit => return Ok(()),
            client = pipe.connect() => client.map_err(|error| CoreError::Io("pipe_unavailable", error))?,
        }

        tokio::select! {
            signal = &mut shutdown => {
                signal.map_err(|error| CoreError::Io("shutdown_unavailable", error))?;
                return Ok(());
            },
            _ = tokio::time::timeout(REQUEST_LIMIT, respond(pipe, database, profile)) => {}
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
    profile: &Profile,
) -> io::Result<()> {
    let response = match read_frame::<Request>(pipe).await {
        Ok(request) if request.version == PROTOCOL_VERSION => match request.operation {
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
        },
        Ok(_) => error_response("unsupported_protocol_version"),
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            error_response("invalid_request")
        }
        Err(error) => return Err(error),
    };

    write_frame(pipe, &response).await?;
    // DisconnectNamedPipe can discard unread bytes. Wait until the client has the response.
    let _ = pipe.read_u8().await;
    Ok(())
}

fn error_response(code: &str) -> Response {
    Response::Error { code: code.into() }
}

fn log_event(file: &mut File, event: &str) -> io::Result<()> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let event = event.replace(['\r', '\n'], " ");
    writeln!(file, "unix_ms={timestamp} pid={} {event}", process::id())
}
