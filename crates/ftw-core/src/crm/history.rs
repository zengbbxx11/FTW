use super::{page, Actor, Result, PAGE_SIZE};
use ftw_ipc::crm::{
    ContactRevision, ContactRevisionField, CustomFieldChange, Result as Reply, Source,
};
use serde_json::{Map, Value};
use sqlx::{sqlite::SqliteRow, FromRow, Row, SqlitePool};

#[derive(FromRow)]
struct RevisionRow {
    revision: i64,
    actor_id: String,
    actor_name: String,
    source: String,
    occurred_at: i64,
    before_json: String,
    after_json: String,
}

pub async fn list(pool: &SqlitePool, id: &str, offset: u32) -> Result<Reply> {
    let rows: Vec<RevisionRow> = sqlx::query_as(
        "SELECT h.*,coalesce(a.name,h.actor_id) actor_name,
            CASE h.actor_id WHEN 'desktop-user' THEN 'manual' WHEN 'system' THEN 'system' ELSE 'agent' END source
         FROM contact_revision_history h LEFT JOIN mail_agent_clients a ON a.id=h.actor_id
         WHERE h.contact_id=? ORDER BY h.revision DESC LIMIT ? OFFSET ?",
    )
    .bind(id)
    .bind((PAGE_SIZE + 1) as i64)
    .bind(i64::from(offset))
    .fetch_all(pool)
    .await?;
    let (rows, has_more) = page(rows);
    let rows = rows
        .into_iter()
        .map(|row| {
            let before = decode(&row.before_json)?;
            let after = decode(&row.after_json)?;
            let fields = before
                .into_iter()
                .filter_map(|(field, before)| {
                    let after = after.get(&field)?;
                    (after != &before).then(|| ContactRevisionField {
                        field,
                        before,
                        after: after.clone(),
                    })
                })
                .collect();
            Ok(ContactRevision {
                revision: row.revision,
                actor_id: row.actor_id,
                actor_name: row.actor_name,
                source: row.source,
                occurred_at: row.occurred_at,
                fields,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Reply::ContactHistory { rows, has_more })
}

fn decode(value: &str) -> Result<Map<String, Value>> {
    serde_json::from_str(value).map_err(|error| sqlx::Error::Decode(Box::new(error)).into())
}

pub async fn custom_fields(
    pool: &SqlitePool,
    actor: Actor<'_>,
    id: &str,
    offset: u32,
) -> Result<Reply> {
    let rows = sqlx::query(
        "SELECT h.*,coalesce(a.name,h.actor_id) actor_name
        FROM contact_custom_history h JOIN crm_fields f ON f.id=h.field_id
        LEFT JOIN mail_agent_clients a ON a.id=h.actor_id
        WHERE h.contact_id=? AND h.before_json<>h.after_json AND (? OR (NOT f.archived AND f.agent_access<>'hidden'))
        ORDER BY h.revision DESC,h.field_id LIMIT ? OFFSET ?",
    )
    .bind(id)
    .bind(matches!(actor.source, Source::Manual))
    .bind((PAGE_SIZE + 1) as i64)
    .bind(i64::from(offset))
    .try_map(|row: SqliteRow| {
        let value = |name| {
            serde_json::from_str::<Option<ftw_ipc::fields::Value>>(row.get(name))
                .map_err(|error| sqlx::Error::Decode(Box::new(error)))
        };
        Ok(CustomFieldChange {
            revision: row.get("revision"),
            field_id: row.get("field_id"),
            field_name: row.get("field_name"),
            actor_id: row.get("actor_id"),
            actor_name: row.get("actor_name"),
            source: row.get("source"),
            task_id: row.get("task_id"),
            before: value("before_json")?,
            after: value("after_json")?,
            occurred_at: row.get("occurred_at"),
        })
    })
    .fetch_all(pool)
    .await?;
    let (rows, has_more) = page(rows);
    Ok(Reply::CustomFieldHistory { rows, has_more })
}
