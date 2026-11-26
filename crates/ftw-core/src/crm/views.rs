use super::{audit, new_id, query, validation, Actor, Error, Result};
use ftw_ipc::{
    crm::{Result as Reply, Source},
    views::{Config, SavedView, ViewInput},
};
use sqlx::{FromRow, QueryBuilder, SqliteConnection, SqlitePool};
use std::collections::HashSet;

#[cfg(test)]
mod tests;

#[derive(FromRow)]
struct ViewRow {
    id: String,
    name: String,
    revision: i64,
    hidden: bool,
    is_default: bool,
    archived: bool,
    config_json: String,
}

fn permitted(catalog: &query::Catalog, config: &Config) -> Result<()> {
    let mut projected = QueryBuilder::new("");
    query::conditions(&mut projected, &config.query, catalog)?;
    query::sorting(&mut projected, &config.query, catalog)?;
    query::page_size(&config.query)?;
    if let Some(group) = &config.group_field {
        catalog.group_kind(group)?;
    }
    for field in &config.query.custom_columns {
        catalog.kind(field)?;
    }
    let mut columns = HashSet::new();
    if config.columns.len() > 100 {
        return Err(Error::Invalid("view_columns"));
    }
    for column in &config.columns {
        catalog.kind(&column.field)?;
        if !columns.insert(&column.field) || !(72..=640).contains(&column.width) {
            return Err(Error::Invalid("view_columns"));
        }
    }
    Ok(())
}

fn filter_references(filter: &ftw_ipc::views::Filter, field_id: &str) -> bool {
    use ftw_ipc::views::Filter;
    match filter {
        Filter::Rule { field, .. } => field == field_id,
        Filter::Group { children, .. } => children
            .iter()
            .any(|child| filter_references(child, field_id)),
    }
}

pub(super) async fn check_field_archive(db: &mut SqliteConnection, field_id: &str) -> Result<()> {
    let configs: Vec<String> =
        sqlx::query_scalar("SELECT config_json FROM crm_views WHERE NOT archived")
            .fetch_all(db)
            .await?;
    for config in configs {
        let config: Config =
            serde_json::from_str(&config).map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
        if config
            .query
            .filter
            .as_ref()
            .is_some_and(|filter| filter_references(filter, field_id))
            || config
                .query
                .order
                .iter()
                .any(|order| order.field == field_id)
            || config.group_field.as_deref() == Some(field_id)
            || config
                .query
                .group
                .as_ref()
                .is_some_and(|(field, _)| field == field_id)
        {
            return Err(Error::Invalid("field_in_use_by_view"));
        }
    }
    Ok(())
}

async fn archived_fields(db: &mut SqliteConnection) -> Result<HashSet<String>> {
    Ok(
        sqlx::query_scalar("SELECT id FROM crm_fields WHERE archived")
            .fetch_all(db)
            .await?
            .into_iter()
            .collect(),
    )
}

fn omit_archived_columns(config: &mut Config, archived: &HashSet<String>) {
    config
        .columns
        .retain(|column| !archived.contains(&column.field));
    config
        .query
        .custom_columns
        .retain(|field| !archived.contains(field));
}

fn preserve_archived_columns(config: &mut Config, previous: Config, archived: &HashSet<String>) {
    for (index, column) in previous.columns.into_iter().enumerate() {
        if archived.contains(&column.field) {
            config
                .columns
                .insert(index.min(config.columns.len()), column);
        }
    }
    let fields = &mut config.query.custom_columns;
    for (index, field) in previous.query.custom_columns.into_iter().enumerate() {
        if archived.contains(&field) {
            fields.insert(index.min(fields.len()), field);
        }
    }
}

pub async fn list(pool: &SqlitePool, actor: Actor<'_>, archived: bool) -> Result<Reply> {
    list_with_status(pool, actor, if archived { None } else { Some(false) }).await
}

pub async fn list_archived(pool: &SqlitePool, actor: Actor<'_>) -> Result<Reply> {
    list_with_status(pool, actor, Some(true)).await
}

async fn list_with_status(
    pool: &SqlitePool,
    actor: Actor<'_>,
    archived: Option<bool>,
) -> Result<Reply> {
    let desktop = matches!(actor.source, Source::Manual);
    let mut tx = pool.begin().await?;
    let rows: Vec<ViewRow> = sqlx::query_as(
        "SELECT id,name,revision,hidden,is_default,archived,config_json
        FROM crm_views WHERE (?1 IS NULL OR archived=?1) AND (?2 OR NOT hidden) ORDER BY position,id",
    )
    .bind(archived)
    .bind(desktop)
    .fetch_all(&mut *tx)
    .await?;
    let catalog = query::catalog_on(&mut tx, actor).await?;
    let archived = archived_fields(&mut tx).await?;
    let mut views = Vec::new();
    for row in rows {
        let mut config: Config = serde_json::from_str(&row.config_json)
            .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
        omit_archived_columns(&mut config, &archived);
        if !desktop && permitted(&catalog, &config).is_err() {
            continue;
        }
        views.push(SavedView {
            id: row.id,
            name: row.name,
            revision: row.revision,
            hidden: row.hidden,
            is_default: row.is_default,
            archived: row.archived,
            config,
        });
    }
    tx.commit().await?;
    Ok(Reply::Views { views })
}

async fn header(db: &mut SqliteConnection, id: &str, revision: i64) -> Result<(bool, bool, i64)> {
    let (current, archived, default, position): (i64, bool, bool, i64) =
        sqlx::query_as("SELECT revision,archived,is_default,position FROM crm_views WHERE id=?")
            .bind(id)
            .fetch_optional(db)
            .await?
            .ok_or(Error::NotFound)?;
    if current != revision {
        return Err(Error::Conflict(current));
    }
    Ok((archived, default, position))
}

async fn name_available(db: &mut SqliteConnection, id: &str, name: &str) -> Result<()> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM crm_views WHERE NOT archived AND name=? COLLATE NOCASE AND id<>?)")
        .bind(name).bind(id).fetch_one(db).await?;
    if exists {
        Err(Error::Invalid("view_duplicate"))
    } else {
        Ok(())
    }
}

pub async fn save(pool: &SqlitePool, actor: Actor<'_>, mut input: ViewInput) -> Result<Reply> {
    let agent = matches!(actor.source, Source::Agent);
    if agent && (input.hidden || input.is_default) {
        return Err(Error::ReadOnly);
    }
    validation::text(&mut input.name, "view_name", 100, true)?;
    if input.hidden && input.is_default {
        return Err(Error::Invalid("view_default_hidden"));
    }
    input.config.query.offset = 0;
    input.config.query.group = None;
    for column in &mut input.config.columns {
        if let Some(label) = &mut column.label {
            validation::text(label, "view_column_label", 96, true)?;
        }
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let catalog = query::catalog_on(&mut tx, actor).await?;
    let archived_column_ids = archived_fields(&mut tx).await?;
    omit_archived_columns(&mut input.config, &archived_column_ids);
    permitted(&catalog, &input.config)?;
    let inserting = input.id.is_none();
    let id = input.id.unwrap_or_else(new_id);
    name_available(&mut tx, &id, &input.name).await?;
    if inserting {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM crm_views")
            .fetch_one(&mut *tx)
            .await?;
        if count >= 64 {
            return Err(Error::Invalid("view_limit"));
        }
    } else {
        let (archived, was_default, _) = header(
            &mut tx,
            &id,
            input
                .expected_revision
                .ok_or(Error::Invalid("view_revision"))?,
        )
        .await?;
        if archived {
            return Err(Error::ReadOnly);
        }
        if agent {
            check_shared(&mut tx, &id, &catalog, false).await?;
        }
        if was_default && !input.is_default {
            return Err(Error::Invalid("view_choose_default"));
        }
        if !archived_column_ids.is_empty() {
            let previous: String =
                sqlx::query_scalar("SELECT config_json FROM crm_views WHERE id=?")
                    .bind(&id)
                    .fetch_one(&mut *tx)
                    .await?;
            let previous =
                serde_json::from_str(&previous).map_err(|_| Error::Invalid("view_config"))?;
            preserve_archived_columns(&mut input.config, previous, &archived_column_ids);
            if input.config.columns.len() > 100 {
                return Err(Error::Invalid("view_columns"));
            }
        }
    }
    let data = serde_json::to_string(&input.config).map_err(|_| Error::Invalid("view_config"))?;
    if input.is_default {
        sqlx::query(
            "UPDATE crm_views SET is_default=0,revision=revision+1 WHERE is_default=1 AND id<>?",
        )
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("INSERT INTO crm_views(id,name,hidden,is_default,position,config_json)
        VALUES (?,?,?,?,(SELECT COALESCE(MAX(position),0)+1 FROM crm_views),?)
        ON CONFLICT(id) DO UPDATE SET name=excluded.name,hidden=excluded.hidden,is_default=excluded.is_default,
        config_json=excluded.config_json,revision=crm_views.revision+1")
        .bind(&id).bind(&input.name).bind(input.hidden).bind(input.is_default).bind(data).execute(&mut *tx).await?;
    audit(
        &mut tx,
        actor,
        "customer_view",
        &id,
        if inserting { "create" } else { "update" },
    )
    .await?;
    tx.commit().await?;
    Ok(Reply::Saved { id })
}

pub async fn archive(
    pool: &SqlitePool,
    actor: Actor<'_>,
    id: &str,
    revision: i64,
    archived: bool,
) -> Result<Reply> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if matches!(actor.source, Source::Agent) {
        let catalog = query::catalog_on(&mut tx, actor).await?;
        check_shared(&mut tx, id, &catalog, !archived).await?;
    }
    let (current, default, _) = header(&mut tx, id, revision).await?;
    if default && archived {
        return Err(Error::Invalid("view_choose_default"));
    }
    if current != archived {
        if !archived {
            let (name, config): (String, String) =
                sqlx::query_as("SELECT name, config_json FROM crm_views WHERE id=?")
                    .bind(id)
                    .fetch_one(&mut *tx)
                    .await?;
            name_available(&mut tx, id, &name).await?;
            if matches!(actor.source, Source::Manual) {
                let mut config =
                    serde_json::from_str(&config).map_err(|_| Error::Invalid("view_config"))?;
                omit_archived_columns(&mut config, &archived_fields(&mut tx).await?);
                permitted(&query::catalog_on(&mut tx, actor).await?, &config)?;
            }
        }
        sqlx::query("UPDATE crm_views SET archived=?,revision=revision+1 WHERE id=?")
            .bind(archived)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        audit(
            &mut tx,
            actor,
            "customer_view",
            id,
            if archived { "archive" } else { "restore" },
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Reply::Saved { id: id.into() })
}

async fn check_shared(
    db: &mut SqliteConnection,
    id: &str,
    catalog: &query::Catalog,
    allow_archived: bool,
) -> Result<()> {
    let config: String = sqlx::query_scalar("SELECT config_json FROM crm_views WHERE id=? AND NOT hidden AND (? OR NOT archived) AND NOT is_default")
        .bind(id).bind(allow_archived).fetch_optional(&mut *db).await?.ok_or(Error::ReadOnly)?;
    let mut config = serde_json::from_str(&config).map_err(|_| Error::Invalid("view_config"))?;
    omit_archived_columns(&mut config, &archived_fields(db).await?);
    permitted(catalog, &config).map_err(|_| Error::ReadOnly)
}

pub async fn query_saved(
    pool: &SqlitePool,
    actor: Actor<'_>,
    id: &str,
    offset: u32,
) -> Result<Reply> {
    let Reply::Views { views } = list(pool, actor, false).await? else {
        unreachable!("list returns saved views")
    };
    let mut query = views
        .into_iter()
        .find(|view| view.id == id)
        .ok_or(Error::NotFound)?
        .config
        .query;
    query.offset = offset;
    super::contacts::list(pool, query, actor).await
}

pub async fn move_view(
    pool: &SqlitePool,
    actor: Actor<'_>,
    id: &str,
    revision: i64,
    direction: i8,
) -> Result<Reply> {
    let (comparison, order) = match direction {
        -1 => ("<", "DESC"),
        1 => (">", "ASC"),
        _ => return Err(Error::Invalid("view_direction")),
    };
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let desktop = matches!(actor.source, Source::Manual);
    let shared = if desktop {
        None
    } else {
        let catalog = query::catalog_on(&mut tx, actor).await?;
        check_shared(&mut tx, id, &catalog, false).await?;
        Some((catalog, archived_fields(&mut tx).await?))
    };
    let (archived, _, position) = header(&mut tx, id, revision).await?;
    if archived {
        return Err(Error::ReadOnly);
    }
    let sql = format!(
        "SELECT id,position,config_json FROM crm_views WHERE NOT archived AND position{comparison}?
        AND (? OR (NOT hidden AND NOT is_default)) ORDER BY position {order},id"
    );
    let candidates: Vec<(String, i64, String)> = sqlx::query_as(&sql)
        .bind(position)
        .bind(desktop)
        .fetch_all(&mut *tx)
        .await?;
    for (other, neighbor, config) in candidates {
        if let Some((catalog, archived)) = &shared {
            let Ok(mut config) = serde_json::from_str(&config) else {
                continue;
            };
            omit_archived_columns(&mut config, archived);
            if permitted(catalog, &config).is_err() {
                continue;
            }
        }
        sqlx::query("UPDATE crm_views SET position=CASE WHEN id=? THEN ? ELSE ? END,revision=revision+1 WHERE id IN (?,?)")
            .bind(id).bind(neighbor).bind(position).bind(id).bind(other).execute(&mut *tx).await?;
        audit(&mut tx, actor, "customer_view", id, "move").await?;
        break;
    }
    tx.commit().await?;
    Ok(Reply::Saved { id: id.into() })
}
