use super::{audit, id_or_new, new_id, page, validation, Actor, Error, Result, PAGE_SIZE};
use ftw_ipc::crm::{
    ArchiveFilter, Contact, ContactInput, ContactQuery, ContactRow, ContactSort, Interest, Method,
    Result as Reply,
};
use sqlx::{sqlite::SqliteRow, QueryBuilder, Row, SqliteConnection, SqlitePool};
use std::collections::HashMap;

pub async fn list(pool: &SqlitePool, mut filters: ContactQuery) -> Result<Reply> {
    validation::text(&mut filters.keyword, "keyword", 300, false)?;
    let mut query = QueryBuilder::new(
        "SELECT c.id, c.display_name, c.company_name, c.country_region, c.stage, c.customer_level,
         c.archived_at, c.source, COALESCE(c.last_contact_at, (SELECT MAX(occurred_at)
            FROM activities WHERE contact_id = c.id)) AS effective_last,
         (SELECT value FROM contact_methods WHERE contact_id = c.id AND kind = 'email'
            ORDER BY is_primary DESC, position LIMIT 1) AS email
         FROM contacts c WHERE 1 = 1",
    );
    match filters.archive {
        ArchiveFilter::Active => {
            query.push(" AND archived_at IS NULL");
        }
        ArchiveFilter::Archived => {
            query.push(" AND archived_at IS NOT NULL");
        }
        ArchiveFilter::All => {}
    }
    for (column, value) in [
        ("country_region", filters.country),
        ("stage", filters.stage),
        ("customer_level", filters.level),
        ("customer_type", filters.customer_type),
        ("customer_source", filters.source),
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
                .push_bind(filters.keyword).push(")) > 0)");
        }
    }
    let order = match filters.sort {
        ContactSort::Updated => "c.updated_at DESC, c.rowid DESC, c.id",
        ContactSort::Name => "c.display_name COLLATE NOCASE, c.id",
        ContactSort::LastContact => "effective_last DESC, c.updated_at DESC, c.id",
    };
    query
        .push(" ORDER BY ")
        .push(order)
        .push(" LIMIT ")
        .push_bind((PAGE_SIZE + 1) as i64)
        .push(" OFFSET ")
        .push_bind(i64::from(filters.offset));
    let rows = query
        .build()
        .map(|row: SqliteRow| ContactRow {
            id: row.get("id"),
            display_name: row.get("display_name"),
            company_name: row.get("company_name"),
            country_region: row.get("country_region"),
            stage: row.get("stage"),
            customer_level: row.get("customer_level"),
            email: row.get("email"),
            last_contact_at: row.get("effective_last"),
            archived_at: row.get("archived_at"),
            source: row.get("source"),
        })
        .fetch_all(pool)
        .await?;
    let (rows, has_more) = page(rows);
    Ok(Reply::Contacts { rows, has_more })
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Reply> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query(
        "SELECT c.*, (SELECT MIN(occurred_at) FROM activities WHERE contact_id=c.id) observed_first,
                     (SELECT MAX(occurred_at) FROM activities WHERE contact_id=c.id) observed_last
         FROM contacts c WHERE id=?",
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
          alibaba_buyer_id=excluded.alibaba_buyer_id,notes=excluded.notes,updated_by=excluded.updated_by,updated_at=unixepoch()
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
        sqlx::query("UPDATE contacts SET archived_at=CASE WHEN ? THEN unixepoch() END, updated_at=unixepoch(), updated_by=? WHERE id=?")
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
