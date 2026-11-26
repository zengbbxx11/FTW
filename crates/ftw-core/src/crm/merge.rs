use super::{audit, contacts, validation, Actor, Error, Result};
use ftw_ipc::{crm::Result as Reply, views::CellTarget};
use sqlx::SqlitePool;

pub async fn merge(
    pool: &SqlitePool,
    actor: Actor<'_>,
    primary: CellTarget,
    secondary: CellTarget,
) -> Result<Reply> {
    if primary.id == secondary.id || primary.epoch != secondary.epoch {
        return Err(Error::Invalid("merge_pair"));
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    for target in [&primary, &secondary] {
        contacts::check_revision(&mut tx, &target.id, target.revision, Some(&target.epoch)).await?;
    }
    let keep = contacts::record_on(&mut tx, &primary.id).await?;
    let old = contacts::record_on(&mut tx, &secondary.id).await?;
    if keep.archived_at.is_some() || old.archived_at.is_some() {
        return Err(Error::ReadOnly);
    }

    let in_flight: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM email_drafts WHERE contact_id=?
           AND state IN ('draft','pending_approval','approved','queued','sending'))",
    )
    .bind(&secondary.id)
    .fetch_one(&mut *tx)
    .await?;
    if in_flight {
        return Err(Error::Invalid("merge_open_mail"));
    }

    let mut input = keep.input;
    let old = old.input;
    for (value, fallback) in [
        (&mut input.company_name, old.company_name),
        (&mut input.country_region, old.country_region),
        (&mut input.customer_type, old.customer_type),
        (&mut input.customer_source, old.customer_source),
        (&mut input.customer_level, old.customer_level),
        (&mut input.first_contact_channel, old.first_contact_channel),
        (&mut input.primary_language, old.primary_language),
        (&mut input.main_need, old.main_need),
        (&mut input.sample_status, old.sample_status),
        (&mut input.bulk_order_status, old.bulk_order_status),
        (&mut input.website_url, old.website_url),
        (&mut input.alibaba_buyer_id, old.alibaba_buyer_id),
        (&mut input.notes, old.notes),
    ] {
        if value.is_empty() {
            *value = fallback;
        }
    }
    input.first_contact_at = input
        .first_contact_at
        .into_iter()
        .chain(old.first_contact_at)
        .min();
    input.last_contact_at = input
        .last_contact_at
        .into_iter()
        .chain(old.last_contact_at)
        .max();

    if let (Some(first), Some(last)) = (input.first_contact_at, input.last_contact_at) {
        input.first_contact_at = Some(first.min(last));
        input.last_contact_at = Some(first.max(last));
    }
    for mut method in old.methods {
        let normalize = |value: &str| {
            if method.kind == "email" {
                validation::email(value)
            } else {
                Ok(value.to_lowercase())
            }
        };
        let normalized = normalize(&method.value)?;
        if let Some(same) = input.methods.iter_mut().find(|item| {
            item.kind == method.kind
                && normalize(&item.value).is_ok_and(|current| current == normalized)
        }) {
            same.is_verified |= method.is_verified;
            continue;
        }
        method.id = None;
        method.is_primary &= !input
            .methods
            .iter()
            .any(|item| item.kind == method.kind && item.is_primary);
        input.methods.push(method);
    }
    for mut interest in old.interests {
        if input.interests.iter().any(|item| {
            item.product_text == interest.product_text
                && item.intent_quantity == interest.intent_quantity
                && item.quantity_is_estimate == interest.quantity_is_estimate
                && item.requirement == interest.requirement
        }) {
            continue;
        }
        interest.id = None;
        input.interests.push(interest);
    }
    let reply = contacts::save(&mut tx, Some(primary.id.clone()), input, true, actor).await?;

    let copied: Vec<String> = sqlx::query_scalar(
        "INSERT INTO crm_field_values(contact_id,field_id,kind,text_value,number_value,bool_value,option_id)
         SELECT ?,field_id,kind,text_value,number_value,bool_value,option_id FROM crm_field_values
         WHERE contact_id=? ON CONFLICT(contact_id,field_id) DO NOTHING RETURNING field_id",
    ).bind(&primary.id).bind(&secondary.id).fetch_all(&mut *tx).await?;
    // Copy multi-select items only for attributes that were missing on the retained customer.
    // The incoming source and all of its original property values remain on the archived record.
    for field in &copied {
        sqlx::query("INSERT INTO crm_field_multi_values(contact_id,field_id,option_id) SELECT ?,field_id,option_id FROM crm_field_multi_values WHERE contact_id=? AND field_id=?")
            .bind(&primary.id).bind(&secondary.id).bind(field).execute(&mut *tx).await?;
    }
    super::fields::record_merged_values(&mut tx, actor, &primary.id, primary.revision + 1, &copied)
        .await?;
    for query in [
        "INSERT INTO crm_company_memberships(contact_id,company_id,branch)
         SELECT ?,company_id,branch FROM crm_company_memberships WHERE contact_id=? ON CONFLICT(contact_id) DO NOTHING",
        "UPDATE activities SET contact_id=?,record_revision=record_revision+1 WHERE contact_id=?",
        "UPDATE lead_evidence SET contact_id=? WHERE contact_id=?",
        "UPDATE inbox_messages SET contact_id=? WHERE contact_id=?",
        "UPDATE email_messages SET contact_id=? WHERE contact_id=?",
        "UPDATE contacts SET merged_into=?,record_revision=record_revision+1 WHERE merged_into=?",
    ] {
        sqlx::query(query)
            .bind(&primary.id)
            .bind(&secondary.id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(
        "UPDATE contacts SET merged_into=?,archived_at=unixepoch(),updated_at=unixepoch(),
        updated_by=?,record_revision=record_revision+1 WHERE id=?",
    )
    .bind(&primary.id)
    .bind(actor.id)
    .bind(&secondary.id)
    .execute(&mut *tx)
    .await?;
    audit(
        &mut tx,
        actor,
        "contact",
        &primary.id,
        &format!("merge_from:{}", secondary.id),
    )
    .await?;
    audit(
        &mut tx,
        actor,
        "contact",
        &secondary.id,
        &format!("merge_into:{}", primary.id),
    )
    .await?;
    tx.commit().await?;
    Ok(reply)
}
