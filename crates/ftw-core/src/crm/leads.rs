use super::{
    audit, contacts, duplicates, id_or_new, page, validation, Actor, Error, Result, PAGE_SIZE,
};
use ftw_ipc::crm::{ContactInput, Lead, LeadInput, LeadRow, Method, Result as Reply, Source};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

pub async fn list(pool: &SqlitePool, offset: u32, status: &str) -> Result<Reply> {
    validation::choice(
        status,
        "review_status",
        &["", "pending", "rejected", "promoted"],
    )?;
    let rows = sqlx::query(
        "SELECT id,display_name,company_name,country_region,email,review_status,source FROM lead_candidates
         WHERE (?='' OR review_status=?) ORDER BY updated_at DESC,rowid DESC,id LIMIT ? OFFSET ?",
    ).bind(status).bind(status).bind((PAGE_SIZE+1) as i64).bind(i64::from(offset)).map(|row: SqliteRow| LeadRow {
        id: row.get("id"), display_name: row.get("display_name"), company_name: row.get("company_name"),
        country_region: row.get("country_region"), email: row.get("email"), review_status: row.get("review_status"),
        source: row.get("source"),
    }).fetch_all(pool).await?;
    let (rows, has_more) = page(rows);
    Ok(Reply::Leads { rows, has_more })
}

fn input_from(row: &SqliteRow) -> Result<LeadInput> {
    let urls: &str = row.get("source_urls_json");
    let source_urls =
        serde_json::from_str(urls).map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
    Ok(LeadInput {
        display_name: row.get("display_name"),
        company_name: row.get("company_name"),
        country_region: row.get("country_region"),
        email: row.get("email"),
        website_url: row.get("website_url"),
        business_summary: row.get("business_summary"),
        source_urls,
        research_notes: row.get("research_notes"),
        confidence: row.get("confidence"),
    })
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Reply> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query("SELECT * FROM lead_candidates WHERE id=?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::NotFound)?;
    let input = input_from(&row)?;
    let email: &str = row.get("email_normalized");
    let emails = if email.is_empty() {
        &[][..]
    } else {
        &[email][..]
    };
    let duplicate_contacts = duplicates::find(
        &mut tx,
        duplicates::Criteria {
            emails,
            domain: row.get("website_domain"),
            company: &input.company_name,
            country: &input.country_region,
            contact_id: row
                .get::<Option<&str>, _>("promoted_contact_id")
                .unwrap_or(""),
            lead_id: Some(id),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Reply::Lead {
        record: Box::new(Lead {
            id: id.to_owned(),
            input,
            review_status: row.get("review_status"),
            promoted_contact_id: row.get("promoted_contact_id"),
            source: row.get("source"),
        }),
        duplicates: duplicate_contacts,
    })
}

pub async fn save(
    pool: &SqlitePool,
    id: Option<String>,
    mut input: LeadInput,
    allow_duplicate: bool,
    actor: Actor<'_>,
) -> Result<Reply> {
    let (domain, email) = validation::lead(&mut input)?;
    let (id, inserting) = id_or_new(id)?;
    let mut tx = pool.begin().await?;
    if !inserting {
        let state: String =
            sqlx::query_scalar("SELECT review_status FROM lead_candidates WHERE id=?")
                .bind(&id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Error::NotFound)?;
        if state == "promoted" {
            return Err(Error::ReadOnly);
        }
    }
    if !allow_duplicate {
        let emails = if email.is_empty() {
            &[][..]
        } else {
            &[email.as_str()][..]
        };
        let matches = duplicates::find(
            &mut tx,
            duplicates::Criteria {
                emails,
                domain: &domain,
                company: &input.company_name,
                country: &input.country_region,
                contact_id: "",
                lead_id: Some(&id),
            },
        )
        .await?;
        if !matches.is_empty() {
            tx.rollback().await?;
            return Ok(Reply::Duplicates { matches });
        }
    }
    sqlx::query(
        "INSERT INTO lead_candidates(id,display_name,company_name,country_region,email,email_normalized,website_url,
          website_domain,business_summary,source_urls_json,research_notes,confidence,source,created_by_agent_run_id)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET display_name=excluded.display_name,company_name=excluded.company_name,
           country_region=excluded.country_region,email=excluded.email,email_normalized=excluded.email_normalized,
           website_url=excluded.website_url,website_domain=excluded.website_domain,business_summary=excluded.business_summary,
           source_urls_json=excluded.source_urls_json,research_notes=excluded.research_notes,confidence=excluded.confidence,
           updated_at=unixepoch()",
    ).bind(&id).bind(input.display_name).bind(input.company_name).bind(input.country_region)
        .bind(input.email).bind(email).bind(input.website_url).bind(domain).bind(input.business_summary)
        .bind(serde_json::json!(input.source_urls).to_string()).bind(input.research_notes).bind(input.confidence)
        .bind(actor.source.as_str()).bind(actor.agent_run_id).execute(&mut *tx).await?;
    audit(
        &mut tx,
        actor,
        "lead",
        &id,
        if inserting { "create" } else { "update" },
    )
    .await?;
    tx.commit().await?;
    Ok(Reply::Saved { id })
}

pub async fn review(
    pool: &SqlitePool,
    id: String,
    rejected: bool,
    actor: Actor<'_>,
) -> Result<Reply> {
    if !matches!(actor.source, Source::Manual) {
        return Err(Error::ReadOnly);
    }
    let mut tx = pool.begin().await?;
    let state: String = sqlx::query_scalar("SELECT review_status FROM lead_candidates WHERE id=?")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::NotFound)?;
    if state == "promoted" {
        return Err(Error::ReadOnly);
    }
    let next = if rejected { "rejected" } else { "pending" };
    if state != next {
        sqlx::query("UPDATE lead_candidates SET review_status=?,updated_at=unixepoch() WHERE id=?")
            .bind(next)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        audit(
            &mut tx,
            actor,
            "lead",
            &id,
            if rejected { "reject" } else { "reopen" },
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Reply::Saved { id })
}

pub async fn promote(
    pool: &SqlitePool,
    id: String,
    allow_duplicate: bool,
    actor: Actor<'_>,
) -> Result<Reply> {
    if !matches!(actor.source, Source::Manual) {
        return Err(Error::ReadOnly);
    }
    let mut tx = pool.begin().await?;
    let row = sqlx::query("SELECT * FROM lead_candidates WHERE id=?")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::NotFound)?;
    if let Some(contact_id) = row.get::<Option<String>, _>("promoted_contact_id") {
        tx.commit().await?;
        return Ok(Reply::Saved { id: contact_id });
    }
    if row.get::<&str, _>("review_status") != "pending" {
        return Err(Error::ReadOnly);
    }

    let lead = input_from(&row)?;
    let methods = if lead.email.is_empty() {
        Vec::new()
    } else {
        vec![Method {
            id: None,
            kind: "email".into(),
            value: lead.email,
            label: String::new(),
            is_primary: true,
            is_verified: false,
        }]
    };
    let contact = ContactInput {
        display_name: lead.display_name,
        company_name: lead.company_name,
        country_region: lead.country_region,
        website_url: lead.website_url,
        customer_source: "候选客户".into(),
        methods,
        ..ContactInput::default()
    };
    let result = contacts::save(&mut tx, None, contact, allow_duplicate, actor).await?;
    if let Reply::Saved { id: contact_id } = &result {
        sqlx::query("UPDATE lead_candidates SET review_status='promoted',promoted_contact_id=?,updated_at=unixepoch() WHERE id=?")
            .bind(contact_id).bind(&id).execute(&mut *tx).await?;
        audit(&mut tx, actor, "lead", &id, "promote").await?;
    }
    tx.commit().await?;
    Ok(result)
}
