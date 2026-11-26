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
async fn upgrading_preserves_agent_identity_and_allows_pause_and_resume() {
    use ftw_ipc::{agent::Collaboration, mail};
    use sha2::{Digest, Sha256};

    let directory = tempdir().unwrap();
    let root = directory.path();
    let data = root.join("data");
    fs::create_dir(&data).unwrap();
    let mut database = open_database(&data).await;
    let mut previous = sqlx::migrate!("./migrations");
    previous
        .migrations
        .to_mut()
        .retain(|migration| migration.version <= 26);
    previous.run(&mut database).await.unwrap();
    let token = format!("{:064x}", uuid::Uuid::new_v4().as_u128());
    sqlx::query("INSERT INTO mail_agent_clients(id,name,token_digest,automatic) VALUES ('fixture','Existing Agent',?,1)")
        .bind(Sha256::digest(token.as_bytes()).to_vec()).execute(&mut database).await.unwrap();
    database.close().await.unwrap();

    let mut core = CoreProcess::start(root);
    assert_eq!(core.ready(root).await.schema_version, 27);
    assert!(fs::read_dir(root.join("backups")).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".db")
    }));
    let pipe = PipeEndpoint::for_data_directory(&data).unwrap();
    let context = Operation::AgentCollaboration {
        token: mail::Secret(token),
        command: Box::new(Collaboration::Context),
    };
    assert!(
        matches!(pipe.request(&context).await.unwrap(), Response::AgentData { data }
        if data["agent"]["id"] == "fixture")
    );
    let settings = pipe
        .request(&Operation::Mail(Box::new(mail::Command::AgentMailSettings)))
        .await
        .unwrap();
    assert!(
        matches!(settings, Response::Mail {data} if matches!(data.as_ref(),
        mail::Result::AgentMailSettings { agents, .. } if agents[0].automatic && agents[0].last_seen_at.is_some()))
    );
    for paused in [true, false] {
        let command = mail::Command::SetAgentPaused {
            id: "fixture".into(),
            paused,
        };
        assert!(matches!(
            pipe.request(&Operation::Mail(Box::new(command)))
                .await
                .unwrap(),
            Response::Mail { .. }
        ));
        let response = pipe.request(&context).await.unwrap();
        if paused {
            assert!(matches!(response, Response::Error {code,..} if code == "mail_agent_paused"));
            assert!(
                matches!(pipe.request(&Operation::AgentCrm(Box::new(ftw_ipc::agent::Command::Fields))).await.unwrap(),
                Response::Error {code,..} if code == "mail_agent_unregistered")
            );
        } else {
            assert!(matches!(response, Response::AgentData { .. }));
        }
    }
}

#[tokio::test]
async fn only_one_core_opens_a_profile_and_settings_survive_restart() {
    let directory = Builder::new().prefix("FTW 验收 ").tempdir().unwrap();
    let root = directory.path();
    let mut first = CoreProcess::start(root);
    let health = first.ready(root).await;
    assert_eq!(health.process_id, first.0.id());
    assert_eq!(health.schema_version, 27);
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
    assert_eq!(restarted.ready(root).await.schema_version, 27);
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

#[tokio::test]
async fn independent_customer_documents_remain_downloadable_after_backup_restore() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use ftw_ipc::{agent, data, mail};
    let profile = tempdir().unwrap();
    let root = profile.path();
    let mut core = CoreProcess::start(root);
    core.ready(root).await;
    let pipe = PipeEndpoint::for_data_directory(&root.join("data")).unwrap();
    let contact = saved(
        crm_ok(
            &pipe,
            crm::Command::SaveContact {
                id: None,
                input: contact("Document fixture", "documents@example.test"),
                allow_duplicate: false,
            },
        )
        .await,
    );
    let content = b"%PDF-1.4\n% isolated test document\n";
    let source = root.join("test catalogue.pdf");
    std::fs::write(&source, content).unwrap();
    for count in [1, 0] {
        let response = pipe
            .request(&Operation::Mail(Box::new(
                mail::Command::ImportCustomerDocuments {
                    contact_id: contact.clone(),
                    paths: vec![source.to_string_lossy().into_owned()],
                },
            )))
            .await
            .unwrap();
        assert!(
            matches!(response, Response::Mail { data } if matches!(*data, mail::Result::ImportedDocuments { count: n } if n == count))
        );
    }
    let response = pipe
        .request(&Operation::Mail(Box::new(mail::Command::FileWorkspace {
            document_id: None,
            contact_id: Some(contact.clone()),
            keyword: "catalogue".into(),
            direction: "document".into(),
            offset: 0,
        })))
        .await
        .unwrap();
    let Response::Mail { data } = response else {
        panic!("file workspace")
    };
    let mail::Result::FileWorkspace { rows, .. } = *data else {
        panic!("file workspace rows")
    };
    assert_eq!(rows.len(), 1);
    let file = &rows[0];
    assert_eq!(file.size, Some(content.len() as u32));
    assert_eq!(file.source.as_deref(), Some("manual"));

    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let sender = pipe
        .request(&Operation::Mail(Box::new(
            mail::Command::RegisterMailAgent {
                name: "File collaboration fixture".into(),
                token: mail::Secret(token.clone()),
            },
        )))
        .await
        .unwrap();
    assert!(
        matches!(sender, Response::Mail { data } if matches!(*data, mail::Result::Saved { .. }))
    );
    let collaborate = |command| Operation::AgentCollaboration {
        token: mail::Secret(token.clone()),
        command: Box::new(command),
    };
    let read = pipe
        .request(&collaborate(agent::Collaboration::ReadDocument {
            id: file.document_id.clone().unwrap(),
            offset: 0,
        }))
        .await
        .unwrap();
    assert!(
        matches!(read, Response::Mail { data } if matches!(data.as_ref(), mail::Result::DocumentChunk {
        content_base64, next_offset: None, .. } if STANDARD.decode(content_base64).unwrap() == content))
    );

    let agent_content = b"%PDF-1.4\n% Agent catalogue\n";
    let upload = pipe
        .request(&collaborate(agent::Collaboration::UploadDocument {
            contact_id: contact,
            filename: "agent catalogue.pdf".into(),
            content_base64: STANDARD.encode(agent_content),
        }))
        .await
        .unwrap();
    let Response::Mail { data } = upload else {
        panic!("agent document upload")
    };
    let mail::Result::Saved {
        id: customer_document_id,
    } = *data
    else {
        panic!("upload result")
    };
    let agent_destination = root.join("exported agent catalogue.pdf");
    let exported = pipe
        .request(&Operation::Mail(Box::new(
            mail::Command::ExportCustomerDocument {
                id: customer_document_id,
                destination: agent_destination.to_string_lossy().into_owned(),
            },
        )))
        .await
        .unwrap();
    assert!(
        matches!(exported, Response::Mail { data } if matches!(*data, mail::Result::Saved { .. }))
    );
    assert_eq!(std::fs::read(agent_destination).unwrap(), agent_content);

    let library = pipe
        .request(&Operation::Mail(Box::new(mail::Command::ListAssets)))
        .await
        .unwrap();
    assert!(
        matches!(library, Response::Mail { data } if matches!(data.as_ref(), mail::Result::Assets { rows } if rows.is_empty()))
    );
    let response = pipe
        .request(&Operation::Data(Box::new(data::Command::CreateBackup)))
        .await
        .unwrap();
    let Response::Data { data } = response else {
        panic!("backup")
    };
    let data::Result::File { path } = *data else {
        panic!("backup file")
    };
    let filename = Path::new(&path)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    std::fs::remove_file(
        root.join("data/mail-objects")
            .join(file.sha256.as_ref().unwrap()),
    )
    .unwrap();
    let restore = pipe
        .request(&Operation::Data(Box::new(data::Command::Restore {
            filename,
        })))
        .await
        .unwrap();
    assert!(
        matches!(restore, Response::Data { data } if matches!(*data, data::Result::Restored { .. }))
    );
    let destination = root.join("exported document.pdf");
    let export = || {
        Operation::Mail(Box::new(mail::Command::ExportCustomerDocument {
            id: file.document_id.clone().unwrap(),
            destination: destination.to_string_lossy().into_owned(),
        }))
    };
    let exported = pipe.request(&export()).await.unwrap();
    assert!(
        matches!(exported, Response::Mail { data } if matches!(*data, mail::Result::Saved { .. }))
    );
    assert_eq!(std::fs::read(&destination).unwrap(), content);
    let duplicate = pipe.request(&export()).await.unwrap();
    assert!(
        matches!(duplicate, Response::Error { code, .. } if code == "attachment_destination_unavailable")
    );
    assert_eq!(std::fs::read(&destination).unwrap(), content);
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
            keyword: None,
            source: None,
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
                expected: None,
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
            expected: None,
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
                expected: None,
            },
        )
        .await,
    );
    let rejected = crm_call(
        &pipe,
        crm::Command::PromoteLead {
            id: lead.clone(),
            allow_duplicate: false,
            expected: None,
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
                expected: None,
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
                expected: None,
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
                expected: None,
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
    assert_eq!(core.ready(root).await.schema_version, 27);
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

async fn register_fixture_agent(pipe: &PipeEndpoint, name: &str) -> String {
    let token =
        uuid::Uuid::new_v4().simple().to_string() + &uuid::Uuid::new_v4().simple().to_string();
    let response = pipe
        .request(&Operation::Mail(Box::new(
            ftw_ipc::mail::Command::RegisterMailAgent {
                name: name.into(),
                token: ftw_ipc::mail::Secret(token.clone()),
            },
        )))
        .await
        .unwrap();
    assert!(
        matches!(response, Response::Mail { data } if matches!(*data, ftw_ipc::mail::Result::Saved { .. }))
    );
    token
}

async fn revisions(pipe: &PipeEndpoint) -> ftw_ipc::ChangeVersions {
    let Response::Changes { versions } = pipe.request(&Operation::Changes).await.unwrap() else {
        panic!("expected Core change versions")
    };
    versions
}

#[tokio::test]
async fn agent_detail_review_preserves_a_quantity_confirmed_after_the_proposal() {
    use ftw_ipc::agent::{Command, ContactDetailSuggestion, Evidence, EvidenceKind};

    let dir = tempdir().unwrap();
    let mut core = CoreProcess::start(dir.path());
    core.ready(dir.path()).await;
    let pipe = PipeEndpoint::for_data_directory(&dir.path().join("data")).unwrap();
    let agent_token = register_fixture_agent(&pipe, "Shared fixture").await;
    let mut input = contact("Confirmed product details", "details@example.test");
    input.interests = vec![crm::Interest {
        id: None,
        product_text: "Pilot camera".into(),
        requirement: "User confirmed test criteria".into(),
        intent_quantity: None,
        quantity_is_estimate: false,
    }];
    let id = saved(
        crm_ok(
            &pipe,
            crm::Command::SaveContact {
                id: None,
                input,
                allow_duplicate: false,
            },
        )
        .await,
    );
    let crm::Result::Contact { mut record } =
        crm_ok(&pipe, crm::Command::GetContact { id: id.clone() }).await
    else {
        panic!("expected customer")
    };
    let result = pipe
        .request(&Operation::RegisteredAgentCrm {
            token: ftw_ipc::mail::Secret(agent_token.clone()),
            command: Box::new(Command::SuggestCustomerDetail {
                request_id: "detail-conflict-test".into(),
                id: id.clone(),
                expected_revision: record.record_revision,
                expected_epoch: record.data_epoch.clone(),
                detail: ContactDetailSuggestion::ProductInterest {
                    product_text: "pilot camera".into(),
                    intent_quantity: Some(20),
                    quantity_is_estimate: true,
                    requirement: String::new(),
                },
                evidence: Evidence {
                    kind: EvidenceKind::Inferred,
                    url: Some("https://example.test/catalog".into()),
                    note: None,
                },
            }),
        })
        .await
        .unwrap();
    assert!(
        matches!(result, Response::Crm { data } if matches!(*data, crm::Result::AgentReviewPending { count: 1, .. }))
    );

    let result = pipe
        .request(&Operation::RegisteredAgentCrm {
            token: ftw_ipc::mail::Secret(agent_token.clone()),
            command: Box::new(Command::SuggestCustomerDetail {
                request_id: "detail-correction-conflict".into(),
                id: id.clone(),
                expected_revision: record.record_revision,
                expected_epoch: record.data_epoch.clone(),
                detail: ContactDetailSuggestion::CorrectProductInterest {
                    interest_id: record.input.interests[0].id.clone().unwrap(),
                    intent_quantity: Some(21),
                    quantity_is_estimate: Some(false),
                    requirement: None,
                },
                evidence: Evidence {
                    kind: EvidenceKind::Inferred,
                    url: None,
                    note: None,
                },
            }),
        })
        .await
        .unwrap();
    assert!(
        matches!(result, Response::Crm { data } if matches!(*data, crm::Result::AgentReviewPending { count: 1, .. }))
    );

    record.input.interests[0].intent_quantity = Some(72);
    saved(
        crm_ok(
            &pipe,
            crm::Command::SaveContactChecked {
                id: id.clone(),
                input: Box::new(record.input),
                allow_duplicate: false,
                expected_revision: record.record_revision,
                expected_epoch: Some(record.data_epoch),
            },
        )
        .await,
    );
    let crm::Result::AgentChanges {
        rows, conflicts, ..
    } = crm_ok(
        &pipe,
        crm::Command::ListAgentChanges {
            offset: 0,
            pending: false,
            scope: Some("conflict".into()),
        },
    )
    .await
    else {
        panic!("expected pending change")
    };
    assert_eq!(conflicts, 2);
    assert_eq!(rows.len(), 2);
    for row in rows {
        assert!(row.current_value.contains("\"intent_quantity\":72"));
        assert_eq!(row.evidence_kind.as_deref(), Some("inferred"));
        let result = crm_call(
            &pipe,
            crm::Command::ResolveAgentChange {
                id: row.id,
                accept: true,
            },
        )
        .await;
        assert!(
            matches!(result, Response::Error { detail, .. } if detail.as_deref() == Some("agent_change_superseded"))
        );
    }
    let crm::Result::Contact { record } = crm_ok(&pipe, crm::Command::GetContact { id }).await
    else {
        panic!("expected customer")
    };
    assert_eq!(record.input.interests[0].intent_quantity, Some(72));
    assert_eq!(
        record.input.interests[0].requirement,
        "User confirmed test criteria"
    );
}

#[tokio::test]
async fn agent_write_refreshes_customer_data_and_rejects_stale_desktop_save() {
    let dir = tempdir().unwrap();
    let mut core = CoreProcess::start(dir.path());
    core.ready(dir.path()).await;
    let pipe = PipeEndpoint::for_data_directory(&dir.path().join("data")).unwrap();
    let agent_token = register_fixture_agent(&pipe, "Shared fixture").await;
    let initial = revisions(&pipe).await;
    let mut input = contact("Shared CRM fixture", "shared@example.test");
    input.first_contact_at = Some(1_700_000_000);
    input.last_contact_at = Some(1_720_000_000);
    input.customer_level = "A | 展会".into();
    input.sample_status = "未寄样".into();
    let Response::Crm { data } = pipe
        .request(&Operation::RegisteredAgentCrm {
            token: ftw_ipc::mail::Secret(agent_token.clone()),
            command: Box::new(ftw_ipc::agent::Command::CreateCustomer {
                request_id: "r1-create".into(),
                input,
            }),
        })
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
        .request(&Operation::RegisteredAgentCrm {
            token: ftw_ipc::mail::Secret(agent_token.clone()),
            command: Box::new(ftw_ipc::agent::Command::UpdateCustomer {
                request_id: "r1-update".into(),
                id: id.clone(),
                expected_revision: Some(before.record_revision),
                expected_epoch: Some(before.data_epoch.clone()),
                patch: Box::new(ftw_ipc::agent::ContactPatch {
                    sample_status: Some("样品运输中".into()),
                    ..Default::default()
                }),
            }),
        })
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
    let result = pipe
        .request(&Operation::RegisteredAgentCrm {
            token: ftw_ipc::mail::Secret(agent_token.clone()),
            command: Box::new(ftw_ipc::agent::Command::AddActivity {
                request_id: "structured-sample-test".into(),
                input: crm::ActivityInput {
                    contact_id: id.clone(),
                    event_type: "sample".into(),
                    occurred_at: 1_760_000_000,
                    summary: "样品寄出".into(),
                    sample: Some(crm::SampleActivity {
                        model: "DC226".into(),
                        quantity: Some(2),
                        batch_ref: Some("CAM-A".into()),
                        progress: Some("in_transit".into()),
                    }),
                    ..Default::default()
                },
            }),
        })
        .await
        .unwrap();
    let Response::Crm { data } = result else {
        panic!("agent sample result")
    };
    let activity = saved(*data);
    saved(
        crm_ok(
            &pipe,
            crm::Command::SaveActivity {
                id: Some(activity),
                input: crm::ActivityInput {
                    contact_id: id.clone(),
                    event_type: "sample".into(),
                    occurred_at: 1_760_000_000,
                    summary: "样品已寄出".into(),
                    ..Default::default()
                },
            },
        )
        .await,
    );
    let result = crm_ok(
        &pipe,
        crm::Command::SampleWorkspace {
            keyword: "DC226".into(),
            status: "in_transit".into(),
            offset: 0,
            by_batch: false,
        },
    )
    .await;
    assert!(
        matches!(result, crm::Result::SampleWorkspace { rows, counts, .. }
        if rows.len() == 1 && rows[0].contact_id == id && rows[0].quantity == Some(2)
           && rows[0].summary == "样品已寄出" && counts == [("in_transit".into(), 1)])
    );
}
