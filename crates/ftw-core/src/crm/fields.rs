use super::{audit, contacts, new_id, validation, Actor, Error, Result};
use ftw_ipc::{
    crm::{Result as Reply, Source},
    fields::{Access, Choice, Definition, Entry, Field, Kind, Value, ValuePatch, Values},
};
use serde::de::DeserializeOwned;
use sqlx::{FromRow, SqliteConnection, SqlitePool};
use std::collections::{HashMap, HashSet};

const MAX_FIELDS: usize = 80;
const MAX_OPTIONS: usize = 36;

#[derive(FromRow)]
struct FieldRow {
    id: String,
    revision: i64,
    name: String,
    kind: String,
    color: String,
    agent_access: String,
    show_in_details: bool,
    default_json: Option<String>,
    archived: bool,
}

#[derive(FromRow)]
struct OptionRow {
    field_id: String,
    id: String,
    label: String,
    color: String,
    archived: bool,
}

fn decode<T: DeserializeOwned>(value: serde_json::Value) -> Result<T> {
    serde_json::from_value(value).map_err(|error| sqlx::Error::Decode(Box::new(error)).into())
}

fn default_value(encoded: Option<String>) -> Result<Option<Value>> {
    encoded
        .map(|json| {
            serde_json::from_str(&json).map_err(|error| sqlx::Error::Decode(Box::new(error)).into())
        })
        .transpose()
}

async fn definitions(
    db: &mut SqliteConnection,
    actor: Actor<'_>,
    include_archived: bool,
) -> Result<Vec<Field>> {
    let show_private = matches!(actor.source, Source::Manual);
    let rows: Vec<FieldRow> = sqlx::query_as(
        "SELECT * FROM crm_fields
        WHERE (? OR NOT archived) AND (? OR agent_access<>'hidden') ORDER BY position,id",
    )
    .bind(include_archived)
    .bind(show_private)
    .fetch_all(&mut *db)
    .await?;
    let choices: Vec<OptionRow> = sqlx::query_as(
        "SELECT o.field_id,o.id,o.label,o.color,o.archived
        FROM crm_field_options o JOIN crm_fields f ON f.id=o.field_id
        WHERE (? OR NOT f.archived) AND (? OR f.agent_access<>'hidden')
        ORDER BY o.position,o.id",
    )
    .bind(include_archived)
    .bind(show_private)
    .fetch_all(db)
    .await?;
    let mut options: HashMap<String, Vec<Choice>> = HashMap::new();
    for row in choices {
        options.entry(row.field_id).or_default().push(Choice {
            id: row.id,
            label: row.label,
            color: row.color,
            archived: row.archived,
        });
    }
    rows.into_iter()
        .map(|row| {
            Ok(Field {
                options: options.remove(&row.id).unwrap_or_default(),
                id: row.id,
                revision: row.revision,
                name: row.name,
                color: row.color,
                kind: decode(serde_json::Value::String(row.kind))?,
                agent_access: decode(serde_json::Value::String(row.agent_access))?,
                show_in_details: row.show_in_details,
                default_value: default_value(row.default_json)?,
                archived: row.archived,
            })
        })
        .collect()
}

pub async fn list(pool: &SqlitePool, actor: Actor<'_>, include_archived: bool) -> Result<Reply> {
    let mut tx = pool.begin().await?;
    let fields = definitions(
        &mut tx,
        actor,
        include_archived && matches!(actor.source, Source::Manual),
    )
    .await?;
    tx.commit().await?;
    Ok(Reply::Fields { fields })
}

pub(crate) async fn list_archived_for_management(
    pool: &SqlitePool,
    actor: Actor<'_>,
) -> Result<Reply> {
    let mut tx = pool.begin().await?;
    let mut fields = definitions(&mut tx, actor, true).await?;
    fields.retain(|field| field.archived && field.agent_access == Access::Write);
    tx.commit().await?;
    Ok(Reply::Fields { fields })
}

fn color(value: &str) -> Result<()> {
    validation::choice(
        value,
        "field_color",
        &["gray", "teal", "blue", "green", "amber", "purple", "rose"],
    )
}

async fn field_header(
    db: &mut SqliteConnection,
    id: &str,
    expected: i64,
) -> Result<(String, bool, i64)> {
    let (revision, kind, archived, position): (i64, String, bool, i64) =
        sqlx::query_as("SELECT revision,kind,archived,position FROM crm_fields WHERE id=?")
            .bind(id)
            .fetch_optional(db)
            .await?
            .ok_or(Error::NotFound)?;
    if expected != revision {
        return Err(Error::Conflict(revision));
    }
    Ok((kind, archived, position))
}

async fn name_available(db: &mut SqliteConnection, id: &str, name: &str) -> Result<()> {
    let duplicate: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM crm_fields WHERE NOT archived AND name=? COLLATE NOCASE AND id<>?)",
    ).bind(name).bind(id).fetch_one(db).await?;
    if duplicate {
        Err(Error::Invalid("field_name_duplicate"))
    } else {
        Ok(())
    }
}

pub async fn save(pool: &SqlitePool, actor: Actor<'_>, mut input: Definition) -> Result<Reply> {
    let agent = matches!(actor.source, Source::Agent);
    let creating = input.id.is_none();
    let required_access = if creating {
        Access::Hidden
    } else {
        Access::Write
    };
    if agent && input.agent_access != required_access {
        return Err(Error::ReadOnly);
    }
    validation::text(&mut input.name, "field_name", 96, true)?;
    color(&input.color)?;
    if input.options.len() > MAX_OPTIONS
        || (!input.options.is_empty()
            && !matches!(input.kind, Kind::SingleSelect | Kind::MultiSelect))
    {
        return Err(Error::Invalid("field_options"));
    }
    let mut seen = HashSet::new();
    let mut labels = HashSet::new();
    let mut active = HashSet::new();
    for option in &mut input.options {
        if uuid::Uuid::parse_str(&option.id).is_err() || !seen.insert(option.id.as_str()) {
            return Err(Error::Invalid("field_option_id"));
        }
        validation::text(&mut option.label, "field_option_label", 80, true)?;
        color(&option.color)?;
        if !option.archived {
            if !labels.insert(option.label.to_ascii_lowercase()) {
                return Err(Error::Invalid("field_option_label"));
            }
            active.insert(option.id.as_str());
        }
    }
    if let Some(value) = &mut input.default_value {
        normalize(input.kind, value, &active)?;
    }
    let encoded = input
        .default_value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|_| Error::Invalid("field_value"))?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let id = input.id.unwrap_or_else(new_id);
    if agent && !creating {
        let editable: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM crm_fields WHERE id=? AND agent_access='write' AND NOT archived)")
            .bind(&id).fetch_one(&mut *tx).await?;
        if !editable {
            return Err(Error::ReadOnly);
        }
    }
    name_available(&mut tx, &id, &input.name).await?;
    if creating {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM crm_fields")
            .fetch_one(&mut *tx)
            .await?;
        if count >= MAX_FIELDS as i64 {
            return Err(Error::Invalid("field_limit"));
        }
        sqlx::query("INSERT INTO crm_fields(id,name,kind,color,agent_access,show_in_details,default_json,position)
            VALUES (?,?,?,?,?,?,?,(SELECT COALESCE(MAX(position),0)+1 FROM crm_fields))")
            .bind(&id).bind(&input.name).bind(input.kind.code()).bind(&input.color)
            .bind(input.agent_access.code()).bind(input.show_in_details).bind(encoded)
            .execute(&mut *tx).await?;
    } else {
        let revision = input
            .expected_revision
            .ok_or(Error::Invalid("field_revision"))?;
        let (kind, archived, _) = field_header(&mut tx, &id, revision).await?;
        if archived || kind != input.kind.code() {
            return Err(Error::ReadOnly);
        }
        let old: Vec<String> =
            sqlx::query_scalar("SELECT id FROM crm_field_options WHERE field_id=?")
                .bind(&id)
                .fetch_all(&mut *tx)
                .await?;
        if old.iter().any(|id| !seen.contains(id.as_str())) {
            return Err(Error::Invalid("field_option_archive_required"));
        }
        // Release active label constraints before updating the complete option list.
        sqlx::query("UPDATE crm_field_options SET archived=1 WHERE field_id=?")
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE crm_fields SET name=?,color=?,agent_access=?,show_in_details=?,default_json=?,revision=revision+1 WHERE id=?")
            .bind(&input.name).bind(&input.color).bind(input.agent_access.code()).bind(input.show_in_details)
            .bind(encoded).bind(&id).execute(&mut *tx).await?;
    }
    for (position, option) in input.options.into_iter().enumerate() {
        sqlx::query("INSERT INTO crm_field_options(field_id,id,label,color,position,archived) VALUES (?,?,?,?,?,?)
            ON CONFLICT(field_id,id) DO UPDATE SET label=excluded.label,color=excluded.color,
            position=excluded.position,archived=excluded.archived")
            .bind(&id).bind(option.id).bind(option.label).bind(option.color)
            .bind(position as i64).bind(option.archived).execute(&mut *tx).await?;
    }
    audit(
        &mut tx,
        actor,
        "custom_field",
        &id,
        if creating { "create" } else { "update" },
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
        let writable: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM crm_fields WHERE id=? AND agent_access='write')",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if !writable {
            return Err(Error::ReadOnly);
        }
    }
    let (_, was_archived, _) = field_header(&mut tx, id, revision).await?;
    if archived != was_archived {
        if archived {
            super::views::check_field_archive(&mut tx, id).await?;
        } else {
            let name: String = sqlx::query_scalar("SELECT name FROM crm_fields WHERE id=?")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
            name_available(&mut tx, id, &name).await?;
        }
        sqlx::query("UPDATE crm_fields SET archived=?,revision=revision+1 WHERE id=?")
            .bind(archived)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        audit(
            &mut tx,
            actor,
            "custom_field",
            id,
            if archived { "archive" } else { "restore" },
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Reply::Saved { id: id.into() })
}

pub async fn move_field(
    pool: &SqlitePool,
    actor: Actor<'_>,
    id: &str,
    revision: i64,
    direction: i8,
) -> Result<Reply> {
    let (comparison, order) = match direction {
        -1 => ("<", "DESC"),
        1 => (">", "ASC"),
        _ => return Err(Error::Invalid("field_direction")),
    };
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let desktop = matches!(actor.source, Source::Manual);
    if !desktop {
        let writable: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM crm_fields WHERE id=? AND NOT archived AND agent_access='write')")
            .bind(id).fetch_one(&mut *tx).await?;
        if !writable {
            return Err(Error::ReadOnly);
        }
    }
    let (_, archived, position) = field_header(&mut tx, id, revision).await?;
    if archived {
        return Err(Error::ReadOnly);
    }
    let sql = format!(
        "SELECT id,position FROM crm_fields WHERE NOT archived AND (? OR agent_access='write') AND position{comparison}?
        ORDER BY position {order},id LIMIT 1"
    );
    if let Some((other, neighbor)) = sqlx::query_as::<_, (String, i64)>(&sql)
        .bind(desktop)
        .bind(position)
        .fetch_optional(&mut *tx)
        .await?
    {
        sqlx::query("UPDATE crm_fields SET position=CASE WHEN id=? THEN ? ELSE ? END,revision=revision+1 WHERE id IN (?,?)")
            .bind(id).bind(neighbor).bind(position).bind(id).bind(other).execute(&mut *tx).await?;
        audit(&mut tx, actor, "custom_field", id, "move").await?;
    }
    tx.commit().await?;
    Ok(Reply::Saved { id: id.into() })
}

#[derive(FromRow)]
struct ValueRow {
    field_id: String,
    kind: String,
    text_value: Option<String>,
    number_value: Option<f64>,
    bool_value: Option<bool>,
    option_id: Option<String>,
}

pub async fn values(pool: &SqlitePool, actor: Actor<'_>, id: &str) -> Result<Reply> {
    let mut tx = pool.begin().await?;
    let (revision, epoch) = sqlx::query_as(
        "SELECT record_revision,(SELECT epoch FROM app_change_epoch) FROM contacts WHERE id=?",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Error::NotFound)?;
    let entries = value_entries(&mut tx, actor, id, false).await?;
    tx.commit().await?;
    Ok(Reply::FieldValues {
        record: Values {
            revision,
            epoch,
            entries,
        },
    })
}

async fn value_entries(
    db: &mut SqliteConnection,
    actor: Actor<'_>,
    id: &str,
    include_archived: bool,
) -> Result<Vec<Entry>> {
    let show_private = matches!(actor.source, Source::Manual);
    let values: Vec<ValueRow> = sqlx::query_as("SELECT v.* FROM crm_field_values v JOIN crm_fields f ON f.id=v.field_id
        WHERE v.contact_id=? AND (? OR NOT f.archived) AND (? OR f.agent_access<>'hidden') ORDER BY f.position")
        .bind(id).bind(include_archived).bind(show_private).fetch_all(&mut *db).await?;
    let choices: Vec<(String,String)> = sqlx::query_as("SELECT m.field_id,m.option_id FROM crm_field_multi_values m
        JOIN crm_fields f ON f.id=m.field_id JOIN crm_field_options o ON o.field_id=m.field_id AND o.id=m.option_id
        WHERE m.contact_id=? AND (? OR NOT f.archived) AND (? OR f.agent_access<>'hidden') ORDER BY o.position")
        .bind(id).bind(include_archived).bind(show_private).fetch_all(&mut *db).await?;
    let mut multi: HashMap<String, Vec<String>> = HashMap::new();
    for (field, option) in choices {
        multi.entry(field).or_default().push(option);
    }
    let mut entries = Vec::with_capacity(values.len());
    for row in values {
        let value = match row.kind.as_str() {
            "text" => row.text_value.map(Value::Text),
            "date" => row.text_value.map(Value::Date),
            "url" => row.text_value.map(Value::Url),
            "number" => row.number_value.map(Value::Number),
            "checkbox" => row.bool_value.map(Value::Checkbox),
            "single_select" => row.option_id.map(Value::SingleSelect),
            "multi_select" => multi.remove(&row.field_id).map(Value::MultiSelect),
            _ => return Err(Error::Invalid("field_kind")),
        };
        entries.push(Entry {
            field_id: row.field_id,
            value,
        });
    }
    Ok(entries)
}

pub async fn patch(pool: &SqlitePool, actor: Actor<'_>, patch: ValuePatch) -> Result<Reply> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let id = patch.contact_id;
    contacts::check_revision(
        &mut tx,
        &id,
        patch.expected_revision,
        Some(&patch.expected_epoch),
    )
    .await?;
    let archived: bool =
        sqlx::query_scalar("SELECT archived_at IS NOT NULL FROM contacts WHERE id=?")
            .bind(&id)
            .fetch_one(&mut *tx)
            .await?;
    if archived {
        return Err(Error::ReadOnly);
    }
    if !patch.entries.is_empty() {
        apply(
            &mut tx,
            actor,
            &id,
            patch.expected_revision + 1,
            patch.entries,
        )
        .await?;
        sqlx::query("UPDATE contacts SET record_revision=record_revision+1,updated_at=unixepoch(),updated_by=? WHERE id=?")
            .bind(actor.id).bind(&id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Reply::Saved { id })
}

pub async fn apply(
    db: &mut SqliteConnection,
    actor: Actor<'_>,
    contact: &str,
    revision: i64,
    entries: Vec<Entry>,
) -> Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    if entries.len() > MAX_FIELDS {
        return Err(Error::Invalid("field_limit"));
    }
    let fields: HashMap<_, _> = definitions(db, actor, false)
        .await?
        .into_iter()
        .map(|field| (field.id.clone(), field))
        .collect();
    let mut previous: HashMap<_, _> = value_entries(db, actor, contact, false)
        .await?
        .into_iter()
        .map(|entry| (entry.field_id, entry.value))
        .collect();
    let mut seen = HashSet::new();
    for mut entry in entries {
        if !seen.insert(entry.field_id.clone()) {
            return Err(Error::Invalid("field_duplicate"));
        }
        let field = fields.get(&entry.field_id).ok_or(Error::NotFound)?;
        if matches!(actor.source, Source::Agent) && field.agent_access != Access::Write {
            return Err(Error::ReadOnly);
        }
        if let Some(value) = &mut entry.value {
            let options = field
                .options
                .iter()
                .filter(|option| !option.archived)
                .map(|option| option.id.as_str())
                .collect();
            normalize(field.kind, value, &options)?;
        }
        let mut before = previous.remove(&field.id).flatten();
        if let Some(Value::MultiSelect(options)) = &mut before {
            options.sort_unstable();
        }
        let before = serde_json::to_string(&before).map_err(|_| Error::Invalid("field_value"))?;
        let after =
            serde_json::to_string(&entry.value).map_err(|_| Error::Invalid("field_value"))?;
        if before != after {
            store(db, contact, &field.id, field.kind.code(), entry.value).await?;
            record_change(db, actor, (contact, revision), &field.id, (&before, &after)).await?;
        }
    }
    audit(db, actor, "contact", contact, "patch_custom_fields").await?;
    Ok(())
}

pub async fn apply_defaults(
    db: &mut SqliteConnection,
    contact: &str,
    actor: Actor<'_>,
) -> Result<()> {
    let defaults: Vec<(String, String, String)> = sqlx::query_as("SELECT id,kind,default_json FROM crm_fields WHERE NOT archived AND default_json IS NOT NULL").fetch_all(&mut *db).await?;
    for (id, kind, encoded) in defaults {
        record_change(db, actor, (contact, 1), &id, ("null", &encoded)).await?;
        store(db, contact, &id, &kind, default_value(Some(encoded))?).await?;
    }
    Ok(())
}

pub(super) async fn record_merged_values(
    db: &mut SqliteConnection,
    actor: Actor<'_>,
    contact: &str,
    revision: i64,
    fields: &[String],
) -> Result<()> {
    if fields.is_empty() {
        return Ok(());
    }
    for entry in value_entries(db, Actor::DESKTOP, contact, true).await? {
        if fields.contains(&entry.field_id) {
            let value =
                serde_json::to_string(&entry.value).map_err(|_| Error::Invalid("field_value"))?;
            record_change(
                db,
                actor,
                (contact, revision),
                &entry.field_id,
                ("null", &value),
            )
            .await?;
        }
    }
    Ok(())
}

async fn record_change(
    db: &mut SqliteConnection,
    actor: Actor<'_>,
    (contact, revision): (&str, i64),
    field_id: &str,
    (before, after): (&str, &str),
) -> Result<()> {
    sqlx::query(
        "INSERT INTO contact_custom_history
        (contact_id,revision,field_id,field_name,actor_id,source,task_id,before_json,after_json)
        SELECT ?,?,id,name,?,?,?,?,? FROM crm_fields WHERE id=?
        ON CONFLICT(contact_id,revision,field_id) DO UPDATE SET after_json=excluded.after_json",
    )
    .bind(contact)
    .bind(revision)
    .bind(actor.id)
    .bind(actor.source.as_str())
    .bind(actor.agent_run_id)
    .bind(before)
    .bind(after)
    .bind(field_id)
    .execute(db)
    .await?;
    Ok(())
}

async fn store(
    db: &mut SqliteConnection,
    contact: &str,
    field_id: &str,
    kind: &str,
    value: Option<Value>,
) -> Result<()> {
    sqlx::query("DELETE FROM crm_field_values WHERE contact_id=? AND field_id=?")
        .bind(contact)
        .bind(field_id)
        .execute(&mut *db)
        .await?;
    let Some(value) = value else {
        return Ok(());
    };
    let (text, number, check, option, multi) = match value {
        Value::Text(value) | Value::Url(value) | Value::Date(value) => {
            (Some(value), None, None, None, vec![])
        }
        Value::Number(value) => (None, Some(value), None, None, vec![]),
        Value::Checkbox(value) => (None, None, Some(value), None, vec![]),
        Value::SingleSelect(value) => (None, None, None, Some(value), vec![]),
        Value::MultiSelect(values) => (None, None, None, None, values),
    };
    sqlx::query("INSERT INTO crm_field_values(contact_id,field_id,kind,text_value,number_value,bool_value,option_id)
        VALUES(?,?,?,?,?,?,?)")
        .bind(contact).bind(field_id).bind(kind).bind(text).bind(number).bind(check).bind(option)
        .execute(&mut *db).await?;
    for option in multi {
        sqlx::query(
            "INSERT INTO crm_field_multi_values(contact_id,field_id,option_id) VALUES(?,?,?)",
        )
        .bind(contact)
        .bind(field_id)
        .bind(option)
        .execute(&mut *db)
        .await?;
    }
    Ok(())
}

fn normalize(kind: Kind, value: &mut Value, options: &HashSet<&str>) -> Result<()> {
    match (kind, value) {
        (Kind::Text, Value::Text(text)) => validation::text(text, "field_value", 2000, true),
        (Kind::Url, Value::Url(text)) => {
            validation::text(text, "field_url", 1000, true)?;
            validation::website(text, "field_url")?;
            Ok(())
        }
        (Kind::Number, Value::Number(number))
            if number.is_finite() && number.abs() <= 9_000_000_000_000_000.0 =>
        {
            Ok(())
        }
        (Kind::Date, Value::Date(date)) if valid_date(date) => Ok(()),
        (Kind::Checkbox, Value::Checkbox(_)) => Ok(()),
        (Kind::SingleSelect, Value::SingleSelect(option)) if options.contains(option.as_str()) => {
            Ok(())
        }
        (Kind::MultiSelect, Value::MultiSelect(values))
            if !values.is_empty()
                && values.len() <= MAX_OPTIONS
                && values.iter().all(|id| options.contains(id.as_str())) =>
        {
            values.sort_unstable();
            values.dedup();
            Ok(())
        }
        _ => Err(Error::Invalid("field_value")),
    }
}

pub(super) fn valid_date(date: &str) -> bool {
    let bytes = date.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return false;
    }
    let year: u32 = date[..4].parse().unwrap_or_default();
    let month: u32 = date[5..7].parse().unwrap_or_default();
    let day: u32 = date[8..].parse().unwrap_or_default();
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(400) || (year.is_multiple_of(4) && !year.is_multiple_of(100)) => {
            29
        }
        2 => 28,
        _ => 0,
    };
    year != 0 && (1..=days).contains(&day)
}

#[cfg(test)]
mod tests;

pub async fn project(
    pool: &SqlitePool,
    actor: Actor<'_>,
    contacts: &[String],
    field_ids: &[String],
) -> Result<HashMap<String, Vec<Entry>>> {
    if contacts.is_empty() || field_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
        "SELECT v.contact_id,v.field_id,v.kind,substr(v.text_value,1,120) text_value,v.number_value,
        v.bool_value,v.option_id,(SELECT json_group_array(m.option_id) FROM crm_field_multi_values m
        WHERE m.contact_id=v.contact_id AND m.field_id=v.field_id) multi
        FROM crm_field_values v JOIN crm_fields f ON f.id=v.field_id WHERE NOT f.archived",
    );
    if matches!(actor.source, Source::Agent) {
        query.push(" AND f.agent_access<>'hidden'");
    }
    query.push(" AND v.contact_id IN (");
    let mut ids = query.separated(",");
    for id in contacts {
        ids.push_bind(id);
    }
    ids.push_unseparated(")");
    query.push(" AND v.field_id IN (");
    let mut fields = query.separated(",");
    for field in field_ids {
        fields.push_bind(field);
    }
    fields.push_unseparated(")");
    let rows = query.build().fetch_all(pool).await?;
    let mut values: HashMap<String, Vec<Entry>> = HashMap::new();
    use sqlx::Row;
    for row in rows {
        let value = match row.get::<&str, _>("kind") {
            "text" => Value::Text(row.get("text_value")),
            "date" => Value::Date(row.get("text_value")),
            "url" => Value::Url(row.get("text_value")),
            "number" => Value::Number(row.get("number_value")),
            "checkbox" => Value::Checkbox(row.get("bool_value")),
            "single_select" => Value::SingleSelect(row.get("option_id")),
            "multi_select" => Value::MultiSelect(
                serde_json::from_str(row.get("multi"))
                    .map_err(|error| sqlx::Error::Decode(Box::new(error)))?,
            ),
            _ => return Err(Error::Invalid("field_kind")),
        };
        values
            .entry(row.get("contact_id"))
            .or_default()
            .push(Entry {
                field_id: row.get("field_id"),
                value: Some(value),
            });
    }
    Ok(values)
}
