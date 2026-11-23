use super::{audit, contacts, new_id, page, validation, Actor, Error, Result};
use ftw_ipc::{
    agent::ContactPatch,
    crm::{AgentChange, ContactInput, Result as Reply, Source},
};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

pub async fn apply(
    pool: &SqlitePool,
    id: String,
    patch: ContactPatch,
    request_id: &str,
    expected_revision: Option<i64>,
    expected_epoch: Option<&str>,
) -> Result<Reply> {
    let actor = Actor {
        source: Source::Agent,
        id: "mcp",
        agent_run_id: Some(request_id),
    };
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let record = contacts::record_on(&mut tx, &id).await?;
    if record.archived_at.is_some() {
        return Err(Error::ReadOnly);
    }
    if expected_epoch.is_some_and(|epoch| epoch != record.data_epoch) {
        return Err(Error::Conflict(record.record_revision));
    }
    let conflict = expected_revision.is_some_and(|revision| revision != record.record_revision);
    let review_only = patch.review_only.unwrap_or(false);
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
            "INSERT INTO crm_agent_changes(id,contact_id,request_id,field,previous_value,proposed_value,state)
             VALUES (?,?,?,?,?,?,?)",
        ).bind(new_id()).bind(&id).bind(request_id).bind(field).bind(original).bind(proposed.as_str())
         .bind(state).execute(&mut *tx).await?;
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

pub async fn list(pool: &SqlitePool, offset: u32, pending: bool) -> Result<Reply> {
    let rows = sqlx::query(
        "SELECT a.*,c.display_name,CASE field
           WHEN 'stage' THEN c.stage WHEN 'customer_level' THEN c.customer_level
           WHEN 'primary_language' THEN c.primary_language WHEN 'main_need' THEN c.main_need
           WHEN 'sample_status' THEN c.sample_status ELSE c.notes END current_value
         FROM crm_agent_changes a JOIN contacts c ON c.id=a.contact_id
         WHERE NOT ? OR a.state='pending'
         ORDER BY a.created_at DESC,a.rowid DESC LIMIT 21 OFFSET ?",
    )
    .bind(pending)
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
    })
    .fetch_all(pool)
    .await?;
    let (rows, has_more) = page(rows);
    Ok(Reply::AgentChanges { rows, has_more })
}

pub async fn resolve(pool: &SqlitePool, actor: Actor<'_>, id: &str, accept: bool) -> Result<Reply> {
    if !matches!(actor.source, Source::Manual) {
        return Err(Error::ReadOnly);
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let (customer, field, previous, proposed, state): (String, String, String, String, String) = sqlx::query_as(
        "SELECT contact_id,field,previous_value,proposed_value,state FROM crm_agent_changes WHERE id=?",
    ).bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
    if state != "pending" {
        return Err(Error::ReadOnly);
    }
    if accept {
        let current = contacts::record_on(&mut tx, &customer).await?;
        if current.archived_at.is_some() {
            return Err(Error::ReadOnly);
        }
        let mut input = current.input;
        let value = field_ref(&mut input, &field)?;
        if *value != previous && *value != proposed {
            return Err(Error::Invalid("agent_change_superseded"));
        }
        if *value != proposed {
            *value = proposed;
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
