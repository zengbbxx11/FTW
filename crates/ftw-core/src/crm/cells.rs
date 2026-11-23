use super::{contacts, Actor, Error, Result};
use ftw_ipc::{
    crm::{Result as Reply, Source},
    views::CellTarget,
};
use sqlx::SqlitePool;
use std::collections::HashSet;

pub async fn update(
    pool: &SqlitePool,
    actor: Actor<'_>,
    targets: Vec<CellTarget>,
    field: &str,
    value: &str,
) -> Result<Reply> {
    if !matches!(actor.source, Source::Manual) {
        return Err(Error::ReadOnly);
    }
    if targets.is_empty() || targets.len() > 50 {
        return Err(Error::Invalid("cell_targets"));
    }
    let state_field = matches!(field, "stage" | "level" | "sample");
    if !(state_field || (targets.len() == 1 && matches!(field, "name" | "company" | "country"))) {
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
            "stage" => &mut input.stage,
            "level" => &mut input.customer_level,
            "sample" => &mut input.sample_status,
            _ => return Err(Error::Invalid("cell_field")),
        };
        value.clone_into(destination);
        edits.push((target, input));
    }
    let mut tx = pool.begin().await?;
    let count = edits.len();
    for (target, input) in edits {
        contacts::check_revision(&mut tx, &target.id, target.revision, Some(&target.epoch)).await?;
        let response = contacts::save(&mut tx, Some(target.id), input, state_field, actor).await?;
        if let Reply::Duplicates { .. } = response {
            return Ok(response);
        }
    }
    tx.commit().await?;
    Ok(Reply::CellsUpdated { count })
}
