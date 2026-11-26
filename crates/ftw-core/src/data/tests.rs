use super::*;
use ftw_ipc::data::{SearchCategory, SearchTarget};

#[tokio::test]
async fn analytics_needs_an_observed_correspondence_or_completed_sync() {
    let dir = tempfile::tempdir().unwrap();
    let pool = crate::storage::open(dir.path(), dir.path()).await.unwrap();
    transfer::import(
        &pool,
        "客户名称,邮箱\nSample,one@example.test\n",
        true,
        "prospect",
    )
    .await
    .unwrap();
    sqlx::raw_sql("INSERT INTO local_secrets(id,protected) VALUES('fixture',X''),('imap-fixture',X'');
        INSERT INTO email_accounts(id,display_name,email_address,smtp_host,smtp_port,smtp_security,smtp_username,secret_ref)
            VALUES('fixture','Sample','sample@example.test','smtp.example.test',465,'tls','sample','fixture');
        INSERT INTO imap_accounts(account_id,host,port,username,secret_ref) VALUES('fixture','imap.example.test',993,'sample','imap-fixture');
        INSERT INTO imap_sync_state(account_id,folder,uid_validity,last_uid,last_sync,outcome)
            VALUES('fixture','INBOX',1,1,unixepoch()-10,'ok');
        WITH ids(id) AS (VALUES('reply'),('auto'))
        INSERT INTO email_drafts(id,contact_id,account_id,recipient,subject,body_text,attachments_json,state,source)
            SELECT ids.id,c.id,'fixture','one@example.test',ids.id,'sample','[]','smtp_accepted','manual' FROM ids CROSS JOIN contacts c;
        INSERT INTO email_approvals(id,draft_id,revision,content_sha256,approved_by_user,expires_at)
            SELECT id,id,1,'fixture','manual',unixepoch() FROM email_drafts;
        INSERT INTO email_outbox_jobs(id,draft_id,approval_id,revision,message_id_header,state,purpose)
            SELECT id,id,id,1,id,'smtp_accepted','outreach_followup' FROM email_drafts;
        INSERT INTO email_messages(id,contact_id,account_id,outbox_id,message_id_header,from_address,to_address,subject,body_text,submitted_at)
            SELECT id,contact_id,'fixture',id,id,'sample@example.test','one@example.test',id,'sample',unixepoch()-300 FROM email_drafts;
        WITH responses(id,thread,role) AS (VALUES('one','sent:reply','correspondence'),('two','sent:reply','correspondence'),('three','sent:auto','automatic'))
        INSERT INTO inbox_messages(id,account_id,contact_id,thread_id,message_id_header,raw_sha256,sender,subject,body_text,attachment_names,received_at,raw,kind,message_role,match_state)
            SELECT ids.id,'fixture',c.id,ids.thread,ids.id,ids.id,'one@example.test','sample','sample','[]',unixepoch()-30,X'','message',ids.role,'thread'
            FROM responses ids CROSS JOIN contacts c;")
        .execute(&pool).await.unwrap();
    let report = analytics::report(&pool, 7, false).await.unwrap();
    assert_eq!(
        (
            report.totals.accepted,
            report.totals.replied,
            report.totals.waiting,
            report.totals.unsynced
        ),
        (2, 1, 1, 0)
    );
    assert_eq!(report.trend.iter().map(|day| day.replied).sum::<i64>(), 1);
    sqlx::query("UPDATE imap_sync_state SET outcome='more_available'")
        .execute(&pool)
        .await
        .unwrap();
    let report = analytics::report(&pool, 7, false).await.unwrap();
    assert_eq!(
        (
            report.totals.replied,
            report.totals.waiting,
            report.totals.unsynced
        ),
        (1, 0, 1)
    );
}

#[tokio::test]
async fn global_search_finds_cross_record_terms_and_counts_past_the_page() {
    let dir = tempfile::tempdir().unwrap();
    let pool = crate::storage::open(dir.path(), dir.path()).await.unwrap();
    let csv = "客户名称,邮箱\nOrchid Optics,optic@example.test\n";
    transfer::import(&pool, csv, true, "prospect")
        .await
        .unwrap();
    let contact: String = sqlx::query_scalar("SELECT id FROM contacts")
        .fetch_one(&pool)
        .await
        .unwrap();
    for index in 0..21 {
        sqlx::query("INSERT INTO activities(id,contact_id,event_type,occurred_at,summary,details,source,actor_id)
             VALUES(?,?,'note',?,'Orchid evaluation','Optics report','manual','test')")
            .bind(format!("note-{index}")).bind(&contact).bind(index).execute(&pool).await.unwrap();
    }
    sqlx::query(
        "INSERT INTO customer_documents(id,contact_id,filename,content_type,sha256,size)
         VALUES('report',?,'Orchid Optics.pdf','application/pdf',?,99)",
    )
    .bind(&contact)
    .bind("0".repeat(64))
    .execute(&pool)
    .await
    .unwrap();
    let Reply::Search { counts, rows } =
        search::search(&pool, "orchid Optics", SearchCategory::All, 0)
            .await
            .unwrap()
    else {
        panic!("search")
    };
    assert_eq!(
        (
            counts.all,
            counts.customer,
            counts.activity,
            counts.file,
            counts.mail
        ),
        (23, 1, 21, 1, 0)
    );
    assert_eq!(rows.len(), 20);
    assert!(matches!(&rows[0].target, SearchTarget::Customer { id } if id == &contact));
    assert!(
        matches!(&rows[1].target, SearchTarget::Document { id, contact_id } if id == "report" && contact_id == &contact)
    );
    let Reply::Search { counts, rows } =
        search::search(&pool, "orchid Optics", SearchCategory::Activity, 20)
            .await
            .unwrap()
    else {
        panic!("activity search")
    };
    assert_eq!((counts.all, rows.len()), (23, 1));
    let Reply::Search { counts, rows } = search::search(&pool, "%'", SearchCategory::All, 0)
        .await
        .unwrap()
    else {
        panic!("literal search")
    };
    assert_eq!(counts.all, 0);
    assert!(rows.is_empty());
}

#[tokio::test]
async fn preview_import_duplicate_filter_export_and_restore() {
    let dir = tempfile::tempdir().unwrap();
    let pool = crate::storage::open(dir.path(), dir.path()).await.unwrap();
    let sample = "客户名称,邮箱,国家/地区,样品进度\n示例客户,one@example.test,法国,样品运输中\n重复客户,one@example.test,法国,\n";
    let Reply::ImportReport {
        accepted,
        rejected,
        applied,
        mappings,
        ..
    } = transfer::import(&pool, sample, false, "won").await.unwrap()
    else {
        panic!("preview")
    };
    assert_eq!((accepted, rejected, applied), (1, 1, false));
    assert!(mappings
        .iter()
        .any(|mapping| mapping.source == "国家/地区" && mapping.field == "国家或地区"));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contacts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let Reply::ImportReport {
        accepted, applied, ..
    } = transfer::import(&pool, sample, true, "won").await.unwrap()
    else {
        panic!("import")
    };
    assert_eq!((accepted, applied), (1, true));
    let Reply::File { path } = transfer::export(&pool, dir.path()).await.unwrap() else {
        panic!("export")
    };
    let exported = std::fs::read_to_string(path).unwrap();
    assert!(exported.contains("one@example.test") && !exported.contains("重复客户"));

    sqlx::query("UPDATE contacts SET archived_at=unixepoch()")
        .execute(&pool)
        .await
        .unwrap();
    let Reply::File { path } = transfer::export_selected(
        &pool,
        dir.path(),
        ftw_ipc::crm::ArchiveFilter::Archived,
        &["display_name".into()],
    )
    .await
    .unwrap() else {
        panic!("selected export")
    };
    assert_eq!(
        std::fs::read_to_string(path)
            .unwrap()
            .trim_start_matches('\u{feff}'),
        "客户名称\n示例客户\n"
    );
    sqlx::query("UPDATE contacts SET archived_at=NULL")
        .execute(&pool)
        .await
        .unwrap();

    let Reply::AuditWorkspace { rows, .. } =
        audit_workspace::list(&pool, 0, "manual", "contact", "示例客户", 7)
            .await
            .unwrap()
    else {
        panic!("audit workspace")
    };
    assert_eq!(rows.len(), 1);
    assert!(matches!(
        rows[0].target,
        Some(ftw_ipc::data::AuditTarget::Customer { .. })
    ));

    let epoch = crate::storage::changes(&pool).await.unwrap().epoch;
    let copy = backup::create_manual(&pool, dir.path()).await.unwrap();
    sqlx::query("UPDATE contacts SET display_name='changed'")
        .execute(&pool)
        .await
        .unwrap();
    let filename = copy.file_name().unwrap().to_str().unwrap();
    assert!(matches!(
        backup::inspect(dir.path(), filename).await.unwrap(),
        Reply::BackupInspection {
            customers: 1,
            mail_records: 0,
            ..
        }
    ));
    assert!(backup::inspect(dir.path(), "../workbench.db")
        .await
        .is_err());
    std::fs::write(
        dir.path().join("backups/workbench-invalid.db"),
        b"not SQLite",
    )
    .unwrap();
    assert!(backup::inspect(dir.path(), "workbench-invalid.db")
        .await
        .is_err());
    let safety = backup::restore(&pool, dir.path(), filename).await.unwrap();
    assert!(safety.contains("before-restore"));
    assert_ne!(crate::storage::changes(&pool).await.unwrap().epoch, epoch);
    let restored: (String, String) = sqlx::query_as("SELECT display_name,stage FROM contacts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(restored, ("示例客户".into(), "won".into()));
    assert!(backup::restore(&pool, dir.path(), "../workbench.db")
        .await
        .is_err());
    pool.close().await;
}
