use super::{
    asset_info, attachment_info, audit, check_recipient, check_suppression, new_id, snapshot,
    Failure, MailService, Reply, Result, Snapshot,
};
use ftw_ipc::crm::Source;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDNO, IDYES, MB_DEFBUTTON2, MB_ICONWARNING, MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO,
};

impl MailService {
    pub(super) async fn open_approval(&self, id: String) -> Result<Reply> {
        let guard = self
            .prompt
            .clone()
            .try_lock_owned()
            .map_err(|_| Failure::Code("approval_already_open"))?;
        let mut db = self.pool.acquire().await?;
        let (reviewed, revision, state) = snapshot(&mut db, &id).await?;
        if !matches!(state.as_str(), "pending_approval" | "approved") {
            return Err(Failure::Code("approval_required"));
        }
        check_recipient(&mut db, &reviewed.contact_id, &reviewed.recipient).await?;
        check_suppression(&mut db, &reviewed.recipient).await?;
        let digest = reviewed.digest()?;
        let description = prompt_text(&reviewed, revision)?;
        let attempt_id = new_id();
        sqlx::query("INSERT INTO email_approval_attempts(id,draft_id,revision,state) VALUES (?,?,?,'waiting')")
            .bind(&attempt_id).bind(&id).bind(revision).execute(&mut *db).await?;
        drop(db);

        let service = self.clone();
        let requested = id.clone();
        let attempt = attempt_id.clone();
        tokio::spawn(async move {
            let _guard = guard;
            let decision = tokio::task::spawn_blocking(move || confirm(&description))
                .await
                .unwrap_or(Confirmation::WindowFailure);
            service
                .complete_attempt(&requested, revision, &digest, &attempt, decision)
                .await;
        });
        Ok(Reply::ApprovalOpened { id, attempt_id })
    }

    async fn complete_attempt(
        &self,
        id: &str,
        revision: i64,
        digest: &str,
        attempt: &str,
        decision: Confirmation,
    ) {
        let (state, reason) = match decision {
            Confirmation::Accepted => match self
                .record_approval(id, revision, digest, Some(attempt))
                .await
            {
                Ok(()) => return,
                Err(error) => ("failed", Some(error.code())),
            },
            Confirmation::Cancelled => ("cancelled", None),
            Confirmation::WindowFailure => ("failed", Some("approval_window_failed")),
        };
        if let Err(error) = finish_attempt(&self.pool, attempt, state, reason).await {
            eprintln!("FTW could not persist approval result: {}", error.code());
        }
    }

    async fn record_approval(
        &self,
        id: &str,
        revision: i64,
        digest: &str,
        attempt: Option<&str>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let (current, current_revision, state) = snapshot(&mut tx, id).await?;
        if current_revision != revision
            || current.digest()? != digest
            || !matches!(state.as_str(), "pending_approval" | "approved")
        {
            return Err(Failure::Code("approval_snapshot_changed"));
        }
        check_recipient(&mut tx, &current.contact_id, &current.recipient).await?;
        check_suppression(&mut tx, &current.recipient).await?;
        sqlx::query("INSERT INTO email_approvals(id,draft_id,revision,content_sha256,approved_by_user,expires_at)
            VALUES (?,?,?,?,'windows-confirmation',unixepoch()+1800)
            ON CONFLICT(draft_id,revision) DO UPDATE SET content_sha256=excluded.content_sha256,
                approved_at=unixepoch(),expires_at=excluded.expires_at")
            .bind(new_id()).bind(id).bind(revision).bind(digest).execute(&mut *tx).await?;
        sqlx::query("UPDATE email_drafts SET state='approved',updated_at=unixepoch() WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        audit(&mut tx, Source::Manual, "email_draft", id, "approve").await?;
        if let Some(attempt) = attempt {
            finish_attempt(&mut *tx, attempt, "approved", None).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn enqueue(&self, id: &str, source: Source) -> Result<Reply> {
        let mut tx = self.pool.begin().await?;
        let (draft, revision, state) = snapshot(&mut tx, id).await?;
        if let Some((job_id, state)) = sqlx::query_as::<_, (String, String)>(
            "SELECT id,state FROM email_outbox_jobs WHERE draft_id=? AND revision=?",
        )
        .bind(id)
        .bind(revision)
        .fetch_optional(&mut *tx)
        .await?
        {
            return Ok(Reply::Queued {
                id: id.to_owned(),
                job_id,
                state,
            });
        }
        if state != "approved" {
            return Err(Failure::Code("approval_required"));
        }
        let (approval, digest): (String, String) = sqlx::query_as(
            "SELECT id,content_sha256 FROM email_approvals WHERE draft_id=? AND revision=? AND expires_at>unixepoch()",
        ).bind(id).bind(revision).fetch_optional(&mut *tx).await?.ok_or(Failure::Code("approval_expired"))?;
        if draft.digest()? != digest {
            return Err(Failure::Code("approval_snapshot_changed"));
        }
        check_recipient(&mut tx, &draft.contact_id, &draft.recipient).await?;
        check_suppression(&mut tx, &draft.recipient).await?;
        let job_id = new_id();
        let message_id = format!("<{job_id}@ftw.local>");
        sqlx::query("INSERT INTO email_outbox_jobs(id,draft_id,approval_id,revision,message_id_header,state,queued_source) VALUES (?,?,?,?,?,'queued',?)")
            .bind(&job_id).bind(id).bind(approval).bind(revision).bind(message_id).bind(source.as_str()).execute(&mut *tx).await?;
        sqlx::query("UPDATE email_drafts SET state='queued', updated_at=unixepoch() WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        audit(&mut tx, source, "email_draft", id, "enqueue").await?;
        tx.commit().await?;
        Ok(Reply::Queued {
            id: id.to_owned(),
            job_id,
            state: "queued".into(),
        })
    }
}

fn prompt_text(draft: &Snapshot, revision: i64) -> Result<String> {
    let mut files = draft
        .attachments
        .iter()
        .map(attachment_info)
        .collect::<Result<Vec<_>>>()?;
    files.extend(draft.assets.iter().map(asset_info));
    let names = files
        .iter()
        .map(|file| format!("{} ({} 字节)", file.filename, file.size))
        .collect::<Vec<_>>()
        .join("、");
    let excerpt: String = draft.body.chars().take(550).collect();
    Ok(format!(
        "请先在 FTW 邮件中心核对这封草稿的完整正文和附件。批准后，工作台或 Agent 可以发送这个固定版本。\n\n发件人：{}\n收件人：{}\n主题：{}\n草稿修订：{}\n附件：{}\n\n正文预览：\n{}{}\n\n批准发送此版本？",
        draft.sender, draft.recipient, draft.subject, revision, if names.is_empty() { "无" } else { &names },
        excerpt, if excerpt.len() < draft.body.len() { "\n…" } else { "" },
    ))
}

async fn finish_attempt<'e>(
    db: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
    id: &str,
    state: &str,
    outcome: Option<&str>,
) -> Result<()> {
    sqlx::query("UPDATE email_approval_attempts SET state=?,outcome=?,finished_at=unixepoch() WHERE id=? AND state='waiting'")
        .bind(state).bind(outcome).bind(id).execute(db).await?;
    Ok(())
}

enum Confirmation {
    Accepted,
    Cancelled,
    WindowFailure,
}

fn confirm(description: &str) -> Confirmation {
    let title: Vec<_> = "FTW 工作台 · 人工邮件审批"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let text: Vec<_> = description.encode_utf16().chain(Some(0)).collect();
    // This call runs on a blocking thread and both buffers outlive the native dialog.
    let choice = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_DEFBUTTON2 | MB_ICONWARNING | MB_SETFOREGROUND | MB_TOPMOST,
        )
    };
    match choice {
        IDYES => Confirmation::Accepted,
        IDNO => Confirmation::Cancelled,
        _ => Confirmation::WindowFailure,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mail::smtp;
    use ftw_ipc::{
        crm::{self, ContactInput, Method},
        mail::AgentCommand,
        Response,
    };
    use lettre::{AsyncSmtpTransport, Tokio1Executor};
    use std::time::Duration;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::TcpListener,
    };

    async fn setup() -> (tempfile::TempDir, MailService, String) {
        let directory = tempfile::tempdir().unwrap();
        let pool = crate::storage::open(directory.path(), directory.path())
            .await
            .unwrap();
        let result = crate::crm::dispatch(
            &pool,
            crm::Command::SaveContact {
                id: None,
                input: Box::new(ContactInput {
                    display_name: "SMTP fixture".into(),
                    methods: vec![Method {
                        id: None,
                        kind: "email".into(),
                        value: "receiver@example.test".into(),
                        label: String::new(),
                        is_primary: true,
                        is_verified: true,
                    }],
                    ..Default::default()
                }),
                allow_duplicate: false,
            },
            crate::crm::Actor::DESKTOP,
        )
        .await;
        let Response::Crm { data } = result else {
            panic!("{result:?}")
        };
        let crm::Result::Saved { id } = *data else {
            panic!("{data:?}")
        };

        sqlx::query("INSERT INTO local_secrets(id,protected) VALUES ('acct',X'00')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO email_accounts(id,display_name,email_address,smtp_host,smtp_port,smtp_security,smtp_username,secret_ref)
            VALUES ('acct','Fixture','sender@example.test','smtp.example.test',465,'tls','sender','acct')")
            .execute(&pool).await.unwrap();
        let service = MailService::new(pool).await.unwrap();
        (directory, service, id)
    }

    async fn new_draft(service: &MailService, contact_id: String) -> String {
        let result = service
            .agent(AgentCommand::CreateDraft {
                request_id: None,
                contact_id,
                account_id: "acct".into(),
                recipient: "receiver@example.test".into(),
                subject: "Approved test email".into(),
                body: "Fixture body".into(),
            })
            .await;
        let Response::Mail { data } = result else {
            panic!("{result:?}")
        };
        let Reply::Saved { id } = *data else {
            panic!("{data:?}")
        };
        id
    }

    async fn submit_review(service: &MailService, draft: &str) -> (i64, String) {
        service
            .request_approval(draft, Source::Agent)
            .await
            .unwrap();
        let mut connection = service.pool.acquire().await.unwrap();
        let (snapshot, revision, _) = snapshot(&mut connection, draft).await.unwrap();
        (revision, snapshot.digest().unwrap())
    }

    #[tokio::test]
    async fn agent_approval_immutability_and_idempotent_smtp_submission() {
        let (_directory, service, contact) = setup().await;
        let draft = new_draft(&service, contact.clone()).await;
        assert!(matches!(
            service.agent(AgentCommand::SendApproved { id: draft.clone() }).await,
            Response::Error { code, .. } if code == "approval_required"
        ));
        let (old_revision, old_digest) = submit_review(&service, &draft).await;
        service
            .record_approval(&draft, old_revision, &old_digest, None)
            .await
            .unwrap();
        service
            .save(
                Some(draft.clone()),
                ftw_ipc::mail::DraftInput {
                    contact_id: contact,
                    account_id: "acct".into(),
                    recipient: "receiver@example.test".into(),
                    subject: "Approved test email".into(),
                    body: "Approved final text".into(),
                    attachments: Vec::new(),
                    replace_attachments: false,
                },
                Source::Manual,
            )
            .await
            .unwrap();
        let (revision, digest) = submit_review(&service, &draft).await;
        assert!(matches!(
            service
                .record_approval(&draft, old_revision, &old_digest, None)
                .await,
            Err(Failure::Code("approval_snapshot_changed"))
        ));
        service
            .record_approval(&draft, revision, &digest, None)
            .await
            .unwrap();
        let queued = service.enqueue(&draft, Source::Agent).await.unwrap();
        let retried = service.enqueue(&draft, Source::Agent).await.unwrap();
        assert!(
            matches!((queued, retried), (Reply::Queued {job_id: a,..}, Reply::Queued {job_id:b,..}) if a==b)
        );

        let (port, fake) = fake_server(false).await;
        service
            .send_next_with(|_| Ok(test_transport(port)))
            .await
            .unwrap();
        let wire = tokio::time::timeout(Duration::from_secs(4), fake)
            .await
            .unwrap()
            .unwrap();
        assert!(wire.contains("Approved final text") && !wire.contains("Fixture body"));
        service
            .send_next_with(|_| panic!("the same approved draft would be sent twice"))
            .await
            .unwrap();
        let (status, attempts): (String, i64) =
            sqlx::query_as("SELECT state,attempt_count FROM email_outbox_jobs")
                .fetch_one(&service.pool)
                .await
                .unwrap();
        assert_eq!((status.as_str(), attempts), ("smtp_accepted", 1));
        let entries: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM activities WHERE event_type='email_sent'")
                .fetch_one(&service.pool)
                .await
                .unwrap();
        assert_eq!(entries, 1);
        service.pool.close().await;
    }

    #[tokio::test]
    async fn smtp_acceptance_unknown_is_persisted_without_retry() {
        let (_directory, service, contact) = setup().await;
        let draft = new_draft(&service, contact).await;
        let (revision, digest) = submit_review(&service, &draft).await;
        service
            .record_approval(&draft, revision, &digest, None)
            .await
            .unwrap();
        service.enqueue(&draft, Source::Agent).await.unwrap();
        let (port, fake) = fake_server(true).await;
        service
            .send_next_with(|_| Ok(test_transport(port)))
            .await
            .unwrap();
        assert!(fake.await.unwrap().contains("Fixture body"));
        let ended = MailService::new(service.pool.clone()).await.unwrap();
        assert!(
            matches!(ended.enqueue(&draft, Source::Agent).await.unwrap(),
            Reply::Queued {state,..} if state == "delivery_unknown")
        );
        ended
            .send_next_with(|_| panic!("unknown result cannot be retried"))
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM email_messages")
            .fetch_one(&service.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        service.pool.close().await;
    }

    #[tokio::test]
    async fn suppressing_a_queued_recipient_prevents_network_submission() {
        let (_directory, service, contact) = setup().await;
        let draft = new_draft(&service, contact).await;
        let (revision, digest) = submit_review(&service, &draft).await;
        service
            .record_approval(&draft, revision, &digest, None)
            .await
            .unwrap();
        service.enqueue(&draft, Source::Agent).await.unwrap();
        sqlx::query("INSERT INTO suppression_list(normalized) VALUES ('example.test')")
            .execute(&service.pool)
            .await
            .unwrap();
        service
            .send_next_with(|_| panic!("suppression must occur before SMTP transport"))
            .await
            .unwrap();
        let outcome: String = sqlx::query_scalar("SELECT outcome FROM email_outbox_jobs")
            .fetch_one(&service.pool)
            .await
            .unwrap();
        assert_eq!(outcome, "recipient_suppressed");
        service.pool.close().await;
    }

    #[tokio::test]
    async fn daily_limit_counts_unconfirmed_attempts_before_smtp_submission() {
        let (_directory, service, contact) = setup().await;
        sqlx::query("UPDATE email_accounts SET daily_limit=1 WHERE id='acct'")
            .execute(&service.pool)
            .await
            .unwrap();
        let first = new_draft(&service, contact.clone()).await;
        let (revision, digest) = submit_review(&service, &first).await;
        service
            .record_approval(&first, revision, &digest, None)
            .await
            .unwrap();
        service.enqueue(&first, Source::Agent).await.unwrap();
        let (port, server) = fake_server(true).await;
        service
            .send_next_with(|_| Ok(test_transport(port)))
            .await
            .unwrap();
        server.await.unwrap();

        let second = new_draft(&service, contact).await;
        let (revision, digest) = submit_review(&service, &second).await;
        service
            .record_approval(&second, revision, &digest, None)
            .await
            .unwrap();
        service.enqueue(&second, Source::Agent).await.unwrap();
        service
            .send_next_with(|_| panic!("account limit must be checked before connecting"))
            .await
            .unwrap();
        let outcome: String =
            sqlx::query_scalar("SELECT outcome FROM email_outbox_jobs WHERE draft_id=?")
                .bind(second)
                .fetch_one(&service.pool)
                .await
                .unwrap();
        assert_eq!(outcome, "smtp_daily_limit");
        service.pool.close().await;
    }

    #[tokio::test]
    async fn restore_requires_new_approval_and_never_replays_a_queued_job() {
        let (directory, service, contact) = setup().await;
        let approved = new_draft(&service, contact.clone()).await;
        let queued = new_draft(&service, contact).await;
        for draft in [&approved, &queued] {
            let (revision, digest) = submit_review(&service, draft).await;
            service
                .record_approval(draft, revision, &digest, None)
                .await
                .unwrap();
        }
        service.enqueue(&queued, Source::Agent).await.unwrap();
        let backup = crate::storage::backup::create(&service.pool, directory.path(), "manual")
            .await
            .unwrap();
        service
            .restore(
                directory.path(),
                backup.file_name().unwrap().to_str().unwrap(),
            )
            .await
            .unwrap();

        assert!(matches!(
            service.enqueue(&approved, Source::Agent).await,
            Err(Failure::Code("approval_required"))
        ));
        service
            .send_next_with(|_| panic!("a restored job must not be submitted"))
            .await
            .unwrap();
        let job: (String, String) =
            sqlx::query_as("SELECT state,outcome FROM email_outbox_jobs WHERE draft_id=?")
                .bind(queued)
                .fetch_one(&service.pool)
                .await
                .unwrap();
        assert_eq!(
            job,
            ("delivery_unknown".into(), "restored_without_resend".into())
        );
        service.pool.close().await;
    }

    #[tokio::test]
    async fn review_receipts_distinguish_cancelled_failure_approved_and_expired() {
        let (_dir, service, contact) = setup().await;
        let draft = new_draft(&service, contact).await;
        let (revision, digest) = submit_review(&service, &draft).await;
        for (key, decision, expected) in [
            ("cancel", Confirmation::Cancelled, "cancelled"),
            ("window-failed", Confirmation::WindowFailure, "failed"),
            ("changed", Confirmation::Accepted, "failed"),
            ("approved", Confirmation::Accepted, "approved"),
        ] {
            sqlx::query("INSERT INTO email_approval_attempts(id,draft_id,revision,state) VALUES(?,?,?,'waiting')")
                .bind(key).bind(&draft).bind(revision).execute(&service.pool).await.unwrap();
            let checked_digest = if key == "changed" {
                "obsolete"
            } else {
                &digest
            };
            service
                .complete_attempt(&draft, revision, checked_digest, key, decision)
                .await;
            let Reply::Draft { record } = service.get(&draft).await.unwrap() else {
                panic!("draft receipt")
            };
            let receipt = record.approval_attempt.unwrap();
            assert_eq!(receipt.id, key);
            assert_eq!(receipt.state, expected);
            if key == "changed" {
                assert_eq!(
                    receipt.outcome.as_deref(),
                    Some("approval_snapshot_changed")
                );
            }
        }
        let before = crate::storage::changes(&service.pool).await.unwrap();
        assert_eq!(before.domains["mail_expiration"], 0);
        sqlx::query("UPDATE email_approvals SET expires_at=0 WHERE draft_id=?")
            .bind(&draft)
            .execute(&service.pool)
            .await
            .unwrap();
        let after = crate::storage::changes(&service.pool).await.unwrap();
        assert_eq!(after.domains["mail_expiration"], 1);
        let Reply::Drafts { rows, .. } = service.pending(0).await.unwrap() else {
            panic!("pending approvals")
        };
        assert_eq!(rows.len(), 1);
        assert!(rows[0].approval_expired);
        assert!(matches!(
            service.enqueue(&draft, Source::Agent).await,
            Err(Failure::Code("approval_expired"))
        ));
        service.pool.close().await;
    }

    fn test_transport(port: u16) -> AsyncSmtpTransport<Tokio1Executor> {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous("127.0.0.1")
            .port(port)
            .timeout(Some(Duration::from_secs(3)))
            .build()
    }

    async fn fake_server(drop_after_data: bool) -> (u16, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut stream = BufReader::new(stream);
            stream
                .get_mut()
                .write_all(b"220 fixture.example SMTP\r\n")
                .await
                .unwrap();
            let mut body = String::new();
            let mut in_data = false;
            let mut line = String::new();
            while stream.read_line(&mut line).await.unwrap() > 0 {
                let reply = if in_data {
                    if line == ".\r\n" {
                        if drop_after_data {
                            break;
                        }
                        in_data = false;
                        "250 accepted\r\n"
                    } else {
                        body.push_str(&line);
                        line.clear();
                        continue;
                    }
                } else if line == "DATA\r\n" {
                    in_data = true;
                    "354 continue\r\n"
                } else if line == "QUIT\r\n" {
                    stream.get_mut().write_all(b"221 bye\r\n").await.unwrap();
                    break;
                } else {
                    "250 ok\r\n"
                };
                stream.get_mut().write_all(reply.as_bytes()).await.unwrap();
                line.clear();
            }
            body
        });
        (port, server)
    }

    #[test]
    fn mime_contains_approved_attachment_bytes() {
        let snapshot = Snapshot {
            contact_id: "id".into(),
            account_id: "acct".into(),
            sender: "sender@example.test".into(),
            recipient: "receiver@example.test".into(),
            subject: "Attachment review".into(),
            body: "Review body".into(),
            assets: vec![],
            attachments: vec![ftw_ipc::mail::Attachment {
                filename: "example.txt".into(),
                content_base64: "Ym9keQ==".into(),
            }],
        };
        let rendered = smtp::message(&snapshot, "<test@ftw.local>".into(), Vec::new()).unwrap();
        let mime = String::from_utf8(rendered.formatted()).unwrap();
        assert!(
            mime.contains("example.txt")
                && (mime.contains("\r\nbody\r\n") || mime.contains("Ym9keQ=="))
        );
        let mut edited = snapshot;
        let previous = edited.digest().unwrap();
        edited.attachments[0].content_base64 = "YWJj".into();
        assert_ne!(previous, edited.digest().unwrap());
    }
}
