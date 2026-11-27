use super::{store, transport, FOLDER};
use ftw_ipc::{
    crm::{self, ContactInput, Method},
    inbox, Response,
};
use sqlx::SqlitePool;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const MAIL: &[u8] = b"From: Buyer <buyer@example.test>\r\nTo: sender@example.test\r\nSubject: Re: Quote\r\nMessage-ID: <reply@example.test>\r\nIn-Reply-To: <out@ftw.local>\r\n\r\nBuyer reply.\r\n";

#[tokio::test]
async fn non_customer_handling_is_reversible_persistent_and_does_not_block_new_messages() {
    let (dir, pool, _) = fixture().await;
    let known = String::from_utf8_lossy(MAIL).replace("In-Reply-To: <out@ftw.local>\r\n", "");
    let ordinary = known
        .replace("buyer@example.test", "newsletter@example.test")
        .replace("reply@", "ordinary@");
    let automatic = known
        .replace("Subject:", "Auto-Submitted: auto-replied\r\nSubject:")
        .replace("reply@", "automatic@");
    for (uid, raw) in [(1, &known), (2, &ordinary), (3, &automatic)] {
        store::save(&pool, "acct", 12, uid, raw.as_bytes(), None, false)
            .await
            .unwrap();
    }
    let list = |filter| store::list(&pool, None, None, false, 0, None, filter);
    let inbox::Result::Rows { rows, .. } = list(inbox::InboxFilter::Review).await.unwrap() else {
        panic!("review")
    };
    assert_eq!(rows.len(), 1);
    let target = inbox::HandlingTarget {
        id: rows[0].id.clone(),
        expected_revision: rows[0].handling_revision,
    };
    store::classify(
        &pool,
        crate::crm::Actor::DESKTOP,
        vec![target.clone()],
        true,
    )
    .await
    .unwrap();
    assert!(
        store::classify(&pool, crate::crm::Actor::DESKTOP, vec![target], false)
            .await
            .is_err()
    );
    store::save(&pool, "acct", 13, 9, known.as_bytes(), None, false)
        .await
        .unwrap();
    let inbox::Result::Rows { rows, .. } = list(inbox::InboxFilter::NonCustomer).await.unwrap()
    else {
        panic!("non customer")
    };
    assert_eq!(rows.len(), 1);
    assert!(rows[0].contact_id.is_none());
    let restored_id = rows[0].id.clone();
    let new_mail = known.replace("reply@example.test", "new-reply@example.test");
    store::save(&pool, "acct", 13, 10, new_mail.as_bytes(), None, false)
        .await
        .unwrap();
    store::classify(
        &pool,
        crate::crm::Actor::DESKTOP,
        vec![inbox::HandlingTarget {
            id: rows[0].id.clone(),
            expected_revision: rows[0].handling_revision,
        }],
        false,
    )
    .await
    .unwrap();
    pool.close().await;
    let pool = crate::storage::open(dir.path(), dir.path()).await.unwrap();
    let inbox::Result::Rows { rows, .. } =
        store::list(&pool, None, None, true, 0, None, Default::default())
            .await
            .unwrap()
    else {
        panic!("restored")
    };
    assert_eq!(rows.len(), 2);
    let restored = rows.iter().find(|row| row.id == restored_id).unwrap();
    assert!(!restored.non_customer);
    assert_eq!(restored.handling_revision, 3);
    pool.close().await;
}

async fn fixture() -> (tempfile::TempDir, SqlitePool, String) {
    let dir = tempfile::tempdir().unwrap();
    let pool = crate::storage::open(dir.path(), dir.path()).await.unwrap();
    let response = crate::crm::dispatch(
        &pool,
        crm::Command::SaveContact {
            id: None,
            allow_duplicate: false,
            input: Box::new(ContactInput {
                display_name: "M3 test".into(),
                methods: vec![Method {
                    id: None,
                    kind: "email".into(),
                    value: "buyer@example.test".into(),
                    label: String::new(),
                    is_primary: true,
                    is_verified: true,
                }],
                ..Default::default()
            }),
        },
        crate::crm::Actor::DESKTOP,
    )
    .await;
    let Response::Crm { data } = response else {
        panic!("{response:?}")
    };
    let crm::Result::Saved { id } = *data else {
        panic!("{data:?}")
    };
    sqlx::raw_sql("INSERT INTO local_secrets VALUES('acct',X'00');
        INSERT INTO email_accounts(id,display_name,email_address,smtp_host,smtp_port,smtp_security,smtp_username,secret_ref)
         VALUES('acct','Fixture','sender@example.test','smtp.example.test',465,'tls','sender','acct');")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO email_drafts(id,contact_id,account_id,recipient,subject,body_text,attachments_json,source)
        VALUES('draft',?,'acct','buyer@example.test','Quote','Sent fixture','[]','manual')")
        .bind(&id).execute(&pool).await.unwrap();
    sqlx::raw_sql("INSERT INTO email_approvals(id,draft_id,revision,content_sha256,approved_by_user,expires_at)
        VALUES('approval','draft',1,'fixture','test',unixepoch()+600);
        INSERT INTO email_outbox_jobs(id,draft_id,approval_id,revision,message_id_header,state)
         VALUES('job','draft','approval',1,'<out@ftw.local>','smtp_accepted');").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO email_messages(id,contact_id,account_id,outbox_id,message_id_header,from_address,to_address,subject,body_text,submitted_at)
        VALUES('outgoing',?,'acct','job','<out@ftw.local>','sender@example.test','buyer@example.test','Quote','Sent fixture',1700000000)")
        .bind(&id).execute(&pool).await.unwrap();
    (dir, pool, id)
}

#[tokio::test]
async fn associates_exact_replies_and_deduplicates_after_uidvalidity_change() {
    let (dir, pool, contact) = fixture().await;
    store::save(&pool, "acct", 12, 1, MAIL, Some(1700000100), false)
        .await
        .unwrap();
    store::save(&pool, "acct", 13, 9, MAIL, Some(1700000100), false)
        .await
        .unwrap();
    let inbox::Result::Thread { messages, .. } =
        store::thread(&pool, "sent:outgoing", 0).await.unwrap()
    else {
        panic!("thread")
    };
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].row.contact_id, Some(contact.clone()));
    assert_eq!(messages[1].body.trim(), "Buyer reply.");
    let locations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM inbox_locations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(locations, 2);
    let incoming_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM activities WHERE event_type='email_received'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(incoming_count, 1);

    let spoof = String::from_utf8_lossy(MAIL)
        .replace("buyer@example.test", "unknown@example.test")
        .replace("reply@", "other@");
    store::save(&pool, "acct", 13, 10, spoof.as_bytes(), None, false)
        .await
        .unwrap();
    let inbox::Result::Rows { rows, .. } = store::list(
        &pool,
        Some("acct".into()),
        None,
        false,
        0,
        None,
        inbox::InboxFilter::Unassociated,
    )
    .await
    .unwrap() else {
        panic!("rows")
    };
    assert_eq!(rows.len(), 1);
    store::link(&pool, &rows[0].id, &contact).await.unwrap();
    let inbox::Result::Message { record } = store::get(&pool, &rows[0].id).await.unwrap() else {
        panic!("record")
    };
    assert_eq!(record.row.contact_id, Some(contact.clone()));

    let backup = crate::storage::backup::create(&pool, dir.path(), "manual")
        .await
        .unwrap();
    sqlx::query("DELETE FROM inbox_locations")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM inbox_messages")
        .execute(&pool)
        .await
        .unwrap();
    crate::storage::backup::restore(
        &pool,
        dir.path(),
        backup.file_name().unwrap().to_str().unwrap(),
    )
    .await
    .unwrap();
    let inbox::Result::Thread { messages, .. } =
        store::thread(&pool, "sent:outgoing", 0).await.unwrap()
    else {
        panic!("restored thread")
    };
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].row.contact_id, Some(contact));
    pool.close().await;
}

#[tokio::test]
async fn replies_to_a_followup_stay_in_the_conversation_and_target_that_send() {
    let (_directory, pool, contact) = fixture().await;
    sqlx::query("INSERT INTO email_drafts(id,contact_id,account_id,recipient,subject,body_text,attachments_json,source)
        VALUES('followup',?,'acct','buyer@example.test','Re: Quote','Follow-up','[]','manual')")
        .bind(&contact).execute(&pool).await.unwrap();
    sqlx::raw_sql(
        "INSERT INTO email_approvals(id,draft_id,revision,content_sha256,approved_by_user,expires_at)
         VALUES('followup-approval','followup',1,'fixture','test',unixepoch()+600);
         INSERT INTO email_outbox_jobs(id,draft_id,approval_id,revision,message_id_header,state)
         VALUES('followup-job','followup','followup-approval',1,'<followup@ftw.local>','smtp_accepted');",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO email_messages(id,contact_id,account_id,outbox_id,message_id_header,from_address,to_address,subject,body_text,submitted_at,conversation_id)
        VALUES('followup-sent',?,'acct','followup-job','<followup@ftw.local>','sender@example.test','buyer@example.test',
          'Re: Quote','Follow-up',1700000001,'sent:outgoing')")
        .bind(&contact).execute(&pool).await.unwrap();
    let reply = String::from_utf8_lossy(MAIL).replace("<out@ftw.local>", "<followup@ftw.local>");
    store::save(
        &pool,
        "acct",
        12,
        1,
        reply.as_bytes(),
        Some(1700000100),
        false,
    )
    .await
    .unwrap();
    let row: (String, String, String) =
        sqlx::query_as("SELECT thread_id,match_state,reply_to_sent_id FROM inbox_messages")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        row,
        (
            "sent:outgoing".into(),
            "thread".into(),
            "followup-sent".into()
        )
    );
    let inbox::Result::Thread { messages, .. } =
        store::thread(&pool, "sent:outgoing", 0).await.unwrap()
    else {
        panic!("thread")
    };
    assert_eq!(messages.len(), 3);
    pool.close().await;
}

#[tokio::test]
async fn imap_fetches_body_peek_and_advances_cursor_without_refetching() {
    let (_dir, pool, _contact) = fixture().await;
    let (client, server) = tokio::io::duplex(4096);
    let mock = tokio::spawn(async move {
        let mut server = BufReader::new(server);
        server
            .get_mut()
            .write_all(b"* OK fixture ready\r\n")
            .await
            .unwrap();
        let mut line = String::new();
        let mut fetches = 0;
        while server.read_line(&mut line).await.unwrap() > 0 {
            let (tag, command) = line.split_once(' ').unwrap();
            if command.starts_with("EXAMINE") {
                server
                    .get_mut()
                    .write_all(b"* OK [UIDVALIDITY 301] uid validity\r\n* 1 EXISTS\r\n")
                    .await
                    .unwrap();
            } else if command == "UID SEARCH UID 1:*\r\n" {
                server.get_mut().write_all(b"* SEARCH 7\r\n").await.unwrap();
            } else if command == "UID SEARCH UID 8:*\r\n" {
                // A reversed n:* range can include the most recent UID.
                server.get_mut().write_all(b"* SEARCH 7\r\n").await.unwrap();
            } else if command.starts_with("UID FETCH 7 (UID RFC822.SIZE") {
                fetches += 1;
                let response = format!("* 1 FETCH (UID 7 RFC822.SIZE {} INTERNALDATE \"19-Oct-2025 12:00:00 +0000\")\r\n", MAIL.len());
                server
                    .get_mut()
                    .write_all(response.as_bytes())
                    .await
                    .unwrap();
            } else if command.starts_with("UID FETCH 7 (UID BODY.PEEK[]<0.") {
                fetches += 1;
                let prefix = format!("* 1 FETCH (UID 7 BODY[]<0> {{{}}}\r\n", MAIL.len());
                server.get_mut().write_all(prefix.as_bytes()).await.unwrap();
                server.get_mut().write_all(MAIL).await.unwrap();
                server.get_mut().write_all(b")\r\n").await.unwrap();
            } else if command == "LOGOUT\r\n" {
                server
                    .get_mut()
                    .write_all(format!("* BYE done\r\n{tag} OK logout\r\n").as_bytes())
                    .await
                    .unwrap();
                break;
            } else {
                assert!(command.starts_with("LOGIN"), "{command}");
            }
            server
                .get_mut()
                .write_all(
                    format!(
                        "{tag} OK {}complete\r\n",
                        if command.starts_with("EXAMINE") {
                            "[READ-ONLY] "
                        } else {
                            ""
                        }
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            line.clear();
        }
        fetches
    });
    let mut client = async_imap::Client::new(client);
    client.read_response().await.unwrap();
    let mut session = client.login("fixture", "dummy-local-only").await.unwrap();
    assert!(!transport::collect(&pool, "acct", &mut session)
        .await
        .unwrap());
    assert!(!transport::collect(&pool, "acct", &mut session)
        .await
        .unwrap());
    session.logout().await.unwrap();
    assert_eq!(mock.await.unwrap(), 2);
    let cursor: (i64, i64) = sqlx::query_as(
        "SELECT uid_validity,last_uid FROM imap_sync_state WHERE account_id='acct' AND folder=?",
    )
    .bind(FOLDER)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(cursor, (301, 7));
    pool.close().await;
}
