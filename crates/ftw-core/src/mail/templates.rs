use super::{audit_as, check_message, check_text, new_id, policy, Failure, Reply, Result};
use crate::crm::Actor;
use ftw_ipc::mail::{Template, TemplateChange, TemplateInput};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

const VARIABLES: &[&str] = &[
    "contact_name",
    "company_name",
    "country_region",
    "product_interest",
];

fn check_purpose(purpose: &str) -> Result<()> {
    if policy::PURPOSES.contains(&purpose) {
        Ok(())
    } else {
        Err(Failure::Code("invalid_template_purpose"))
    }
}

fn validate(input: &TemplateInput) -> Result<()> {
    check_text(&input.name, 140)?;
    check_purpose(&input.purpose)?;
    check_message(&input.subject, &input.body)?;
    for value in [&input.subject, &input.body] {
        interpolate(value, |_| String::new())?;
    }
    Ok(())
}

fn interpolate(mut value: &str, mut replace: impl FnMut(&str) -> String) -> Result<String> {
    let mut result = String::with_capacity(value.len());
    while let Some((literal, after_open)) = value.split_once("{{") {
        result.push_str(literal);
        let (name, tail) = after_open
            .split_once("}}")
            .ok_or(Failure::Code("invalid_template_variable"))?;
        let name = name.trim();
        if !VARIABLES.contains(&name) {
            return Err(Failure::Code("invalid_template_variable"));
        }
        result.push_str(&replace(name));
        value = tail;
    }
    result.push_str(value);
    Ok(result)
}

pub(super) fn ensure_resolved(subject: &str, body: &str) -> Result<()> {
    let unresolved = [subject, body].iter().any(|text| {
        text.split("{{").skip(1).any(|tail| {
            tail.trim_start()
                .chars()
                .next()
                .is_none_or(|c| c == '_' || c.is_ascii_alphanumeric())
        })
    });
    if unresolved {
        Err(Failure::Code("mail_unresolved_template"))
    } else {
        Ok(())
    }
}

fn row(record: SqliteRow) -> Template {
    Template {
        id: record.get("id"),
        revision: record.get("revision"),
        archived_at: record.get("archived_at"),
        input: TemplateInput {
            name: record.get("name"),
            purpose: record.get("purpose"),
            subject: record.get("subject"),
            body: record.get("body"),
        },
    }
}

pub(super) async fn list(
    pool: &SqlitePool,
    purpose: &str,
    offset: u32,
    archived: bool,
) -> Result<Reply> {
    if !purpose.is_empty() {
        check_purpose(purpose)?;
    }
    let mut rows = sqlx::query(
        "SELECT * FROM mail_templates WHERE (archived_at IS NOT NULL)=?1 AND (?2='' OR purpose=?2)
        ORDER BY name COLLATE NOCASE,id LIMIT 21 OFFSET ?3",
    )
    .bind(archived)
    .bind(purpose)
    .bind(i64::from(offset))
    .map(row)
    .fetch_all(pool)
    .await?;
    let has_more = rows.len() > 20;
    rows.truncate(20);
    Ok(Reply::Templates { rows, has_more })
}

pub(super) async fn find(pool: &SqlitePool, id: &str) -> Result<Template> {
    sqlx::query("SELECT * FROM mail_templates WHERE id=?")
        .bind(id)
        .map(row)
        .fetch_optional(pool)
        .await?
        .ok_or(Failure::Code("mail_not_found"))
}

pub(super) async fn mutate(
    pool: &SqlitePool,
    actor: Actor<'_>,
    change: TemplateChange,
) -> Result<(String, i64)> {
    match change {
        TemplateChange::Save {
            id,
            expected_revision,
            input,
        } => save(pool, actor, id, expected_revision, input).await,
        TemplateChange::Archive {
            id,
            expected_revision,
            archived,
        } => archive(pool, actor, id, expected_revision, archived).await,
    }
}

async fn save(
    pool: &SqlitePool,
    actor: Actor<'_>,
    id: Option<String>,
    expected: Option<i64>,
    input: TemplateInput,
) -> Result<(String, i64)> {
    validate(&input)?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let (id, revision, operation) = match (id, expected) {
        (Some(id), Some(revision)) => {
            let revision = sqlx::query_scalar("UPDATE mail_templates SET name=?,purpose=?,subject=?,body=?,revision=revision+1,updated_at=unixepoch()
                WHERE id=? AND revision=? AND archived_at IS NULL RETURNING revision")
                .bind(input.name.trim()).bind(&input.purpose).bind(input.subject.trim()).bind(&input.body)
                .bind(&id).bind(revision).fetch_optional(&mut *tx).await?
                .ok_or(Failure::Code("template_revision_conflict"))?;
            (id, revision, "edit")
        }
        (None, None) => {
            let id = new_id();
            let revision = sqlx::query_scalar(
                "INSERT INTO mail_templates(id,name,purpose,subject,body) VALUES (?,?,?,?,?) RETURNING revision",
            )
            .bind(&id)
            .bind(input.name.trim())
            .bind(&input.purpose)
            .bind(input.subject.trim())
            .bind(&input.body)
            .fetch_one(&mut *tx)
            .await?;
            (id, revision, "create")
        }
        _ => return Err(Failure::Code("template_revision_conflict")),
    };
    audit_as(
        &mut tx,
        actor.source,
        actor.id,
        "email_template",
        &id,
        operation,
        actor.agent_run_id,
    )
    .await?;
    tx.commit().await?;
    Ok((id, revision))
}

async fn archive(
    pool: &SqlitePool,
    actor: Actor<'_>,
    id: String,
    revision: i64,
    archived: bool,
) -> Result<(String, i64)> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let revision = sqlx::query_scalar(
        "UPDATE mail_templates SET archived_at=CASE WHEN ? THEN unixepoch() END,
        updated_at=unixepoch(),revision=revision+1 WHERE id=? AND revision=? RETURNING revision",
    )
    .bind(archived)
    .bind(&id)
    .bind(revision)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Failure::Code("template_revision_conflict"))?;
    audit_as(
        &mut tx,
        actor.source,
        actor.id,
        "email_template",
        &id,
        if archived { "archive" } else { "restore" },
        actor.agent_run_id,
    )
    .await?;
    tx.commit().await?;
    Ok((id, revision))
}

pub(super) async fn render_saved(pool: &SqlitePool, id: &str, contact: &str) -> Result<Reply> {
    let template = sqlx::query("SELECT * FROM mail_templates WHERE id=? AND archived_at IS NULL")
        .bind(id)
        .map(row)
        .fetch_optional(pool)
        .await?
        .ok_or(Failure::Code("mail_not_found"))?;
    render(pool, template.input, contact).await
}

pub(super) async fn render(
    pool: &SqlitePool,
    template: TemplateInput,
    contact: &str,
) -> Result<Reply> {
    validate(&template)?;
    let mut tx = pool.begin().await?;
    let (name, company, country): (String, String, String) = sqlx::query_as(
        "SELECT display_name,company_name,country_region FROM contacts WHERE id=? AND archived_at IS NULL"
    ).bind(contact).fetch_optional(&mut *tx).await?.ok_or(Failure::Code("crm_not_found"))?;
    let products: Vec<String> = sqlx::query_scalar(
        "SELECT product_text FROM customer_interests WHERE contact_id=? ORDER BY position,id",
    )
    .bind(contact)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let values = [name, company, country, products.join(", ")];
    let mut missing = Vec::new();
    let mut substitute = |key: &str| {
        let value = VARIABLES
            .iter()
            .zip(&values)
            .find(|(name, _)| **name == key)
            .map(|(_, value)| value.trim())
            .unwrap_or_default();
        if value.is_empty() {
            if !missing.iter().any(|name| name == key) {
                missing.push(key.to_owned());
            }
            format!("{{{{{key}}}}}")
        } else {
            value.to_owned()
        }
    };
    let subject = interpolate(&template.subject, &mut substitute)?;
    let body = interpolate(&template.body, &mut substitute)?;
    check_message(&subject, &body)?;
    if missing.is_empty() {
        ensure_resolved(&subject, &body)?;
    }
    Ok(Reply::RenderedTemplate {
        subject,
        body,
        missing,
    })
}
