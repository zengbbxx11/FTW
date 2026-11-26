use super::{audit, id_or_new, page, validation, Actor, Error, Result, PAGE_SIZE};
use ftw_ipc::crm::{Activity, ActivityInput, Result as Reply, SampleActivity};
use sqlx::{sqlite::SqliteRow, QueryBuilder, Row, SqlitePool};

pub async fn list(
    pool: &SqlitePool,
    contact: Option<String>,
    event_type: &str,
    offset: u32,
    keyword: Option<String>,
    source: Option<String>,
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
    if let Some(source) = source {
        if !matches!(source.as_str(), "manual" | "agent" | "system") {
            return Err(Error::Invalid("activity_source"));
        }
        query.push(" AND a.source=").push_bind(source);
    }
    if let Some(keyword) = keyword
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if keyword.len() > 600 {
            return Err(Error::Invalid("keyword"));
        }
        query
            .push(" AND (instr(lower(c.display_name),lower(")
            .push_bind(keyword)
            .push("))>0 OR instr(lower(a.summary),lower(")
            .push_bind(keyword)
            .push("))>0 OR instr(lower(a.details),lower(")
            .push_bind(keyword)
            .push("))>0 OR instr(lower(a.customer_feedback),lower(")
            .push_bind(keyword)
            .push("))>0 OR instr(lower(a.sample_model),lower(")
            .push_bind(keyword)
            .push("))>0 OR instr(lower(a.sample_batch_ref),lower(")
            .push_bind(keyword)
            .push("))>0)");
    }
    query
        .push(" ORDER BY a.occurred_at DESC,a.id LIMIT ")
        .push_bind((PAGE_SIZE + 1) as i64)
        .push(" OFFSET ")
        .push_bind(i64::from(offset));
    let rows = query.build().map(activity).fetch_all(pool).await?;
    let (rows, has_more) = page(rows);
    Ok(Reply::Activities { rows, has_more })
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Reply> {
    let record = sqlx::query("SELECT a.*,c.display_name contact_name FROM activities a JOIN contacts c ON c.id=a.contact_id WHERE a.id=?")
        .bind(id).map(activity).fetch_optional(pool).await?.ok_or(Error::NotFound)?;
    Ok(Reply::Activities {
        rows: vec![record],
        has_more: false,
    })
}

fn activity(row: SqliteRow) -> Activity {
    Activity {
        id: row.get("id"),
        revision: row.get("record_revision"),
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
        sample: {
            let model: String = row.get("sample_model");
            let quantity: Option<u32> = row.get("sample_quantity");
            let batch_ref: String = row.get("sample_batch_ref");
            let progress: String = row.get("sample_progress");
            if model.is_empty() && quantity.is_none() && batch_ref.is_empty() && progress.is_empty()
            {
                None
            } else {
                Some(SampleActivity {
                    model,
                    quantity,
                    batch_ref: (!batch_ref.is_empty()).then_some(batch_ref),
                    progress: (!progress.is_empty()).then_some(progress),
                })
            }
        },
    }
}

pub async fn save(
    pool: &SqlitePool,
    id: Option<String>,
    input: ActivityInput,
    actor: Actor<'_>,
) -> Result<Reply> {
    save_guarded(pool, id, input, actor, None).await
}

pub(super) async fn save_guarded(
    pool: &SqlitePool,
    id: Option<String>,
    mut input: ActivityInput,
    actor: Actor<'_>,
    expected_revision: Option<i64>,
) -> Result<Reply> {
    validation::activity(&mut input)?;
    let (id, inserting) = id_or_new(id)?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let archived: Option<i64> = sqlx::query_scalar("SELECT archived_at FROM contacts WHERE id=?")
        .bind(&input.contact_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::NotFound)?;
    if archived.is_some() {
        return Err(Error::ReadOnly);
    }

    if !inserting {
        let (owner, source, revision): (String, String, i64) =
            sqlx::query_as("SELECT contact_id,source,record_revision FROM activities WHERE id=?")
                .bind(&id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Error::NotFound)?;
        if owner != input.contact_id || source == "system" {
            return Err(Error::ReadOnly);
        }
        if expected_revision.is_some_and(|expected| expected != revision) {
            return Err(Error::Conflict(revision));
        }
    }
    let has_sample = input.sample.is_some();
    let sample = input.sample.unwrap_or_default();
    sqlx::query(
        "INSERT INTO activities(id,contact_id,event_type,occurred_at,summary,details,source,actor_id,agent_run_id,channel,followup_category,customer_feedback,counts_as_contact,sample_model,sample_quantity,sample_batch_ref,sample_progress)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,COALESCE(?16,''),COALESCE(?17,''))
         ON CONFLICT(id) DO UPDATE SET event_type=excluded.event_type,occurred_at=excluded.occurred_at,
           summary=excluded.summary, details=excluded.details,channel=excluded.channel, followup_category=excluded.followup_category,
           customer_feedback=excluded.customer_feedback, counts_as_contact=COALESCE(excluded.counts_as_contact,activities.counts_as_contact),
           sample_model=CASE WHEN excluded.event_type<>'sample' OR ?18 THEN excluded.sample_model ELSE activities.sample_model END,
           sample_quantity=CASE WHEN excluded.event_type<>'sample' OR ?18 THEN excluded.sample_quantity ELSE activities.sample_quantity END,
           sample_batch_ref=CASE WHEN excluded.event_type<>'sample' THEN '' ELSE COALESCE(?16,activities.sample_batch_ref) END,
           sample_progress=CASE WHEN excluded.event_type<>'sample' THEN '' ELSE COALESCE(?17,activities.sample_progress) END,
           updated_at=unixepoch(),record_revision=activities.record_revision+1",
    ).bind(&id).bind(input.contact_id).bind(input.event_type).bind(input.occurred_at)
        .bind(input.summary).bind(input.details).bind(actor.source.as_str()).bind(actor.id)
        .bind(actor.agent_run_id)
    .bind(input.channel)
    .bind(input.followup_category)
    .bind(input.customer_feedback).bind(input.counts_as_contact)
    .bind(sample.model).bind(sample.quantity).bind(sample.batch_ref).bind(sample.progress).bind(has_sample).execute(&mut *tx).await?;
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
