use super::{audit, id_or_new, page, validation, Actor, Error, Result, PAGE_SIZE};
use ftw_ipc::crm::{Activity, ActivityInput, Result as Reply};
use sqlx::{sqlite::SqliteRow, QueryBuilder, Row, SqlitePool};

pub async fn list(
    pool: &SqlitePool,
    contact: Option<String>,
    event_type: &str,
    offset: u32,
) -> Result<Reply> {
    let mut query = QueryBuilder::new(
        "SELECT a.*, c.display_name AS contact_name FROM activities a JOIN contacts c ON c.id=a.contact_id WHERE 1=1",
    );
    if let Some(contact) = contact {
        query.push(" AND a.contact_id=").push_bind(contact);
    }
    if !event_type.is_empty() {
        query.push(" AND a.event_type=").push_bind(event_type);
    }
    query
        .push(" ORDER BY a.occurred_at DESC,a.id LIMIT ")
        .push_bind((PAGE_SIZE + 1) as i64)
        .push(" OFFSET ")
        .push_bind(i64::from(offset));
    let rows = query
        .build()
        .map(|row: SqliteRow| Activity {
            id: row.get("id"),
            contact_id: row.get("contact_id"),
            contact_name: row.get("contact_name"),
            event_type: row.get("event_type"),
            occurred_at: row.get("occurred_at"),
            summary: row.get("summary"),
            details: row.get("details"),
            channel: row.get("channel"),
            followup_category: row.get("followup_category"),
            customer_feedback: row.get("customer_feedback"),
            counts_as_contact: row.get("counts_as_contact"),
            source: row.get("source"),
        })
        .fetch_all(pool)
        .await?;
    let (rows, has_more) = page(rows);
    Ok(Reply::Activities { rows, has_more })
}

pub async fn save(
    pool: &SqlitePool,
    id: Option<String>,
    mut input: ActivityInput,
    actor: Actor<'_>,
) -> Result<Reply> {
    validation::activity(&mut input)?;
    let (id, inserting) = id_or_new(id)?;
    let mut tx = pool.begin().await?;
    let archived: Option<i64> = sqlx::query_scalar("SELECT archived_at FROM contacts WHERE id=?")
        .bind(&input.contact_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::NotFound)?;
    if archived.is_some() {
        return Err(Error::ReadOnly);
    }

    if !inserting {
        let (owner, source): (String, String) =
            sqlx::query_as("SELECT contact_id,source FROM activities WHERE id=?")
                .bind(&id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Error::NotFound)?;
        if owner != input.contact_id || source == "system" {
            return Err(Error::ReadOnly);
        }
    }
    sqlx::query(
        "INSERT INTO activities(id,contact_id,event_type,occurred_at,summary,details,source,actor_id,agent_run_id,channel,followup_category,customer_feedback,counts_as_contact)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET event_type=excluded.event_type,occurred_at=excluded.occurred_at,
           summary=excluded.summary, details=excluded.details,channel=excluded.channel, followup_category=excluded.followup_category,
           customer_feedback=excluded.customer_feedback, counts_as_contact=COALESCE(excluded.counts_as_contact,activities.counts_as_contact), updated_at=unixepoch()",
    ).bind(&id).bind(input.contact_id).bind(input.event_type).bind(input.occurred_at)
        .bind(input.summary).bind(input.details).bind(actor.source.as_str()).bind(actor.id)
        .bind(actor.agent_run_id)
    .bind(input.channel)
    .bind(input.followup_category)
    .bind(input.customer_feedback).bind(input.counts_as_contact).execute(&mut *tx).await?;
    audit(
        &mut tx,
        actor,
        "activity",
        &id,
        if inserting { "create" } else { "update" },
    )
    .await?;
    tx.commit().await?;
    Ok(Reply::Saved { id })
}
