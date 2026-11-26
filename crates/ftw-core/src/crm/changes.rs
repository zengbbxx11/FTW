use super::{audit, contacts, detail_changes, new_id, page, validation, Actor, Error, Result};
use ftw_ipc::{
    agent::{ContactDetailSuggestion, ContactPatch, Evidence, EvidenceKind},
    crm::{AgentChange, ContactInput, Result as Reply, Source},
};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

pub async fn apply(
    pool: &SqlitePool,
    id: String,
    mut patch: ContactPatch,
    actor: Actor<'_>,
    expected_revision: Option<i64>,
    expected_epoch: Option<&str>,
) -> Result<Reply> {
    let request_id = actor.agent_run_id.ok_or(Error::Invalid("request_id"))?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let record = contacts::record_on(&mut tx, &id).await?;
    if record.archived_at.is_some() {
        return Err(Error::ReadOnly);
    }
    if expected_epoch.is_some_and(|epoch| epoch != record.data_epoch) {
        return Err(Error::Conflict(record.record_revision));
    }
    let conflict = expected_revision.is_some_and(|revision| revision != record.record_revision);
    let review_only = patch.review_only.unwrap_or(false)
        || patch
            .evidence
            .as_ref()
            .is_some_and(|e| !matches!(e.kind, EvidenceKind::Research));
    if let Some(evidence) = &mut patch.evidence {
        validate_evidence(evidence)?;
    }
    let mut input = record.input;
    let mut edited = Vec::new();
    for (field, value, suggested) in [
        ("stage", &mut input.stage, patch.stage),
        (
            "customer_level",
            &mut input.customer_level,
            patch.customer_level,
        ),
        (
            "primary_language",
            &mut input.primary_language,
            patch.primary_language,
        ),
        ("main_need", &mut input.main_need, patch.main_need),
        (
            "sample_status",
            &mut input.sample_status,
            patch.sample_status,
        ),
        ("notes", &mut input.notes, patch.notes),
    ] {
        if let Some(suggested) = suggested {
            edited.push((field, std::mem::replace(value, suggested)));
        }
    }
    validation::contact(&mut input)?;
    let review_only = review_only
        || (patch.evidence.is_some()
            && edited.iter().any(|(field, original)| {
                !original.is_empty()
                    && field_ref(&mut input, field).is_ok_and(|value| value != original)
            }));
    let state = if conflict || review_only {
        "pending"
    } else {
        "applied"
    };
    let mut changed = 0;
    for (field, original) in edited {
        let proposed = field_ref(&mut input, field)?;
        if *proposed == original {
            continue;
        }
        sqlx::query(
            "INSERT INTO crm_agent_changes(id,contact_id,request_id,field,previous_value,proposed_value,state,evidence_kind,evidence_url,evidence_note)
             VALUES (?,?,?,?,?,?,?,?,?,?)",
        ).bind(new_id()).bind(&id).bind(request_id).bind(field).bind(original).bind(proposed.as_str())
         .bind(state).bind(patch.evidence.as_ref().map(|e| e.kind.as_str()))
         .bind(patch.evidence.as_ref().and_then(|e| e.url.as_deref()))
         .bind(patch.evidence.as_ref().and_then(|e| e.note.as_deref())).execute(&mut *tx).await?;
        changed += 1;
    }
    if conflict {
        if changed > 0 {
            audit(&mut tx, actor, "contact", &id, "agent_field_conflict").await?;
        }
        tx.commit().await?;
        return Err(Error::Conflict(record.record_revision));
    }
    if review_only {
        if changed > 0 {
            audit(&mut tx, actor, "contact", &id, "propose_agent_fields").await?;
        }
        tx.commit().await?;
        return Ok(Reply::AgentReviewPending { id, count: changed });
    }
    let reply = contacts::save(&mut tx, Some(id), input, true, actor).await?;
    tx.commit().await?;
    Ok(reply)
}

pub async fn suggest_detail(
    pool: &SqlitePool,
    actor: Actor<'_>,
    target: ftw_ipc::views::CellTarget,
    detail: ContactDetailSuggestion,
    mut evidence: Evidence,
) -> Result<Reply> {
    let request_id = actor.agent_run_id.ok_or(Error::Invalid("request_id"))?;
    validate_evidence(&mut evidence)?;
    let (field, key, detail) = detail_changes::normalize(detail)?;
    let proposed = serde_json::to_string(&detail).map_err(|_| Error::Invalid("agent_detail"))?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    contacts::check_revision(&mut tx, &target.id, target.revision, Some(&target.epoch)).await?;
    let record = contacts::record_on(&mut tx, &target.id).await?;
    if record.archived_at.is_some() {
        return Err(Error::ReadOnly);
    }
    let mut input = record.input;
    if !detail_changes::apply(&mut tx, &target.id, &mut input, &key, detail).await? {
        return Ok(Reply::AgentReviewPending {
            id: target.id,
            count: 0,
        });
    }
    validation::contact(&mut input)?;
    let change_id = new_id();
    sqlx::query(
        "INSERT INTO crm_agent_changes(id,contact_id,request_id,field,previous_value,proposed_value,state,detail_key,evidence_kind,evidence_url,evidence_note)
         SELECT ?,?,?,?,'',?,'pending',?,?,?,?
         WHERE NOT EXISTS(SELECT 1 FROM crm_agent_changes WHERE contact_id=? AND field=? AND state='pending' AND proposed_value=?)",
    )
    .bind(&change_id).bind(&target.id).bind(request_id).bind(field).bind(&proposed).bind(key)
    .bind(evidence.kind.as_str()).bind(evidence.url).bind(evidence.note)
    .bind(&target.id).bind(field).bind(&proposed)
    .execute(&mut *tx).await?;
    let count = sqlx::query(
        "UPDATE crm_agent_changes SET previous_value=(SELECT current_value FROM crm_agent_change_versions WHERE id=?) WHERE id=?",
    )
    .bind(&change_id).bind(&change_id).execute(&mut *tx).await?.rows_affected() as usize;
    if count > 0 {
        audit(
            &mut tx,
            actor,
            "contact",
            &target.id,
            "propose_agent_fields",
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Reply::AgentReviewPending {
        id: target.id,
        count,
    })
}

fn validate_evidence(evidence: &mut Evidence) -> Result<()> {
    if let Some(url) = &mut evidence.url {
        validation::text(url, "evidence_url", 1000, true)?;
        validation::website(url, "evidence_url")?;
    }
    if let Some(note) = &mut evidence.note {
        validation::text(note, "evidence_note", 1000, true)?;
    }
    Ok(())
}

fn field_ref<'a>(input: &'a mut ContactInput, field: &str) -> Result<&'a mut String> {
    match field {
        "stage" => Ok(&mut input.stage),
        "customer_level" => Ok(&mut input.customer_level),
        "primary_language" => Ok(&mut input.primary_language),
        "main_need" => Ok(&mut input.main_need),
        "sample_status" => Ok(&mut input.sample_status),
        "notes" => Ok(&mut input.notes),
        _ => Err(Error::Invalid("agent_change_field")),
    }
}

pub async fn list(pool: &SqlitePool, offset: u32, scope: &str) -> Result<Reply> {
    if !matches!(scope, "" | "pending" | "applied" | "conflict") {
        return Err(Error::Invalid("agent_change_scope"));
    }
    const SOURCE: &str = "WITH changes AS (
           SELECT *,state='pending' AND current_value<>previous_value AND current_value<>proposed_value conflict
           FROM crm_agent_change_versions
         )";
    let mut tx = pool.begin().await?;
    let (total, pending, applied, conflicts) = sqlx::query_as(&format!(
        "{SOURCE} SELECT COUNT(*),COUNT(*) FILTER(WHERE state='pending'),
         COUNT(*) FILTER(WHERE state='applied'),COUNT(*) FILTER(WHERE conflict) FROM changes"
    ))
    .fetch_one(&mut *tx)
    .await?;
    let rows = sqlx::query(&format!(
        "{SOURCE} SELECT * FROM changes WHERE (?1='' OR state=?1 OR (?1='conflict' AND conflict))
         ORDER BY created_at DESC,change_order DESC LIMIT 21 OFFSET ?2"
    ))
    .bind(scope)
    .bind(i64::from(offset))
    .map(|row: SqliteRow| AgentChange {
        id: row.get("id"),
        contact_id: row.get("contact_id"),
        contact_name: row.get("display_name"),
        field: row.get("field"),
        previous_value: row.get("previous_value"),
        proposed_value: row.get("proposed_value"),
        current_value: row.get("current_value"),
        request_id: row.get("request_id"),
        state: row.get("state"),
        created_at: row.get("created_at"),
        conflict: row.get("conflict"),
        archived: row.get("archived"),
        evidence_kind: row.get("evidence_kind"),
        evidence_url: row.get("evidence_url"),
        evidence_note: row.get("evidence_note"),
    })
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let (rows, has_more) = page(rows);
    Ok(Reply::AgentChanges {
        rows,
        has_more,
        total,
        pending,
        applied,
        conflicts,
    })
}

pub async fn resolve(pool: &SqlitePool, actor: Actor<'_>, id: &str, accept: bool) -> Result<Reply> {
    if !matches!(actor.source, Source::Manual) {
        return Err(Error::ReadOnly);
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let row = sqlx::query(
        "SELECT contact_id,field,previous_value,proposed_value,current_value,state,detail_key FROM crm_agent_change_versions WHERE id=?",
    ).bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
    let customer: String = row.get("contact_id");
    let field: &str = row.get("field");
    let previous: &str = row.get("previous_value");
    let proposed: &str = row.get("proposed_value");
    let state: &str = row.get("state");
    if state != "pending" {
        return Err(Error::ReadOnly);
    }
    if accept {
        let current = contacts::record_on(&mut tx, &customer).await?;
        if current.archived_at.is_some() {
            return Err(Error::ReadOnly);
        }
        let mut input = current.input;
        let live: &str = row.get("current_value");
        if live != previous && live != proposed {
            return Err(Error::Invalid("agent_change_superseded"));
        }
        let changed = if let Some(key) = row.get::<Option<&str>, _>("detail_key") {
            let detail =
                serde_json::from_str(proposed).map_err(|_| Error::Invalid("agent_detail"))?;
            detail_changes::apply(&mut tx, &customer, &mut input, key, detail).await?
        } else {
            let value = field_ref(&mut input, field)?;
            if *value != proposed {
                proposed.clone_into(value);
                true
            } else {
                false
            }
        };
        if changed {
            contacts::save(&mut tx, Some(customer.clone()), input, true, actor).await?;
        }
    }
    let resolution = if accept { "accepted" } else { "kept" };
    sqlx::query("UPDATE crm_agent_changes SET state=? WHERE id=?")
        .bind(resolution)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    audit(
        &mut tx,
        actor,
        "contact",
        &customer,
        if accept {
            "accept_agent_field"
        } else {
            "keep_customer_field"
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Reply::Saved { id: customer })
}
