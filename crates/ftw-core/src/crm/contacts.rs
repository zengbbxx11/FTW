use super::{audit, fields, id_or_new, new_id, query, validation, Actor, Error, Result};
use ftw_ipc::crm::{
    ArchiveFilter, Contact, ContactInput, ContactQuery, ContactRow, ContactSort, Interest,
    MailOutreach, Method, Result as Reply,
};
use sqlx::{sqlite::SqliteRow, QueryBuilder, Row, SqliteConnection, SqlitePool};
use std::collections::HashMap;

fn priority_filter() -> String {
    format!("{}='A'", state_sql("customer_level"))
}

fn sample_filter() -> String {
    format!("({} IN ('planned','in_progress','in_transit','evaluating') OR (c.sample_status='' AND c.stage='sample_in_progress'))", state_sql("sample_status"))
}

pub async fn stats(pool: &SqlitePool) -> Result<Reply> {
    let row = sqlx::query(&format!(
        "SELECT COUNT(*) active,COUNT(*) FILTER(WHERE {}) priority, COUNT(*) FILTER(WHERE {}) sample,
        COUNT(*) FILTER(WHERE stage IN ('contacted','replied','requirements','quote_sample','sample_in_progress','negotiation')) in_progress,
        COUNT(*) FILTER(WHERE stage='won') won FROM contacts c WHERE archived_at IS NULL",
        priority_filter(), sample_filter()
    )).fetch_one(pool).await?;
    Ok(Reply::CustomerStats {
        active: row.get("active"),
        priority: row.get("priority"),
        sample: row.get("sample"),
        in_progress: row.get("in_progress"),
        won: row.get("won"),
    })
}

pub async fn list(pool: &SqlitePool, mut filters: ContactQuery, actor: Actor<'_>) -> Result<Reply> {
    validation::text(&mut filters.keyword, "keyword", 300, false)?;
    let mut query = QueryBuilder::new(
        "SELECT c.id, c.display_name, c.company_name, c.country_region, c.stage, c.customer_level, c.sample_status,
         c.archived_at, c.source, dates.effective_last, c.record_revision,
         (SELECT epoch FROM app_change_epoch) data_epoch,
         COALESCE((SELECT product_text FROM customer_interests i WHERE i.contact_id=c.id ORDER BY position LIMIT 1),'') primary_interest,
         (SELECT value FROM contact_methods WHERE contact_id = c.id AND kind = 'email'
            ORDER BY is_primary DESC, position LIMIT 1) AS email,
         outreach.*
         FROM contacts c JOIN crm_contact_dates dates ON dates.id=c.id
         JOIN mail_contact_outreach outreach ON outreach.contact_id=c.id WHERE 1 = 1",
    );
    match filters.archive {
        ArchiveFilter::Active => {
            query.push(" AND c.archived_at IS NULL");
        }
        ArchiveFilter::Archived => {
            query.push(" AND c.archived_at IS NOT NULL");
        }
        ArchiveFilter::All => {}
    }
    match filters.quick_view.as_str() {
        "" => {}
        "priority" => {
            query.push(" AND ").push(priority_filter());
        }
        "sample" => {
            query.push(" AND ").push(sample_filter());
        }
        _ => return Err(Error::Invalid("quick_view")),
    }
    let page_size = query::page_size(&filters)?;
    let extended = filters.filter.is_some()
        || !filters.order.is_empty()
        || !filters.custom_columns.is_empty()
        || filters.group.is_some();
    let catalog = if extended {
        Some(query::catalog(pool, actor).await?)
    } else {
        None
    };
    if let Some(catalog) = &catalog {
        query::conditions(&mut query, &filters, catalog)?;
    }
    for (column, value) in [
        ("country_region", filters.country.as_str()),
        ("stage", filters.stage.as_str()),
    ] {
        if !value.trim().is_empty() {
            query
                .push(" AND c.")
                .push(column)
                .push(" = ")
                .push_bind(value.trim().to_owned())
                .push(" COLLATE NOCASE");
        }
    }
    for (field, value) in [
        ("customer_level", filters.level.as_str()),
        ("customer_type", filters.customer_type.as_str()),
        ("customer_source", filters.source.as_str()),
        ("sample_status", filters.sample_status.as_str()),
    ] {
        if !value.trim().is_empty() {
            let value = canonical_state(pool, field, value).await?;
            query
                .push(" AND ")
                .push(state_sql(field))
                .push(" = ")
                .push_bind(value)
                .push(" COLLATE NOCASE");
        }
    }
    if !filters.keyword.is_empty() {
        if filters.keyword.chars().count() >= 3 {
            let phrase = format!("\"{}\"", filters.keyword.replace('"', "\"\""));
            query
                .push(
                    " AND c.rowid IN (SELECT rowid FROM contact_search WHERE contact_search MATCH ",
                )
                .push_bind(phrase)
                .push(")");
        } else {
            query.push(" AND c.rowid IN (SELECT rowid FROM contact_search WHERE instr(lower(searchable), lower(")
                .push_bind(filters.keyword.as_str()).push(")) > 0)");
        }
    }
    let order = match filters.sort {
        ContactSort::Updated => "c.updated_at DESC, c.rowid DESC, c.id",
        ContactSort::Name => "c.display_name COLLATE NOCASE, c.id",
        ContactSort::LastContact => "effective_last DESC, c.updated_at DESC, c.id",
    };
    query.push(" ORDER BY ");
    if let Some(catalog) = catalog.as_ref().filter(|_| !filters.order.is_empty()) {
        query::sorting(&mut query, &filters, catalog)?;
        query.push(",c.id");
    } else {
        query.push(order);
    }
    query
        .push(" LIMIT ")
        .push_bind(page_size + 1)
        .push(" OFFSET ")
        .push_bind(i64::from(filters.offset));
    let mut rows = query
        .build()
        .map(|row: SqliteRow| ContactRow {
            mail_outreach: outreach(&row),
            id: row.get("id"),
            record_revision: row.get("record_revision"),
            data_epoch: row.get("data_epoch"),
            primary_interest: row.get("primary_interest"),
            custom_values: Vec::new(),
            display_name: row.get("display_name"),
            company_name: row.get("company_name"),
            country_region: row.get("country_region"),
            stage: row.get("stage"),
            customer_level: row.get("customer_level"),
            sample_status: row.get("sample_status"),
            email: row.get("email"),
            last_contact_at: row.get("effective_last"),
            archived_at: row.get("archived_at"),
            source: row.get("source"),
        })
        .fetch_all(pool)
        .await?;
    let has_more = rows.len() > page_size as usize;
    rows.truncate(page_size as usize);
    if !filters.custom_columns.is_empty() {
        let ids: Vec<_> = rows.iter().map(|row| row.id.clone()).collect();
        let mut values = fields::project(pool, actor, &ids, &filters.custom_columns).await?;
        for row in &mut rows {
            row.custom_values = values.remove(&row.id).unwrap_or_default();
        }
    }
    Ok(Reply::Contacts { rows, has_more })
}

fn outreach(row: &SqliteRow) -> MailOutreach {
    MailOutreach {
        status: row.get("mail_status"),
        last_sent_at: row.get("last_sent_at"),
        last_reply_at: row.get("last_reply_at"),
        outreach_count: row.get("outreach_count"),
        recent_subject: row.get("recent_subject"),
        thread_id: row.get("thread_id"),
        sync_outcome: row.get("sync_outcome"),
        sync_at: row.get("sync_at"),
    }
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Reply> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        "SELECT c.*,dates.observed_first,dates.observed_last,dates.effective_first,dates.effective_last,
                (SELECT epoch FROM app_change_epoch WHERE singleton=1) data_epoch, outreach.*
         FROM contacts c JOIN crm_contact_dates dates ON dates.id=c.id
         JOIN mail_contact_outreach outreach ON outreach.contact_id=c.id WHERE c.id=?",
    ).bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;

    let methods = sqlx::query("SELECT * FROM contact_methods WHERE contact_id=? ORDER BY position")
        .bind(id)
        .map(|row: SqliteRow| Method {
            id: Some(row.get("id")),
            kind: row.get("kind"),
            value: row.get("value"),
            label: row.get("label"),
            is_primary: row.get("is_primary"),
            is_verified: row.get("is_verified"),
        })
        .fetch_all(&mut *tx)
        .await?;
    let interests =
        sqlx::query("SELECT * FROM customer_interests WHERE contact_id=? ORDER BY position")
            .bind(id)
            .map(|row: SqliteRow| Interest {
                id: Some(row.get("id")),
                product_text: row.get("product_text"),
                intent_quantity: row.get("intent_quantity"),
                quantity_is_estimate: row.get("quantity_is_estimate"),
                requirement: row.get("requirement"),
            })
            .fetch_all(&mut *tx)
            .await?;
    tx.commit().await?;
    Ok(Reply::Contact {
        record: Box::new(Contact {
            id: row.get("id"),
            archived_at: row.get("archived_at"),
            source: row.get("source"),
            record_revision: row.get("record_revision"),
            data_epoch: row.get("data_epoch"),
            effective_first_contact_at: row.get("effective_first"),
            effective_last_contact_at: row.get("effective_last"),
            updated_at: row.get("updated_at"),
            latest_thread_reply_at: row.get("last_reply_at"),
            mail_outreach: outreach(&row),
            observed_first_contact_at: row.get("observed_first"),
            observed_last_contact_at: row.get("observed_last"),
            input: ContactInput {
                display_name: row.get("display_name"),
                company_name: row.get("company_name"),
                country_region: row.get("country_region"),
                customer_type: row.get("customer_type"),
                customer_source: row.get("customer_source"),
                customer_level: row.get("customer_level"),
                stage: row.get("stage"),
                first_contact_channel: row.get("first_contact_channel"),
                primary_language: row.get("primary_language"),
                main_need: row.get("main_need"),
                sample_status: row.get("sample_status"),
                bulk_order_status: row.get("bulk_order_status"),
                first_contact_at: row.get("first_contact_at"),
                last_contact_at: row.get("last_contact_at"),
                website_url: row.get("website_url"),
                alibaba_buyer_id: row.get("alibaba_buyer_id"),
                notes: row.get("notes"),
                methods,
                interests,
            },
        }),
    })
}

pub async fn save(
    connection: &mut SqliteConnection,
    id: Option<String>,
    mut input: ContactInput,
    allow_duplicate: bool,
    actor: Actor<'_>,
) -> Result<Reply> {
    let (domain, normalized) = validation::contact(&mut input)?;
    let (id, inserting) = id_or_new(id)?;
    if !inserting {
        let archived: Option<i64> =
            sqlx::query_scalar("SELECT archived_at FROM contacts WHERE id=?")
                .bind(&id)
                .fetch_optional(&mut *connection)
                .await?
                .ok_or(Error::NotFound)?;
        if archived.is_some() {
            return Err(Error::ReadOnly);
        }
    }
    if !allow_duplicate {
        let emails: Vec<_> = input
            .methods
            .iter()
            .zip(&normalized)
            .filter(|(method, _)| method.kind == "email")
            .map(|(_, value)| value.as_str())
            .collect();
        let matches = super::duplicates::find(
            connection,
            super::duplicates::Criteria {
                emails: &emails,
                domain: &domain,
                company: &input.company_name,
                country: &input.country_region,
                contact_id: &id,
                lead_id: None,
            },
        )
        .await?;
        if !matches.is_empty() {
            return Ok(Reply::Duplicates { matches });
        }
    }

    let rowid: i64 = sqlx::query_scalar(
        "INSERT INTO contacts(id, display_name, company_name, country_region, customer_type, customer_source,
          customer_level, stage, first_contact_channel, primary_language, main_need, sample_status,
          bulk_order_status, first_contact_at, last_contact_at, website_url, website_domain, alibaba_buyer_id,
          notes, source, created_by, updated_by)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET display_name=excluded.display_name, company_name=excluded.company_name,
          country_region=excluded.country_region, customer_type=excluded.customer_type,
          customer_source=excluded.customer_source, customer_level=excluded.customer_level,stage=excluded.stage,
          first_contact_channel=excluded.first_contact_channel, primary_language=excluded.primary_language,
          main_need=excluded.main_need, sample_status=excluded.sample_status, bulk_order_status=excluded.bulk_order_status,
          first_contact_at=excluded.first_contact_at,last_contact_at=excluded.last_contact_at,
          website_url=excluded.website_url,website_domain=excluded.website_domain,
          alibaba_buyer_id=excluded.alibaba_buyer_id,notes=excluded.notes,updated_by=excluded.updated_by,updated_at=unixepoch(),
          record_revision=contacts.record_revision+1
         RETURNING rowid",
    )
    .bind(&id).bind(&input.display_name).bind(&input.company_name).bind(&input.country_region)
    .bind(&input.customer_type).bind(&input.customer_source).bind(&input.customer_level).bind(&input.stage)
    .bind(&input.first_contact_channel).bind(&input.primary_language).bind(&input.main_need).bind(&input.sample_status)
    .bind(&input.bulk_order_status).bind(input.first_contact_at).bind(input.last_contact_at)
    .bind(&input.website_url).bind(&domain).bind(&input.alibaba_buyer_id).bind(&input.notes)
    .bind(actor.source.as_str()).bind(actor.id).bind(actor.id)
    .fetch_one(&mut *connection).await?;

    save_children(connection, &id, &input, &normalized).await?;
    if inserting {
        super::fields::apply_defaults(connection, &id).await?;
    }
    let searchable = [
        input.display_name,
        input.company_name,
        input.country_region,
        input.website_url,
        input.alibaba_buyer_id,
        input.main_need,
        input.notes,
    ]
    .into_iter()
    .chain(input.methods.into_iter().map(|method| method.value))
    .chain(
        input
            .interests
            .into_iter()
            .map(|interest| interest.product_text),
    )
    .chain(normalized)
    .collect::<Vec<_>>()
    .join("\n");
    sqlx::query("DELETE FROM contact_search WHERE rowid=?")
        .bind(rowid)
        .execute(&mut *connection)
        .await?;
    sqlx::query("INSERT INTO contact_search(rowid, searchable) VALUES (?, ?)")
        .bind(rowid)
        .bind(searchable)
        .execute(&mut *connection)
        .await?;
    audit(
        connection,
        actor,
        "contact",
        &id,
        if inserting { "create" } else { "update" },
    )
    .await?;
    Ok(Reply::Saved { id })
}

async fn save_children(
    connection: &mut SqliteConnection,
    contact: &str,
    input: &ContactInput,
    normalized: &[String],
) -> Result<()> {
    let mut methods: HashMap<String, i64> =
        sqlx::query_as("SELECT id, created_at FROM contact_methods WHERE contact_id=?")
            .bind(contact)
            .fetch_all(&mut *connection)
            .await?
            .into_iter()
            .collect();
    let mut interests: HashMap<String, i64> =
        sqlx::query_as("SELECT id, created_at FROM customer_interests WHERE contact_id=?")
            .bind(contact)
            .fetch_all(&mut *connection)
            .await?
            .into_iter()
            .collect();
    sqlx::query("DELETE FROM contact_methods WHERE contact_id=?")
        .bind(contact)
        .execute(&mut *connection)
        .await?;
    sqlx::query("DELETE FROM customer_interests WHERE contact_id=?")
        .bind(contact)
        .execute(&mut *connection)
        .await?;
    for (position, (method, value)) in input.methods.iter().zip(normalized).enumerate() {
        let (id, created) = child_identity(&mut methods, method.id.as_deref())?;
        sqlx::query(
            "INSERT INTO contact_methods(id,contact_id,kind,value,normalized,label,is_primary,is_verified,position,created_at)
             VALUES (?,?,?,?,?,?,?,?,?,COALESCE(?,unixepoch()))",
        ).bind(id).bind(contact).bind(&method.kind).bind(&method.value).bind(value).bind(&method.label)
            .bind(method.is_primary).bind(method.is_verified).bind(position as i64).bind(created)
            .execute(&mut *connection).await?;
    }
    for (position, interest) in input.interests.iter().enumerate() {
        let (id, created) = child_identity(&mut interests, interest.id.as_deref())?;
        sqlx::query(
            "INSERT INTO customer_interests(id,contact_id,product_text,intent_quantity,quantity_is_estimate,requirement,position,created_at)
             VALUES (?,?,?,?,?,?,?,COALESCE(?,unixepoch()))",
        ).bind(id).bind(contact).bind(&interest.product_text).bind(interest.intent_quantity)
            .bind(interest.quantity_is_estimate).bind(&interest.requirement).bind(position as i64).bind(created)
            .execute(&mut *connection).await?;
    }
    Ok(())
}

fn child_identity(
    existing: &mut HashMap<String, i64>,
    id: Option<&str>,
) -> Result<(String, Option<i64>)> {
    match id {
        Some(id) => existing
            .remove(id)
            .map(|created| (id.to_owned(), Some(created)))
            .ok_or(Error::Invalid("child_id")),
        None => Ok((new_id(), None)),
    }
}

pub async fn archive(
    pool: &SqlitePool,
    id: String,
    archived: bool,
    actor: Actor<'_>,
) -> Result<Reply> {
    let mut tx = pool.begin().await?;
    let current: Option<i64> = sqlx::query_scalar("SELECT archived_at FROM contacts WHERE id=?")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::NotFound)?;
    if current.is_some() != archived {
        sqlx::query("UPDATE contacts SET archived_at=CASE WHEN ? THEN unixepoch() END, updated_at=unixepoch(), updated_by=?, record_revision=record_revision+1 WHERE id=?")
            .bind(archived).bind(actor.id).bind(&id).execute(&mut *tx).await?;
        audit(
            &mut tx,
            actor,
            "contact",
            &id,
            if archived { "archive" } else { "restore" },
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Reply::Saved { id })
}

// Callers provide only field names defined in this module. User values are bound separately.
pub(super) fn state_sql(field: &str) -> String {
    let legacy = if field == "customer_level" {
        "CASE WHEN upper(substr(trim(c.customer_level),1,3))='A |' THEN 'A' ELSE c.customer_level END".to_owned()
    } else {
        format!("c.{field}")
    };
    format!("COALESCE((SELECT code FROM crm_state_aliases WHERE field='{field}' AND alias=trim(c.{field})), {legacy})")
}

async fn canonical_state(pool: &SqlitePool, field: &str, value: &str) -> Result<String> {
    let value = value.trim();
    let alias: Option<String> =
        sqlx::query_scalar("SELECT code FROM crm_state_aliases WHERE field=? AND alias=?")
            .bind(field)
            .bind(value)
            .fetch_optional(pool)
            .await?;
    if let Some(code) = alias {
        Ok(code)
    } else if field == "customer_level"
        && value
            .get(..3)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("A |"))
    {
        Ok("A".into())
    } else {
        Ok(value.to_owned())
    }
}

pub async fn state_catalog(pool: &SqlitePool) -> Result<Reply> {
    let options = sqlx::query("SELECT o.field,o.code,o.label,(SELECT json_group_array(a.alias)
           FROM crm_state_aliases a WHERE a.field=o.field AND a.code=o.code) aliases FROM crm_state_options o ORDER BY o.field,o.position")
        .try_map(|row: SqliteRow| Ok(ftw_ipc::crm::StateOption {
            field: row.get("field"), code: row.get("code"), label: row.get("label"),
            aliases: serde_json::from_str(row.get("aliases")).map_err(|error| sqlx::Error::Decode(Box::new(error)))?,
        }))
        .fetch_all(pool).await?;
    Ok(Reply::StateCatalog { options })
}

pub async fn check_revision(
    db: &mut SqliteConnection,
    id: &str,
    expected: i64,
    expected_epoch: Option<&str>,
) -> Result<()> {
    let (current, epoch): (i64, String) = sqlx::query_as("SELECT record_revision,(SELECT epoch FROM app_change_epoch WHERE singleton=1) FROM contacts WHERE id=?")
        .bind(id).fetch_optional(db).await?.ok_or(Error::NotFound)?;
    if current == expected && expected_epoch.is_none_or(|expected| expected == epoch) {
        Ok(())
    } else {
        Err(Error::Conflict(current))
    }
}
