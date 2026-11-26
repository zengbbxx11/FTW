mod agent;
mod crm;
mod data;
mod mail;
mod server;
mod storage;

use ftw_ipc::{
    diagnostics::{self, Component, Log},
    PipeEndpoint, Profile, Response, CORE_ALREADY_RUNNING,
};
use std::{env, io, path::PathBuf, process::ExitCode, time::Duration};

#[derive(Debug, thiserror::Error)]
enum CoreError {
    #[error(
        "usage: ftw-core <serve [--idle-exit] | health | diagnostics> [--dev-root ABSOLUTE_PATH]"
    )]
    Usage,
    #[error("profile_unavailable: {0}")]
    Profile(#[source] io::Error),
    #[error("core_already_running: another Core holds this data directory")]
    AlreadyRunning,
    #[error("{0}: {1}")]
    Io(&'static str, #[source] io::Error),
    #[error("database_unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migration_failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("health_unavailable: {0}")]
    Health(String),
}

impl CoreError {
    fn diagnostic_code(&self) -> &'static str {
        match self {
            Self::Usage => "invalid_arguments",
            Self::Profile(_) => "profile_unavailable",
            Self::AlreadyRunning => "core_already_running",
            Self::Io(code, _) => code,
            Self::Database(_) => "database_unavailable",
            Self::Migration(_) => "migration_failed",
            Self::Health(_) => "health_unavailable",
        }
    }
}

struct Options {
    serve: bool,
    diagnostics: bool,
    idle_exit: bool,
    dev_root: Option<PathBuf>,
}

impl Options {
    fn parse() -> Result<Self, CoreError> {
        let mut args = env::args_os().skip(1);
        let command = args.next().ok_or(CoreError::Usage)?;
        let (serve, diagnostics) = match command.to_str() {
            Some("serve") => (true, false),
            Some("health") => (false, false),
            Some("diagnostics") => (false, true),
            _ => return Err(CoreError::Usage),
        };
        let mut options = Self {
            serve,
            diagnostics,
            idle_exit: false,
            dev_root: None,
        };
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--idle-exit") if serve && !options.idle_exit => options.idle_exit = true,
                Some("--dev-root") if options.dev_root.is_none() => {
                    options.dev_root = Some(args.next().ok_or(CoreError::Usage)?.into());
                }
                _ => return Err(CoreError::Usage),
            }
        }
        Ok(options)
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            match error {
                CoreError::AlreadyRunning => ExitCode::from(CORE_ALREADY_RUNNING as u8),
                CoreError::Usage => ExitCode::from(2),
                _ => ExitCode::FAILURE,
            }
        }
    }
}

async fn run() -> Result<(), CoreError> {
    let options = Options::parse()?;
    let executable = env::current_exe().map_err(CoreError::Profile)?;
    if options.diagnostics {
        let report = diagnostics::report(&executable, options.dev_root)
            .map_err(|error| CoreError::Io("diagnostics_unavailable", error))?;
        println!("{report}");
        return Ok(());
    }
    if options.serve {
        let log = Log::new(&executable, options.dev_root.clone(), Component::Core);
        log.install_panic_hook();
        let result = match Profile::resolve(&executable, options.dev_root) {
            Ok(profile) => server::serve(profile, &log, options.idle_exit).await,
            Err(error) => Err(CoreError::Profile(error)),
        };
        if let Err(error) = &result {
            log.event("process", error.diagnostic_code());
        }
        return result;
    }
    let profile = Profile::resolve(&executable, options.dev_root).map_err(CoreError::Profile)?;

    let endpoint = PipeEndpoint::for_data_directory(&profile.data)
        .map_err(|error| CoreError::Io("pipe_unavailable", error))?;
    let response = tokio::time::timeout(Duration::from_secs(3), endpoint.health())
        .await
        .map_err(|_| CoreError::Health("Core did not respond within three seconds".into()))?
        .map_err(|error| CoreError::Health(error.to_string()))?;
    match response {
        Response::Ready { health } => {
            let json = serde_json::to_string_pretty(&health)
                .map_err(|error| CoreError::Health(error.to_string()))?;
            println!("{json}");
            Ok(())
        }
        Response::Error { code, .. } => Err(CoreError::Health(code)),
        Response::Crm { .. }
        | Response::Mail { .. }
        | Response::Inbox { .. }
        | Response::Changes { .. }
        | Response::Data { .. }
        | Response::AgentData { .. } => Err(CoreError::Health("unexpected response".into())),
    }
}
