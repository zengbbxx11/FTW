use super::{AccountRow, Failure, Snapshot};
use lettre::{
    message::{Attachment, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use std::time::Duration;

pub(super) fn transport(
    account: &AccountRow,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, Failure> {
    let builder = if account.smtp_security == "tls" {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&account.smtp_host)
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&account.smtp_host)
    }
    .map_err(|_| Failure::Code("smtp_configuration_invalid"))?;
    let password = super::credentials::read(&account.protected)
        .map_err(|_| Failure::Code("credential_unavailable"))?;
    Ok(builder
        .port(account.smtp_port as u16)
        .credentials(Credentials::new(account.smtp_username.clone(), password))
        .timeout(Some(Duration::from_secs(15)))
        .build())
}

pub(super) fn message(
    snapshot: &Snapshot,
    message_id: String,
    assets: Vec<Vec<u8>>,
) -> Result<Message, Failure> {
    let from = snapshot
        .sender
        .parse()
        .map_err(|_| Failure::Code("invalid_mail_address"))?;
    let to = snapshot
        .recipient
        .parse()
        .map_err(|_| Failure::Code("invalid_mail_address"))?;
    let builder = Message::builder()
        .from(from)
        .to(to)
        .subject(&snapshot.subject)
        .message_id(Some(message_id));
    if snapshot.attachments.is_empty() && assets.is_empty() {
        return builder
            .body(snapshot.body.clone())
            .map_err(|_| Failure::Code("smtp_message_invalid"));
    }
    let mut parts = MultiPart::mixed().singlepart(SinglePart::plain(snapshot.body.clone()));
    for attachment in &snapshot.attachments {
        let name = attachment.filename.clone();
        let bytes = super::attachment_bytes(attachment)?;
        parts = parts.singlepart(
            Attachment::new(name).body(
                bytes,
                lettre::message::header::ContentType::parse("application/octet-stream")
                    .map_err(|_| Failure::Code("smtp_message_invalid"))?,
            ),
        );
    }
    for (asset, bytes) in snapshot.assets.iter().zip(assets) {
        parts = parts.singlepart(
            Attachment::new(asset.filename.clone()).body(
                bytes,
                lettre::message::header::ContentType::parse(&asset.content_type)
                    .map_err(|_| Failure::Code("smtp_message_invalid"))?,
            ),
        );
    }
    builder
        .multipart(parts)
        .map_err(|_| Failure::Code("smtp_message_invalid"))
}

pub(super) async fn send(
    transport: &AsyncSmtpTransport<Tokio1Executor>,
    message: Message,
) -> (&'static str, &'static str) {
    match transport.send(message).await {
        Ok(_) => ("smtp_accepted", "smtp_accepted"),
        Err(error) if error.is_permanent() || error.is_transient() => ("failed", "smtp_rejected"),
        Err(_) => ("delivery_unknown", "smtp_result_unknown"),
    }
}
