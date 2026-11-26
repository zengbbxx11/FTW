use super::{validation, Error, Result};
use ftw_ipc::{
    agent::ContactDetailSuggestion as Detail,
    crm::{ContactInput, Interest, Method},
};
use sqlx::SqliteConnection;

pub fn normalize(detail: Detail) -> Result<(&'static str, String, Detail)> {
    match detail {
        Detail::ContactMethod { kind, value, label } => {
            let mut method = method(kind, value, label);
            let key = validation::method_value(&mut method)?;
            let Method {
                kind, value, label, ..
            } = method;
            Ok((
                "contact_method",
                key,
                Detail::ContactMethod { kind, value, label },
            ))
        }
        Detail::ProductInterest {
            product_text,
            intent_quantity,
            quantity_is_estimate,
            requirement,
        } => {
            let mut interest = Interest {
                id: None,
                product_text,
                intent_quantity,
                quantity_is_estimate,
                requirement,
            };
            validation::product_interest(&mut interest)?;
            let Interest {
                product_text,
                intent_quantity,
                quantity_is_estimate,
                requirement,
                ..
            } = interest;
            Ok((
                "product_interest",
                product_text.clone(),
                Detail::ProductInterest {
                    product_text,
                    intent_quantity,
                    quantity_is_estimate,
                    requirement,
                },
            ))
        }
        Detail::CorrectContactMethod {
            mut method_id,
            mut value,
            mut label,
        } => {
            validation::text(&mut method_id, "child_id", 100, true)?;
            validation::text(&mut value, "method_value", 300, true)?;
            if let Some(label) = &mut label {
                validation::text(label, "method_label", 200, false)?;
            }
            Ok((
                "contact_method",
                method_id.clone(),
                Detail::CorrectContactMethod {
                    method_id,
                    value,
                    label,
                },
            ))
        }
        Detail::CorrectProductInterest {
            mut interest_id,
            intent_quantity,
            quantity_is_estimate,
            mut requirement,
        } => {
            validation::text(&mut interest_id, "child_id", 100, true)?;
            if let Some(text) = &mut requirement {
                validation::text(text, "requirement", 2000, false)?;
            }
            if intent_quantity.is_none() && quantity_is_estimate.is_none() && requirement.is_none()
            {
                return Err(Error::Invalid("agent_detail"));
            }
            Ok((
                "product_interest",
                interest_id.clone(),
                Detail::CorrectProductInterest {
                    interest_id,
                    intent_quantity,
                    quantity_is_estimate,
                    requirement,
                },
            ))
        }
    }
}

pub async fn apply(
    db: &mut SqliteConnection,
    id: &str,
    input: &mut ContactInput,
    key: &str,
    detail: Detail,
) -> Result<bool> {
    match detail {
        Detail::ContactMethod { kind, value, label } => {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM contact_methods WHERE contact_id=? AND kind=? AND normalized=?)",
            )
            .bind(id)
            .bind(&kind)
            .bind(key)
            .fetch_one(&mut *db)
            .await?;
            if exists {
                return Ok(false);
            }
            check_shared_email(db, id, &kind, key).await?;
            input.methods.push(method(kind, value, label));
        }
        Detail::ProductInterest {
            product_text,
            intent_quantity,
            quantity_is_estimate,
            requirement,
        } => {
            let mut matching = input
                .interests
                .iter_mut()
                .filter(|i| i.product_text.eq_ignore_ascii_case(key));
            if let Some(existing) = matching.next() {
                if matching.next().is_some() {
                    return Err(Error::Invalid("agent_interest_ambiguous"));
                }
                if (intent_quantity.is_some()
                    && existing.intent_quantity.is_some()
                    && (existing.intent_quantity != intent_quantity
                        || existing.quantity_is_estimate != quantity_is_estimate))
                    || (!requirement.is_empty()
                        && !existing.requirement.is_empty()
                        && existing.requirement != requirement)
                {
                    return Err(Error::Invalid("agent_detail_overwrite"));
                }
                let mut changed = false;
                if existing.intent_quantity.is_none() && intent_quantity.is_some() {
                    existing.intent_quantity = intent_quantity;
                    existing.quantity_is_estimate = quantity_is_estimate;
                    changed = true;
                }
                if existing.requirement.is_empty() && !requirement.is_empty() {
                    existing.requirement = requirement;
                    changed = true;
                }
                return Ok(changed);
            }
            input.interests.push(Interest {
                id: None,
                product_text,
                intent_quantity,
                quantity_is_estimate,
                requirement,
            });
        }
        Detail::CorrectContactMethod {
            method_id,
            value,
            label,
        } => {
            let target = input
                .methods
                .iter_mut()
                .find(|m| m.id.as_deref() == Some(method_id.as_str()))
                .ok_or(Error::Invalid("child_id"))?;
            let mut updated = target.clone();
            updated.value = value;
            if let Some(label) = label {
                updated.label = label;
            }
            let normalized = validation::method_value(&mut updated)?;
            if updated.value == target.value && updated.label == target.label {
                return Ok(false);
            }
            if updated.kind == "email" {
                let previous = validation::email(&target.value);
                if previous.as_ref().ok() != Some(&normalized) {
                    check_shared_email(db, id, &updated.kind, &normalized).await?;
                    updated.is_verified = false;
                }
            } else if updated.value != target.value {
                updated.is_verified = false;
            }
            *target = updated;
        }
        Detail::CorrectProductInterest {
            interest_id,
            intent_quantity,
            quantity_is_estimate,
            requirement,
        } => {
            let target = input
                .interests
                .iter_mut()
                .find(|i| i.id.as_deref() == Some(interest_id.as_str()))
                .ok_or(Error::Invalid("child_id"))?;
            let mut updated = target.clone();
            if intent_quantity.is_some() {
                updated.intent_quantity = intent_quantity;
            }
            if let Some(estimate) = quantity_is_estimate {
                if updated.intent_quantity.is_none() {
                    return Err(Error::Invalid("intent_quantity"));
                }
                updated.quantity_is_estimate = estimate;
            }
            if let Some(requirement) = requirement {
                updated.requirement = requirement;
            }
            validation::product_interest(&mut updated)?;
            if updated.intent_quantity == target.intent_quantity
                && updated.requirement == target.requirement
                && updated.quantity_is_estimate == target.quantity_is_estimate
            {
                return Ok(false);
            }
            *target = updated;
        }
    }
    Ok(true)
}

async fn check_shared_email(
    db: &mut SqliteConnection,
    id: &str,
    kind: &str,
    normalized: &str,
) -> Result<()> {
    if kind == "email"
        && sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM contact_methods m JOIN contacts c ON c.id=m.contact_id
          WHERE m.kind='email' AND m.normalized=? AND c.id<>? AND c.archived_at IS NULL)",
        )
        .bind(normalized)
        .bind(id)
        .fetch_one(db)
        .await?
    {
        return Err(Error::Invalid("agent_email_already_assigned"));
    }
    Ok(())
}

fn method(kind: String, value: String, label: String) -> Method {
    Method {
        id: None,
        kind,
        value,
        label,
        is_primary: false,
        is_verified: false,
    }
}
