use super::{audit, new_id, page, validation, Actor, Error, Result};
use ftw_ipc::crm::{LeadEvidence, LeadEvidenceInput, Result as Reply, Source};
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection, SqlitePool};

#[derive(Clone, Copy)]
pub enum Owner<'a> {
    Lead(&'a str),
    Contact(&'a str),
}

impl<'a> Owner<'a> {
    fn parts(self) -> (&'a str, &'static str, &'static str, &'static str) {
        match self {
            Self::Lead(id) => (id, "lead", "lead_candidates", "lead_id"),
            Self::Contact(id) => (id, "contact", "contacts", "contact_id"),
        }
    }
}

pub(super) async fn record_legacy_links(
    db: &mut SqliteConnection,
    lead: &str,
    links: &[String],
    actor: Actor<'_>,
) -> Result<()> {
    for url in links {
        sqlx::query(
            "INSERT OR IGNORE INTO lead_evidence(id,lead_id,subject,url,source,actor_id,request_id,imported)
             VALUES (?,?,'来源网址',?,?,?,?,1)",
        )
        .bind(new_id()).bind(lead).bind(url).bind(actor.source.as_str()).bind(actor.id).bind(actor.agent_run_id)
        .execute(&mut *db).await?;
    }
    Ok(())
}

pub async fn list(pool: &SqlitePool, owner: Owner<'_>, offset: u32) -> Result<Reply> {
    let (id, _, table, _) = owner.parts();
    let mut tx = pool.begin().await?;
    let epoch: String = sqlx::query_scalar(&format!(
        "SELECT epoch FROM app_change_epoch WHERE singleton=1 AND EXISTS(SELECT 1 FROM {table} WHERE id=?)"
    )).bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
    let (filter, candidate) = match owner {
        Owner::Lead(_) => ("e.lead_id=?1", "NULL"),
        Owner::Contact(_) => (
            "(e.contact_id=?1 OR e.lead_id IN (SELECT l.id FROM lead_candidates l JOIN contacts c ON c.id=l.promoted_contact_id WHERE c.id=?1 OR c.merged_into=?1))",
            "(SELECT display_name FROM lead_candidates WHERE id=e.lead_id)",
        ),
    };
    let (total, verified, pending) = sqlx::query_as(&format!(
        "SELECT COUNT(*),COUNT(*) FILTER(WHERE status='verified'),COUNT(*) FILTER(WHERE status='pending') FROM lead_evidence e WHERE {filter}"
    )).bind(id).fetch_one(&mut *tx).await?;
    let rows = sqlx::query(&format!(
        "SELECT e.*, {candidate} candidate,
         CASE WHEN e.reviewed_by='desktop-user' THEN 'manual' WHEN e.reviewed_by IS NOT NULL THEN 'agent' END review_source
         FROM lead_evidence e WHERE {filter} ORDER BY created_at DESC,e.rowid DESC LIMIT 21 OFFSET ?2"
    )).bind(id).bind(i64::from(offset)).map(|row: SqliteRow| LeadEvidence {
        id: row.get("id"), subject: row.get("subject"), url: row.get("url"), note: row.get("note"),
        source: row.get("source"), request_id: row.get("request_id"), observed_at: row.get("observed_at"),
        created_at: row.get("created_at"), status: row.get("status"), reviewed_at: row.get("reviewed_at"),
        review_source: row.get("review_source"),
        revision: row.get("revision"),
        candidate: row.get("candidate"),
    }).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    let (rows, has_more) = page(rows);
    Ok(Reply::LeadEvidence {
        rows,
        total,
        verified,
        pending,
        has_more,
        epoch,
    })
}

pub async fn add(
    pool: &SqlitePool,
    actor: Actor<'_>,
    owner: Owner<'_>,
    mut input: LeadEvidenceInput,
) -> Result<Reply> {
    validation::text(&mut input.subject, "evidence_subject", 300, true)?;
    validation::text(&mut input.url, "evidence_url", 1000, false)?;
    validation::website(&input.url, "evidence_url")?;
    validation::text(&mut input.note, "evidence_note", 2000, input.url.is_empty())?;
    if let Some(timestamp) = input.observed_at {
        validation::timestamp(timestamp, "evidence_observed_at")?;
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let id = new_id();
    let (owner_id, entity, table, column) = owner.parts();
    if matches!(owner, Owner::Contact(_)) {
        let archived: Option<i64> =
            sqlx::query_scalar("SELECT archived_at FROM contacts WHERE id=?")
                .bind(owner_id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Error::NotFound)?;
        if archived.is_some() {
            return Err(Error::ReadOnly);
        }
    }
    let inserted = sqlx::query(&format!(
        "INSERT INTO lead_evidence(id,{column},subject,url,note,source,actor_id,request_id,observed_at)
         SELECT ?,id,?,?,?,?,?,?,? FROM {table} WHERE id=?"
    ))
    .bind(&id).bind(input.subject).bind(input.url).bind(input.note).bind(actor.source.as_str())
    .bind(actor.id).bind(actor.agent_run_id).bind(input.observed_at).bind(owner_id)
    .execute(&mut *tx).await?.rows_affected();
    if inserted == 0 {
        return Err(Error::NotFound);
    }
    audit(&mut tx, actor, entity, owner_id, "add_lead_evidence").await?;
    tx.commit().await?;
    Ok(Reply::Saved { id })
}

pub async fn review(
    pool: &SqlitePool,
    actor: Actor<'_>,
    id: &str,
    revision: i64,
    epoch: &str,
    status: &str,
) -> Result<Reply> {
    validation::choice(
        status,
        "evidence_status",
        &["pending", "verified", "rejected"],
    )?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let (owner, entity, current, state, current_epoch, archived, reviewer): (String, String, i64, String, String, bool, Option<String>) = sqlx::query_as(
        "SELECT COALESCE(e.lead_id,e.contact_id),CASE WHEN e.contact_id IS NULL THEN 'lead' ELSE 'contact' END,
         e.revision,e.status,(SELECT epoch FROM app_change_epoch WHERE singleton=1),c.archived_at IS NOT NULL,e.reviewed_by
         FROM lead_evidence e LEFT JOIN contacts c ON c.id=e.contact_id WHERE e.id=?",
    ).bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
    if current != revision || current_epoch != epoch {
        return Err(Error::Conflict(current));
    }
    if archived {
        return Err(Error::ReadOnly);
    }
    let human_reviewed = reviewer.as_deref() == Some(Actor::DESKTOP.id);
    if human_reviewed && matches!(actor.source, Source::Agent) {
        return Err(Error::ReadOnly);
    }
    if state != status
        || (status != "pending" && !human_reviewed && matches!(actor.source, Source::Manual))
    {
        sqlx::query(
            "UPDATE lead_evidence SET status=?1,revision=revision+1,reviewed_at=CASE WHEN ?1<>'pending' THEN unixepoch() END,
             reviewed_by=CASE WHEN ?1<>'pending' THEN ?2 END WHERE id=?3",
        ).bind(status).bind(actor.id).bind(id).execute(&mut *tx).await?;
        audit(
            &mut tx,
            actor,
            &entity,
            &owner,
            match status {
                "verified" => "verify_lead_evidence",
                "rejected" => "reject_lead_evidence",
                _ => "reopen_lead_evidence",
            },
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Reply::Saved { id: id.to_owned() })
}
