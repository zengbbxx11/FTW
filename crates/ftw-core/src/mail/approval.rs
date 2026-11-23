use super::{
    attachment_info, audit, check_recipient, check_suppression, new_id, snapshot, Failure,
    MailService, Reply, Result, Snapshot,
};
use ftw_ipc::crm::Source;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_DEFBUTTON2, MB_ICONWARNING, MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO,
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
        drop(db);

        let service = self.clone();
        let requested = id.clone();
        tokio::spawn(async move {
            let _guard = guard;
            // The Windows dialog produces the decision inside Core. IPC carries only the draft ID.
            if tokio::task::spawn_blocking(move || confirm(&description))
                .await
                .unwrap_or(false)
            {
                let _ = service.record_approval(&requested, revision, &digest).await;
            }
        });
        Ok(Reply::ApprovalOpened { id })
    }

    async fn record_approval(&self, id: &str, revision: i64, digest: &str) -> Result<()> {
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
        sqlx::query("INSERT INTO email_outbox_jobs(id,draft_id,approval_id,revision,message_id_header,state) VALUES (?,?,?,?,?,'queued')")
            .bind(&job_id).bind(id).bind(approval).bind(revision).bind(message_id).execute(&mut *tx).await?;
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
    let files = draft
        .attachments
        .iter()
        .map(attachment_info)
        .collect::<Result<Vec<_>>>()?;
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

fn confirm(description: &str) -> bool {
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
    choice == IDYES
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
            .record_approval(&draft, old_revision, &old_digest)
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
                .record_approval(&draft, old_revision, &old_digest)
                .await,
            Err(Failure::Code("approval_snapshot_changed"))
        ));
        service
            .record_approval(&draft, revision, &digest)
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
            .record_approval(&draft, revision, &digest)
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
            .record_approval(&draft, revision, &digest)
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
            attachments: vec![ftw_ipc::mail::Attachment {
                filename: "example.txt".into(),
                content_base64: "Ym9keQ==".into(),
            }],
        };
        let rendered = smtp::message(&snapshot, "<test@ftw.local>".into()).unwrap();
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
