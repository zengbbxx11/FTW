use super::{check_recipient, email_address, DraftInput, Failure, MailService, Reply, Result};
use crate::crm::Actor;
use ftw_ipc::mail::ReplyDraftInput;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqliteConnection};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Context {
    pub inbox_id: String,
    pub thread_id: String,
    pub parent: String,
    pub root: Option<String>,
}

#[derive(FromRow)]
pub(super) struct SourceMail {
    pub contact_id: String,
    pub account_id: String,
    pub sender: String,
    pub subject: String,
    thread_id: String,
    message_id_header: String,
}

impl MailService {
    pub(super) async fn create_reply_draft(
        &self,
        input: ReplyDraftInput,
        actor: Actor<'_>,
        asset_ids: Option<Vec<String>>,
    ) -> Result<Reply> {
        let draft = DraftInput {
            contact_id: String::new(),
            account_id: String::new(),
            recipient: String::new(),
            subject: input.subject.unwrap_or_default(),
            body: input.body,
            attachments: Vec::new(),
            replace_attachments: false,
        };
        self.persist_draft(
            None,
            draft,
            actor,
            asset_ids,
            None,
            Some(&input.inbox_message_id),
        )
        .await
    }
}

pub(super) async fn resolve(
    db: &mut SqliteConnection,
    inbox_id: &str,
) -> Result<(SourceMail, Context)> {
    let source: SourceMail = sqlx::query_as(
        "SELECT i.contact_id,i.account_id,i.sender,i.subject,i.thread_id,i.message_id_header
         FROM inbox_messages i JOIN contacts c ON c.id=i.contact_id JOIN email_accounts a ON a.id=i.account_id
         WHERE i.id=? AND i.message_role='correspondence' AND i.match_state IN ('thread','manual')
           AND c.archived_at IS NULL AND a.archived_at IS NULL",
    ).bind(inbox_id).fetch_optional(&mut *db).await?.ok_or(Failure::Code("mail_reply_link_required"))?;
    let recipient = email_address(&source.sender)?;
    check_recipient(db, &source.contact_id, &recipient).await?;
    let parent =
        message_id(&source.message_id_header).ok_or(Failure::Code("mail_reply_header_missing"))?;
    let root: Option<String> = sqlx::query_scalar(
        "SELECT message_id_header FROM email_messages WHERE 'sent:' || id=?1 AND contact_id=?2 AND account_id=?3
         UNION ALL SELECT message_id_header FROM inbox_messages WHERE 'inbox:' || id=?1 AND contact_id=?2 AND account_id=?3",
    ).bind(&source.thread_id).bind(&source.contact_id).bind(&source.account_id).fetch_optional(db).await?;
    let root = root
        .as_deref()
        .and_then(message_id)
        .filter(|id| id != &parent);
    let context = Context {
        inbox_id: inbox_id.into(),
        thread_id: source.thread_id.clone(),
        parent,
        root,
    };
    Ok((source, context))
}

pub(super) fn decode(value: Option<String>) -> Result<Option<Context>> {
    value
        .map(|encoded| {
            serde_json::from_str(&encoded).map_err(|_| Failure::Code("mail_reply_context_changed"))
        })
        .transpose()
}

pub(super) async fn validate(
    db: &mut SqliteConnection,
    context: &Context,
    contact: &str,
    account: &str,
    recipient: &str,
) -> Result<()> {
    let (source, current) = resolve(db, &context.inbox_id).await?;
    if current != *context
        || source.contact_id != contact
        || source.account_id != account
        || !source.sender.trim().eq_ignore_ascii_case(recipient)
    {
        return Err(Failure::Code("mail_reply_context_changed"));
    }
    Ok(())
}

pub(super) fn subject(original: &str) -> String {
    let clean: String = original
        .trim()
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    let mut subject = if clean
        .get(..3)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("re:"))
    {
        String::new()
    } else {
        "Re: ".into()
    };
    for ch in clean.chars() {
        if subject.len() + ch.len_utf8() > 360 {
            break;
        }
        subject.push(ch);
    }
    if subject.trim().eq_ignore_ascii_case("re:") {
        subject.push_str("(no subject)");
    }
    subject
}

fn message_id(value: &str) -> Option<String> {
    let bare = value
        .strip_prefix('<')
        .and_then(|value| value.strip_suffix('>'))
        .unwrap_or(value);
    let (local, domain) = bare.split_once('@')?;
    if local.is_empty()
        || domain.is_empty()
        || bare.len() > 480
        || domain.contains('@')
        || !bare
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && !b"<>\\\"(),;".contains(&byte))
    {
        return None;
    }
    Some(format!("<{bare}>"))
}
