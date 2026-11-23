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

pub async fn list(pool: &SqlitePool, actor: Actor<'_>, archived: bool) -> Result<Reply> {
    let desktop = matches!(actor.source, Source::Manual);
    let rows: Vec<ViewRow> = sqlx::query_as(
        "SELECT id,name,revision,hidden,is_default,archived,config_json
        FROM crm_views WHERE (? OR NOT archived) AND (? OR NOT hidden) ORDER BY position,id",
    )
    .bind(archived && desktop)
    .bind(desktop)
    .fetch_all(pool)
    .await?;
    let catalog = query::catalog(pool, actor).await?;
    let mut views = Vec::new();
    for row in rows {
        let config: Config = serde_json::from_str(&row.config_json)
            .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
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
    Ok(Reply::Views { views })
}

fn desktop(actor: Actor<'_>) -> Result<()> {
    if matches!(actor.source, Source::Manual) {
        Ok(())
    } else {
        Err(Error::ReadOnly)
    }
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
    desktop(actor)?;
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
    permitted(&query::catalog(pool, actor).await?, &input.config)?;
    let data = serde_json::to_string(&input.config).map_err(|_| Error::Invalid("view_config"))?;
    let mut tx = pool.begin().await?;
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
        if was_default && !input.is_default {
            return Err(Error::Invalid("view_choose_default"));
        }
    }
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
    desktop(actor)?;
    let mut tx = pool.begin().await?;
    let (current, default, _) = header(&mut tx, id, revision).await?;
    if default && archived {
        return Err(Error::Invalid("view_choose_default"));
    }
    if current != archived {
        if !archived {
            let name: String = sqlx::query_scalar("SELECT name FROM crm_views WHERE id=?")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
            name_available(&mut tx, id, &name).await?;
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

pub async fn move_view(
    pool: &SqlitePool,
    actor: Actor<'_>,
    id: &str,
    revision: i64,
    direction: i8,
) -> Result<Reply> {
    desktop(actor)?;
    let (comparison, order) = match direction {
        -1 => ("<", "DESC"),
        1 => (">", "ASC"),
        _ => return Err(Error::Invalid("view_direction")),
    };
    let mut tx = pool.begin().await?;
    let (archived, _, position) = header(&mut tx, id, revision).await?;
    if archived {
        return Err(Error::ReadOnly);
    }
    let sql = format!(
        "SELECT id,position FROM crm_views WHERE NOT archived AND position{comparison}?
        ORDER BY position {order},id LIMIT 1"
    );
    if let Some((other, neighbor)) = sqlx::query_as::<_, (String, i64)>(&sql)
        .bind(position)
        .fetch_optional(&mut *tx)
        .await?
    {
        sqlx::query("UPDATE crm_views SET position=CASE WHEN id=? THEN ? ELSE ? END,revision=revision+1 WHERE id IN (?,?)")
            .bind(id).bind(neighbor).bind(position).bind(id).bind(other).execute(&mut *tx).await?;
        audit(&mut tx, actor, "customer_view", id, "move").await?;
    }
    tx.commit().await?;
    Ok(Reply::Saved { id: id.into() })
}
