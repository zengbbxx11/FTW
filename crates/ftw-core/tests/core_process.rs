use ftw_ipc::{Health, PipeEndpoint, Response, StorageMode, CORE_ALREADY_RUNNING};
use sqlx::{sqlite::SqliteConnectOptions, Connection, SqliteConnection};
use std::{
    fs,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};
use tempfile::{tempdir, Builder};
use tokio::time::{sleep, timeout, Instant};

const CORE: &str = env!("CARGO_BIN_EXE_ftw-core");

struct CoreProcess(Child);

impl CoreProcess {
    fn start(root: &Path) -> Self {
        Self(
            Command::new(CORE)
                .args(["serve", "--dev-root"])
                .arg(root)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .spawn()
                .expect("launch independent Core"),
        )
    }

    async fn ready(&mut self, root: &Path) -> Health {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(
                self.0.try_wait().unwrap().is_none(),
                "Core exited before readiness"
            );
            let data = root.join("data");
            if let Ok(pipe) = PipeEndpoint::for_data_directory(&data) {
                if let Ok(Ok(Response::Ready { health })) =
                    timeout(Duration::from_secs(1), pipe.health()).await
                {
                    return health;
                }
            }
            assert!(Instant::now() < deadline, "Core did not report readiness");
            sleep(Duration::from_millis(50)).await;
        }
    }
}

impl Drop for CoreProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn open_database(data: &Path) -> SqliteConnection {
    let options = SqliteConnectOptions::new()
        .filename(data.join("workbench.db"))
        .create_if_missing(true);
    SqliteConnection::connect_with(&options).await.unwrap()
}

#[tokio::test]
async fn only_one_core_opens_a_profile_and_settings_survive_restart() {
    let directory = Builder::new().prefix("FTW 验收 ").tempdir().unwrap();
    let root = directory.path();
    let mut first = CoreProcess::start(root);
    let health = first.ready(root).await;
    assert_eq!(health.process_id, first.0.id());
    assert_eq!(health.schema_version, 1);
    assert!(matches!(health.storage_mode, StorageMode::Development));
    assert!(health.data_root.contains("FTW 验收 "));

    let duplicate = Command::new(CORE)
        .args(["serve", "--dev-root"])
        .arg(root)
        .output()
        .unwrap();
    assert_eq!(duplicate.status.code(), Some(CORE_ALREADY_RUNNING));
    let cli = Command::new(CORE)
        .args(["health", "--dev-root"])
        .arg(root)
        .output()
        .unwrap();
    assert!(
        cli.status.success(),
        "{}",
        String::from_utf8_lossy(&cli.stderr)
    );
    let health_from_cli: Health = serde_json::from_slice(&cli.stdout).unwrap();
    assert_eq!(health_from_cli.process_id, health.process_id);
    assert!(!fs::read_to_string(root.join("logs/core.log"))
        .unwrap()
        .is_empty());
    drop(first);

    let data = root.join("data");
    let mut database = open_database(&data).await;
    sqlx::query("INSERT INTO app_settings(key, value) VALUES ('test-setting', 'retained')")
        .execute(&mut database)
        .await
        .unwrap();
    database.close().await.unwrap();

    let mut restarted = CoreProcess::start(root);
    assert_eq!(restarted.ready(root).await.schema_version, 1);
    drop(restarted);

    let mut database = open_database(&data).await;
    let value: String =
        sqlx::query_scalar("SELECT value FROM app_settings WHERE key = 'test-setting'")
            .fetch_one(&mut database)
            .await
            .unwrap();
    assert_eq!(value, "retained");
    database.close().await.unwrap();
}

#[tokio::test]
async fn failed_migration_preserves_existing_data_and_does_not_listen() {
    let directory = tempdir().unwrap();
    let root = directory.path();
    let data = root.join("data");
    fs::create_dir(&data).unwrap();
    let mut database = open_database(&data).await;
    sqlx::query("CREATE TABLE app_settings (existing TEXT NOT NULL)")
        .execute(&mut database)
        .await
        .unwrap();
    sqlx::query("INSERT INTO app_settings(existing) VALUES ('keep me')")
        .execute(&mut database)
        .await
        .unwrap();
    database.close().await.unwrap();

    let outcome = Command::new(CORE)
        .args(["serve", "--dev-root"])
        .arg(root)
        .output()
        .unwrap();
    assert!(!outcome.status.success());
    assert!(String::from_utf8_lossy(&outcome.stderr).contains("migration_failed"));
    let pipe = PipeEndpoint::for_data_directory(&data).unwrap();
    assert!(matches!(
        pipe.health().await,
        Err(ftw_ipc::PipeOpenError::Absent)
    ));

    let mut database = open_database(&data).await;
    let row: String = sqlx::query_scalar("SELECT existing FROM app_settings")
        .fetch_one(&mut database)
        .await
        .unwrap();
    assert_eq!(row, "keep me");
    database.close().await.unwrap();
}

#[test]
fn core_rejects_a_read_only_lock_location() {
    let directory = tempdir().unwrap();
    let data = directory.path().join("data");
    fs::create_dir(&data).unwrap();
    let lock = data.join("core.lock");
    fs::write(&lock, "").unwrap();
    let original_permissions = fs::metadata(&lock).unwrap().permissions();
    let mut read_only = original_permissions.clone();
    read_only.set_readonly(true);
    fs::set_permissions(&lock, read_only).unwrap();

    let result = Command::new(CORE)
        .args(["serve", "--dev-root"])
        .arg(directory.path())
        .output();
    fs::set_permissions(&lock, original_permissions).unwrap();
    let outcome = result.unwrap();

    assert!(!outcome.status.success());
    assert!(String::from_utf8_lossy(&outcome.stderr).contains("data_unavailable"));
    assert!(!data.join("workbench.db").exists());
}
