use super::{audit, contacts, validation, Actor, Error, Result};
use ftw_ipc::{
    crm::{ContactQuery, Result as Reply, Source},
    views::CellTarget,
};
use sha2::{Digest, Sha256};
use sqlx::{SqliteConnection, SqlitePool};
use std::collections::HashSet;

pub async fn update(
    pool: &SqlitePool,
    actor: Actor<'_>,
    targets: Vec<CellTarget>,
    field: &str,
    value: &str,
) -> Result<Reply> {
    if targets.is_empty() || targets.len() > 1000 {
        return Err(Error::Invalid("cell_targets"));
    }
    let state_field = matches!(field, "stage" | "level" | "sample");
    if state_field {
        let (column, value) = state_value(field, value.to_owned())?;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        let mut seen = HashSet::new();
        for target in &targets {
            if !seen.insert(&target.id) {
                return Err(Error::Invalid("cell_targets"));
            }
            contacts::check_revision(&mut tx, &target.id, target.revision, Some(&target.epoch))
                .await?;
            write_state(&mut tx, actor, &target.id, column, &value).await?;
        }
        tx.commit().await?;
        return Ok(Reply::CellsUpdated {
            count: targets.len(),
        });
    }
    if !matches!(actor.source, Source::Manual) {
        return Err(Error::ReadOnly);
    }
    if targets.len() != 1 || !matches!(field, "name" | "company" | "country") {
        return Err(Error::Invalid("cell_field"));
    }
    let mut edits = Vec::new();
    let mut seen = HashSet::new();
    for target in targets {
        if !seen.insert(target.id.clone()) {
            return Err(Error::Invalid("cell_targets"));
        }
        let Reply::Contact { record } = contacts::get(pool, &target.id).await? else {
            return Err(Error::NotFound);
        };
        let mut input = record.input;
        let destination = match field {
            "name" => &mut input.display_name,
            "company" => &mut input.company_name,
            "country" => &mut input.country_region,
            _ => return Err(Error::Invalid("cell_field")),
        };
        value.clone_into(destination);
        edits.push((target, input));
    }
    let mut tx = pool.begin().await?;
    let count = edits.len();
    for (target, input) in edits {
        contacts::check_revision(&mut tx, &target.id, target.revision, Some(&target.epoch)).await?;
        let response = contacts::save(&mut tx, Some(target.id), input, false, actor).await?;
        if let Reply::Duplicates { .. } = response {
            return Ok(response);
        }
    }
    tx.commit().await?;
    Ok(Reply::CellsUpdated { count })
}

type ScopeTarget = (String, i64, String);

fn fingerprint(targets: &[ScopeTarget]) -> Result<String> {
    let serialized = serde_json::to_vec(targets).map_err(|_| Error::Invalid("cell_targets"))?;
    Ok(format!("{:x}", Sha256::digest(serialized)))
}

pub async fn scope(pool: &SqlitePool, actor: Actor<'_>, filters: ContactQuery) -> Result<Reply> {
    let mut tx = pool.begin().await?;
    let mut query = contacts::matching_targets_query(&mut tx, actor, filters).await?;
    let targets: Vec<ScopeTarget> = query.build_query_as().fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(Reply::BulkContacts {
        count: targets.len(),
        fingerprint: fingerprint(&targets)?,
    })
}

pub async fn update_scope(
    pool: &SqlitePool,
    actor: Actor<'_>,
    filters: ContactQuery,
    expected: &str,
    field: &str,
    value: String,
) -> Result<Reply> {
    let (column, value) = state_value(field, value)?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let mut query = contacts::matching_targets_query(&mut tx, actor, filters).await?;
    let targets: Vec<ScopeTarget> = query.build_query_as().fetch_all(&mut *tx).await?;
    if targets.is_empty() || fingerprint(&targets)? != expected {
        return Err(Error::Invalid("bulk_scope_changed"));
    }
    for (id, _, _) in &targets {
        write_state(&mut tx, actor, id, column, &value).await?;
    }
    tx.commit().await?;
    Ok(Reply::CellsUpdated {
        count: targets.len(),
    })
}

fn state_value(field: &str, mut value: String) -> Result<(&'static str, String)> {
    validation::text(&mut value, "cell_value", 300, false)?;
    let column = match field {
        "stage" => {
            if value.is_empty() {
                value = "prospect".into();
            }
            validation::choice(&value, "stage", validation::STAGES)?;
            "stage"
        }
        "sample" => "sample_status",
        "level" => "customer_level",
        _ => return Err(Error::Invalid("cell_field")),
    };
    Ok((column, value))
}

async fn write_state(
    db: &mut SqliteConnection,
    actor: Actor<'_>,
    id: &str,
    column: &str,
    value: &str,
) -> Result<()> {
    let result = sqlx::query(&format!(
        "UPDATE contacts SET {column}=?,updated_by=?,updated_at=unixepoch(),record_revision=record_revision+1
         WHERE id=? AND archived_at IS NULL"
    )).bind(value).bind(actor.id).bind(id).execute(&mut *db).await?;
    if result.rows_affected() == 0 {
        return Err(Error::ReadOnly);
    }
    audit(db, actor, "contact", id, "update").await
}

#[cfg(test)]
mod tests {
    use super::*;
    use ftw_ipc::crm::ContactInput;

    #[tokio::test]
    async fn scoped_edit_rejects_changed_snapshot_and_updates_beyond_loaded_page() {
        let directory = tempfile::tempdir().unwrap();
        let pool = crate::storage::open(directory.path(), directory.path())
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        let mut ids = Vec::new();
        for index in 0..25 {
            let Reply::Saved { id } = contacts::save(
                &mut tx,
                None,
                ContactInput {
                    display_name: format!("Fixture {index}"),
                    country_region: if index == 24 {
                        "Unmatched"
                    } else {
                        "Fixture market"
                    }
                    .into(),
                    ..Default::default()
                },
                true,
                Actor::DESKTOP,
            )
            .await
            .unwrap() else {
                panic!("fixture not saved")
            };
            ids.push(id);
        }
        tx.commit().await.unwrap();
        let filters = ContactQuery {
            country: "Fixture market".into(),
            offset: 20,
            page_size: Some(20),
            ..Default::default()
        };
        let Reply::BulkContacts { count, fingerprint } =
            scope(&pool, Actor::DESKTOP, filters.clone()).await.unwrap()
        else {
            panic!("bulk scope unavailable")
        };
        assert_eq!(count, 24);
        contacts::archive(&pool, ids[5].clone(), true, Actor::DESKTOP)
            .await
            .unwrap();
        assert!(matches!(
            update_scope(
                &pool,
                Actor::DESKTOP,
                filters.clone(),
                &fingerprint,
                "stage",
                "won".into()
            )
            .await,
            Err(Error::Invalid("bulk_scope_changed"))
        ));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contacts WHERE stage='won'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        let Reply::BulkContacts { fingerprint, .. } =
            scope(&pool, Actor::DESKTOP, filters.clone()).await.unwrap()
        else {
            panic!("bulk scope unavailable")
        };
        assert!(matches!(
            update_scope(
                &pool,
                Actor::DESKTOP,
                filters,
                &fingerprint,
                "stage",
                "won".into()
            )
            .await
            .unwrap(),
            Reply::CellsUpdated { count: 23 }
        ));
        pool.close().await;
    }
}
