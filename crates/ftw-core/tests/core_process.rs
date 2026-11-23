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
    assert_eq!(health.schema_version, 7);
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
    assert_eq!(restarted.ready(root).await.schema_version, 7);
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

use ftw_ipc::{crm, Operation};

async fn crm_call(pipe: &PipeEndpoint, command: crm::Command) -> Response {
    timeout(
        Duration::from_secs(5),
        pipe.request(&Operation::Crm(Box::new(command))),
    )
    .await
    .expect("Core response deadline")
    .expect("CRM pipe exchange")
}

async fn crm_ok(pipe: &PipeEndpoint, command: crm::Command) -> crm::Result {
    match crm_call(pipe, command).await {
        Response::Crm { data } => *data,
        response => panic!("expected CRM result, received {response:?}"),
    }
}

fn saved(result: crm::Result) -> String {
    match result {
        crm::Result::Saved { id } => id,
        other => panic!("expected saved record, received {other:?}"),
    }
}

fn contact(display_name: &str, email: &str) -> Box<crm::ContactInput> {
    Box::new(crm::ContactInput {
        display_name: display_name.into(),
        company_name: "FTW Test Company".into(),
        country_region: "中国".into(),
        customer_level: "A".into(),
        customer_type: "经销商".into(),
        customer_source: "手动录入".into(),
        stage: "requirements".into(),
        methods: vec![crm::Method {
            id: None,
            kind: "email".into(),
            value: email.into(),
            label: "工作".into(),
            is_primary: true,
            is_verified: false,
        }],
        interests: vec![crm::Interest {
            id: None,
            product_text: "BK05".into(),
            intent_quantity: Some(500),
            quantity_is_estimate: true,
            requirement: "样品".into(),
        }],
        ..Default::default()
    })
}

#[tokio::test]
async fn crm_contacts_timeline_search_and_archive_survive_restart() {
    let directory = tempdir().unwrap();
    let root = directory.path();
    let mut first = CoreProcess::start(root);
    first.ready(root).await;
    let pipe = PipeEndpoint::for_data_directory(&root.join("data")).unwrap();
    let id = saved(
        crm_ok(
            &pipe,
            crm::Command::SaveContact {
                id: None,
                input: contact("深圳采购联系人", "Used@Example.com"),
                allow_duplicate: false,
            },
        )
        .await,
    );

    let result = crm_ok(
        &pipe,
        crm::Command::SaveContact {
            id: None,
            input: contact("重复客户", "used@example.COM"),
            allow_duplicate: false,
        },
    )
    .await;
    assert!(
        matches!(result, crm::Result::Duplicates { ref matches } if matches[0].id == id && matches[0].reason == "email")
    );

    let second_id = saved(
        crm_ok(
            &pipe,
            crm::Command::SaveContact {
                id: None,
                input: contact("同公司第二联系人", "second@example.com"),
                allow_duplicate: true,
            },
        )
        .await,
    );
    assert_ne!(id, second_id);

    for keyword in ["深圳", "深圳采", "BK05", "used@example.com"] {
        let result = crm_ok(
            &pipe,
            crm::Command::ListContacts {
                query: crm::ContactQuery {
                    keyword: keyword.into(),
                    country: "中国".into(),
                    stage: "requirements".into(),
                    level: "A".into(),
                    customer_type: "经销商".into(),
                    source: "手动录入".into(),
                    ..Default::default()
                },
            },
        )
        .await;
        let crm::Result::Contacts { rows, .. } = result else {
            panic!("expected customer list")
        };
        assert!(!rows.is_empty());
        assert!(rows.iter().any(|row| row.id == id));
    }

    let result = crm_ok(&pipe, crm::Command::GetContact { id: id.clone() }).await;
    let crm::Result::Contact { mut record } = result else {
        panic!("expected customer")
    };
    assert_eq!(record.input.methods[0].value, "Used@Example.com");
    let channel_id = record.input.methods[0].id.clone();
    record.input.customer_level = "B".into();
    record.input.methods.push(crm::Method {
        id: None,
        kind: "whatsapp".into(),
        value: "+0012345".into(),
        label: String::new(),
        is_primary: true,
        is_verified: false,
    });
    saved(
        crm_ok(
            &pipe,
            crm::Command::SaveContact {
                id: Some(id.clone()),
                input: Box::new(record.input),
                allow_duplicate: true,
            },
        )
        .await,
    );

    let activity_id = saved(
        crm_ok(
            &pipe,
            crm::Command::SaveActivity {
                id: None,
                input: crm::ActivityInput {
                    counts_as_contact: None,
                    contact_id: id.clone(),
                    event_type: "call".into(),
                    occurred_at: 1_760_000_000,
                    summary: "讨论样品".into(),
                    details: "确认采购意向".into(),
                    ..Default::default()
                },
            },
        )
        .await,
    );
    saved(
        crm_ok(
            &pipe,
            crm::Command::SaveActivity {
                id: Some(activity_id.clone()),
                input: crm::ActivityInput {
                    counts_as_contact: None,
                    contact_id: id.clone(),
                    event_type: "call".into(),
                    occurred_at: 1_760_000_100,
                    summary: "更新样品跟进".into(),
                    details: "保留历史记录".into(),
                    ..Default::default()
                },
            },
        )
        .await,
    );
    saved(
        crm_ok(
            &pipe,
            crm::Command::SetArchived {
                id: id.clone(),
                archived: true,
            },
        )
        .await,
    );

    drop(first);
    let mut restarted = CoreProcess::start(root);
    restarted.ready(root).await;

    let result = crm_ok(
        &pipe,
        crm::Command::ListContacts {
            query: crm::ContactQuery::default(),
        },
    )
    .await;
    assert!(
        matches!(result, crm::Result::Contacts { rows, .. } if rows.len() == 1 && rows[0].id == second_id)
    );
    let result = crm_ok(&pipe, crm::Command::GetContact { id: id.clone() }).await;
    let crm::Result::Contact { record } = result else {
        panic!("expected archived customer")
    };
    assert!(record.archived_at.is_some());
    assert_eq!(record.input.customer_level, "B");
    assert_eq!(record.input.methods[0].id, channel_id);
    assert_eq!(record.input.methods[1].value, "+0012345");
    assert_eq!(record.observed_last_contact_at, Some(1_760_000_100));
    let result = crm_ok(
        &pipe,
        crm::Command::ListActivities {
            contact_id: Some(id.clone()),
            event_type: "call".into(),
            offset: 0,
        },
    )
    .await;
    assert!(matches!(result, crm::Result::Activities { rows, .. }
        if rows.len() == 1 && rows[0].id == activity_id && rows[0].source == "manual"));

    saved(
        crm_ok(
            &pipe,
            crm::Command::SetArchived {
                id: id.clone(),
                archived: false,
            },
        )
        .await,
    );
    drop(restarted);
    let mut database = open_database(&root.join("data")).await;
    let audit_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE entity_id=?")
        .bind(&id)
        .fetch_one(&mut database)
        .await
        .unwrap();
    assert_eq!(audit_count, 4);
    database.close().await.unwrap();
}

#[tokio::test]
async fn candidate_review_deduplication_and_promotion_use_one_core_transaction() {
    let directory = tempdir().unwrap();
    let mut core = CoreProcess::start(directory.path());
    core.ready(directory.path()).await;
    let pipe = PipeEndpoint::for_data_directory(&directory.path().join("data")).unwrap();
    let lead = saved(
        crm_ok(
            &pipe,
            crm::Command::SaveLead {
                id: None,
                allow_duplicate: false,
                input: Box::new(crm::LeadInput {
                    display_name: "Potential buyer".into(),
                    email: "Buyer@Example.com".into(),
                    website_url: "https://www.example.com/profile".into(),
                    source_urls: vec!["https://example.com/about".into()],
                    business_summary: "unverified statement".into(),
                    research_notes: "unverified notes".into(),
                    confidence: Some(66),
                    ..Default::default()
                }),
            },
        )
        .await,
    );
    let result = crm_ok(
        &pipe,
        crm::Command::SaveLead {
            id: None,
            allow_duplicate: false,
            input: Box::new(crm::LeadInput {
                display_name: "Same website".into(),
                website_url: "https://example.com/sales".into(),
                ..Default::default()
            }),
        },
    )
    .await;
    assert!(matches!(result, crm::Result::Duplicates { ref matches }
        if matches[0].id == lead && matches[0].reason == "domain"));

    saved(
        crm_ok(
            &pipe,
            crm::Command::SetLeadReview {
                id: lead.clone(),
                rejected: true,
            },
        )
        .await,
    );
    let rejected = crm_call(
        &pipe,
        crm::Command::PromoteLead {
            id: lead.clone(),
            allow_duplicate: false,
        },
    )
    .await;
    assert!(matches!(rejected, Response::Error { code, .. } if code == "crm_read_only"));
    saved(
        crm_ok(
            &pipe,
            crm::Command::SetLeadReview {
                id: lead.clone(),
                rejected: false,
            },
        )
        .await,
    );

    let id = saved(
        crm_ok(
            &pipe,
            crm::Command::PromoteLead {
                id: lead.clone(),
                allow_duplicate: false,
            },
        )
        .await,
    );
    let duplicate_call = saved(
        crm_ok(
            &pipe,
            crm::Command::PromoteLead {
                id: lead.clone(),
                allow_duplicate: false,
            },
        )
        .await,
    );
    assert_eq!(id, duplicate_call);
    let result = crm_ok(&pipe, crm::Command::GetContact { id: id.clone() }).await;
    let crm::Result::Contact { record } = result else {
        panic!("expected promoted contact")
    };
    assert!(record.input.notes.is_empty() && record.input.main_need.is_empty());
    assert!(!record.input.methods[0].is_verified);
    let result = crm_ok(&pipe, crm::Command::GetLead { id: lead }).await;
    let crm::Result::Lead { record, .. } = result else {
        panic!("expected candidate")
    };
    assert_eq!(record.promoted_contact_id, Some(id));
    assert_eq!(record.input.source_urls, ["https://example.com/about"]);
}

#[tokio::test]
async fn upgrading_m0_takes_a_readable_snapshot_before_changing_schema() {
    use sqlx::migrate::Migrate;

    let directory = tempdir().unwrap();
    let root = directory.path();
    let data = root.join("data");
    fs::create_dir(&data).unwrap();
    let migrations = sqlx::migrate!("./migrations");
    let mut original = open_database(&data).await;
    original.ensure_migrations_table().await.unwrap();
    original
        .apply(migrations.iter().next().unwrap())
        .await
        .unwrap();
    sqlx::query("INSERT INTO app_settings(key,value) VALUES ('existing','keep this')")
        .execute(&mut original)
        .await
        .unwrap();
    original.close().await.unwrap();

    let mut core = CoreProcess::start(root);
    assert_eq!(core.ready(root).await.schema_version, 7);
    let snapshot = fs::read_dir(root.join("backups"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|extension| extension == "db"))
        .unwrap();
    let mut backup = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(snapshot)
            .read_only(true),
    )
    .await
    .unwrap();
    let retained: String =
        sqlx::query_scalar("SELECT value FROM app_settings WHERE key='existing'")
            .fetch_one(&mut backup)
            .await
            .unwrap();
    let version: i64 = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(&mut backup)
        .await
        .unwrap();
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut backup)
        .await
        .unwrap();
    assert_eq!(
        (retained.as_str(), version, integrity.as_str()),
        ("keep this", 1, "ok")
    );
    backup.close().await.unwrap();
}

async fn revisions(pipe: &PipeEndpoint) -> ftw_ipc::ChangeVersions {
    let Response::Changes { versions } = pipe.request(&Operation::Changes).await.unwrap() else {
        panic!("expected Core change versions")
    };
    versions
}

#[tokio::test]
async fn agent_write_refreshes_customer_data_and_rejects_stale_desktop_save() {
    let dir = tempdir().unwrap();
    let mut core = CoreProcess::start(dir.path());
    core.ready(dir.path()).await;
    let pipe = PipeEndpoint::for_data_directory(&dir.path().join("data")).unwrap();
    let initial = revisions(&pipe).await;
    let mut input = contact("Shared CRM fixture", "shared@example.test");
    input.first_contact_at = Some(1_700_000_000);
    input.last_contact_at = Some(1_720_000_000);
    input.customer_level = "A | 展会".into();
    input.sample_status = "未寄样".into();
    let Response::Crm { data } = pipe
        .request(&Operation::AgentCrm(Box::new(
            ftw_ipc::agent::Command::CreateCustomer {
                request_id: "r1-create".into(),
                input,
            },
        )))
        .await
        .unwrap()
    else {
        panic!("Agent create");
    };
    let id = saved(*data);
    let version = revisions(&pipe).await;
    assert_eq!(version.epoch, initial.epoch);
    assert!(version.domains["contacts"] > initial.domains["contacts"]);
    assert_eq!(version.domains["mail"], initial.domains["mail"]);
    let crm::Result::Contact { record: before } =
        crm_ok(&pipe, crm::Command::GetContact { id: id.clone() }).await
    else {
        panic!("customer snapshot");
    };
    assert_eq!(before.input.sample_status, "未寄样");

    let query = crm::ContactQuery {
        quick_view: "sample".into(),
        ..Default::default()
    };
    assert!(
        matches!(crm_ok(&pipe, crm::Command::ListContacts { query }).await,
        crm::Result::Contacts { rows, .. } if rows.is_empty())
    );
    let response = pipe
        .request(&Operation::AgentCrm(Box::new(
            ftw_ipc::agent::Command::UpdateCustomer {
                request_id: "r1-update".into(),
                id: id.clone(),
                expected_revision: Some(before.record_revision),
                expected_epoch: Some(before.data_epoch.clone()),
                patch: Box::new(ftw_ipc::agent::ContactPatch {
                    sample_status: Some("样品运输中".into()),
                    ..Default::default()
                }),
            },
        )))
        .await
        .unwrap();
    assert!(
        matches!(response, Response::Crm { data } if matches!(*data, crm::Result::Saved { .. }))
    );
    let response = crm_call(
        &pipe,
        crm::Command::SaveContactChecked {
            id: id.clone(),
            input: Box::new(before.input),
            allow_duplicate: false,
            expected_revision: before.record_revision,
            expected_epoch: Some(before.data_epoch),
        },
    )
    .await;
    assert!(matches!(response, Response::Error { code, .. } if code == "crm_revision_conflict"));
    assert!(revisions(&pipe).await.domains["contacts"] > version.domains["contacts"]);

    for (event_type, occurred_at, counts_as_contact) in [
        ("call", 1_730_000_000, None),
        ("note", 1_750_000_000, None),
        ("note", 1_740_000_000, Some(true)),
    ] {
        saved(
            crm_ok(
                &pipe,
                crm::Command::SaveActivity {
                    id: None,
                    input: crm::ActivityInput {
                        contact_id: id.clone(),
                        event_type: event_type.into(),
                        occurred_at,
                        counts_as_contact,
                        summary: "Fixture".into(),
                        ..Default::default()
                    },
                },
            )
            .await,
        );
    }
    let after_activity = revisions(&pipe).await;
    assert!(after_activity.domains["activity"] > version.domains["activity"]);
    let crm::Result::Contact { record } =
        crm_ok(&pipe, crm::Command::GetContact { id: id.clone() }).await
    else {
        panic!("customer snapshot");
    };
    assert_eq!(record.effective_first_contact_at, Some(1_700_000_000));
    assert_eq!(record.effective_last_contact_at, Some(1_740_000_000));
    assert_eq!(record.input.last_contact_at, Some(1_720_000_000));
    assert_eq!(record.input.customer_level, "A | 展会");

    for quick_view in ["priority", "sample"] {
        let query = crm::ContactQuery {
            quick_view: quick_view.into(),
            level: "A".into(),
            sample_status: "in_transit".into(),
            ..Default::default()
        };
        assert!(
            matches!(crm_ok(&pipe, crm::Command::ListContacts { query }).await,
            crm::Result::Contacts { rows, .. } if rows.len() == 1 && rows[0].last_contact_at == record.effective_last_contact_at)
        );
    }
}
