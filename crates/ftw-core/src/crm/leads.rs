use super::{
    audit, contacts, duplicates, id_or_new, page, validation, Actor, Error, Result, PAGE_SIZE,
};
use ftw_ipc::crm::{
    ContactInput, Lead, LeadInput, LeadPatch, LeadRow, LeadVersion, LeadWorkspaceRow, Method,
    Result as Reply, Source,
};
use sqlx::{sqlite::SqliteRow, QueryBuilder, Row, SqliteConnection, SqlitePool};

pub async fn list(pool: &SqlitePool, offset: u32, status: &str) -> Result<Reply> {
    let (rows, has_more) = page(list_rows(pool, offset, status, "", "").await?);
    Ok(Reply::Leads {
        rows: rows.iter().map(lead_row).collect(),
        has_more,
    })
}

pub async fn workspace(
    pool: &SqlitePool,
    offset: u32,
    status: &str,
    mut keyword: String,
    source: &str,
) -> Result<Reply> {
    validation::text(&mut keyword, "keyword", 320, false)?;
    validation::choice(source, "source", &["", "manual", "agent"])?;
    let (rows, has_more) = page(list_rows(pool, offset, status, &keyword, source).await?);
    let rows = rows
        .iter()
        .map(|row| LeadWorkspaceRow {
            lead: lead_row(row),
            website_url: row.get("website_url"),
            business_summary: row.get("business_summary"),
            evidence_count: row.get("evidence_count"),
        })
        .collect();
    Ok(Reply::LeadWorkspace { rows, has_more })
}

async fn list_rows(
    pool: &SqlitePool,
    offset: u32,
    status: &str,
    keyword: &str,
    source: &str,
) -> Result<Vec<SqliteRow>> {
    validation::choice(
        status,
        "review_status",
        &["", "pending", "rejected", "promoted"],
    )?;
    let mut query = QueryBuilder::new(
        "SELECT id,display_name,company_name,country_region,email,review_status,source,website_url,business_summary,
            (SELECT COUNT(*) FROM lead_evidence e WHERE e.lead_id=lead_candidates.id) evidence_count
         FROM lead_candidates WHERE 1=1",
    );
    if !status.is_empty() {
        query.push(" AND review_status=").push_bind(status);
    }
    if !source.is_empty() {
        query.push(" AND source=").push_bind(source);
    }
    if !keyword.is_empty() {
        query.push(" AND (");
        let mut predicates = query.separated(" OR ");
        for field in [
            "display_name",
            "company_name",
            "country_region",
            "email",
            "website_url",
            "business_summary",
        ] {
            predicates
                .push(format!("instr(lower({field}),lower("))
                .push_bind_unseparated(keyword)
                .push_unseparated("))>0");
        }
        predicates
            .push(
                "EXISTS(SELECT 1 FROM lead_evidence e WHERE e.lead_id=lead_candidates.id
             AND instr(lower(e.subject || ' ' || e.url || ' ' || e.note),lower(",
            )
            .push_bind_unseparated(keyword)
            .push_unseparated("))>0)");
        query.push(")");
    }
    query
        .push(" ORDER BY updated_at DESC,rowid DESC,id LIMIT ")
        .push_bind((PAGE_SIZE + 1) as i64)
        .push(" OFFSET ")
        .push_bind(i64::from(offset));
    Ok(query.build().fetch_all(pool).await?)
}

fn lead_row(row: &SqliteRow) -> LeadRow {
    LeadRow {
        id: row.get("id"),
        display_name: row.get("display_name"),
        company_name: row.get("company_name"),
        country_region: row.get("country_region"),
        email: row.get("email"),
        review_status: row.get("review_status"),
        source: row.get("source"),
    }
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
    let row = find(&mut tx, id).await?;
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
            record_revision: row.get("record_revision"),
            data_epoch: row.get("data_epoch"),
            input,
            review_status: row.get("review_status"),
            promoted_contact_id: row.get("promoted_contact_id"),
            source: row.get("source"),
            created_at: row.get("created_at"),
        }),
        duplicates: duplicate_contacts,
    })
}

pub async fn save(
    pool: &SqlitePool,
    id: Option<String>,
    input: LeadInput,
    allow_duplicate: bool,
    expected: Option<LeadVersion>,
    actor: Actor<'_>,
) -> Result<Reply> {
    let (id, inserting) = id_or_new(id)?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if !inserting {
        let row = find(&mut tx, &id).await?;
        check_version(&row, expected.as_ref())?;
        if row.get::<&str, _>("review_status") == "promoted" {
            return Err(Error::ReadOnly);
        }
    }
    let reply = write(&mut tx, id, inserting, input, allow_duplicate, actor).await?;
    tx.commit().await?;
    Ok(match reply {
        Reply::LeadUpdated { id, .. } => Reply::Saved { id },
        other => other,
    })
}

pub async fn patch(
    pool: &SqlitePool,
    id: String,
    expected: LeadVersion,
    patch: LeadPatch,
    actor: Actor<'_>,
) -> Result<Reply> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let row = find(&mut tx, &id).await?;
    check_version(&row, Some(&expected))?;
    if row.get::<&str, _>("review_status") != "pending" {
        return Err(Error::ReadOnly);
    }
    let mut input = input_from(&row)?;
    let previous = input.clone();
    for (value, replacement) in [
        (&mut input.display_name, patch.display_name),
        (&mut input.company_name, patch.company_name),
        (&mut input.country_region, patch.country_region),
        (&mut input.email, patch.email),
        (&mut input.website_url, patch.website_url),
        (&mut input.business_summary, patch.business_summary),
        (&mut input.research_notes, patch.research_notes),
    ] {
        if let Some(replacement) = replacement {
            *value = replacement;
        }
    }
    input.confidence = patch.confidence.or(input.confidence);
    for url in patch.add_source_urls {
        let url = url.trim();
        if !input.source_urls.iter().any(|saved| saved == url) {
            input.source_urls.push(url.to_owned());
        }
    }
    // Run duplicate checks when the candidate identity changes. Existing matches stay visible in detail.
    let identity_unchanged = input.display_name == previous.display_name
        && input.company_name == previous.company_name
        && input.country_region == previous.country_region
        && input.email == previous.email
        && input.website_url == previous.website_url;
    let reply = write(&mut tx, id, false, input, identity_unchanged, actor).await?;
    tx.commit().await?;
    Ok(reply)
}

async fn find(db: &mut SqliteConnection, id: &str) -> Result<SqliteRow> {
    sqlx::query("SELECT l.*, (SELECT epoch FROM app_change_epoch WHERE singleton=1) data_epoch FROM lead_candidates l WHERE id=?")
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or(Error::NotFound)
}

fn check_version(row: &SqliteRow, expected: Option<&LeadVersion>) -> Result<()> {
    let revision: i64 = row.get("record_revision");
    if expected.is_some_and(|version| {
        version.revision != revision || version.epoch != row.get::<&str, _>("data_epoch")
    }) {
        return Err(Error::Conflict(revision));
    }
    Ok(())
}

async fn write(
    db: &mut SqliteConnection,
    id: String,
    inserting: bool,
    mut input: LeadInput,
    allow_duplicate: bool,
    actor: Actor<'_>,
) -> Result<Reply> {
    let (domain, email) = validation::lead(&mut input)?;
    if !allow_duplicate {
        let emails = if email.is_empty() {
            &[][..]
        } else {
            &[email.as_str()][..]
        };
        let matches = duplicates::find(
            db,
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
            return Ok(Reply::Duplicates { matches });
        }
    }
    let revision = sqlx::query_scalar(
        "INSERT INTO lead_candidates(id,display_name,company_name,country_region,email,email_normalized,website_url,
          website_domain,business_summary,source_urls_json,research_notes,confidence,source,created_by_agent_run_id)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET display_name=excluded.display_name,company_name=excluded.company_name,
           country_region=excluded.country_region,email=excluded.email,email_normalized=excluded.email_normalized,
           website_url=excluded.website_url,website_domain=excluded.website_domain,business_summary=excluded.business_summary,
           source_urls_json=excluded.source_urls_json,research_notes=excluded.research_notes,confidence=excluded.confidence,
           updated_at=unixepoch(),record_revision=lead_candidates.record_revision+1
         RETURNING record_revision",
    ).bind(&id).bind(input.display_name).bind(input.company_name).bind(input.country_region)
        .bind(input.email).bind(email).bind(input.website_url).bind(domain).bind(input.business_summary)
        .bind(serde_json::json!(input.source_urls).to_string()).bind(input.research_notes).bind(input.confidence)
        .bind(actor.source.as_str()).bind(actor.agent_run_id).fetch_one(&mut *db).await?;
    super::evidence::record_legacy_links(db, &id, &input.source_urls, actor).await?;
    audit(
        db,
        actor,
        "lead",
        &id,
        if inserting { "create" } else { "update" },
    )
    .await?;
    Ok(Reply::LeadUpdated { id, revision })
}

pub async fn review(
    pool: &SqlitePool,
    id: String,
    rejected: bool,
    expected: Option<LeadVersion>,
    actor: Actor<'_>,
) -> Result<Reply> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let row = find(&mut tx, &id).await?;
    check_version(&row, expected.as_ref())?;
    let state: &str = row.get("review_status");
    if state == "promoted" {
        return Err(Error::ReadOnly);
    }
    let next = if rejected { "rejected" } else { "pending" };
    if state != next {
        if matches!(actor.source, Source::Agent) && !rejected {
            let human_rejected: bool = sqlx::query_scalar(
                "SELECT COALESCE((SELECT source='manual' FROM audit_logs
                 WHERE entity='lead' AND entity_id=? AND operation='reject' ORDER BY rowid DESC LIMIT 1),1)",
            )
            .bind(&id)
            .fetch_one(&mut *tx)
            .await?;
            if human_rejected {
                return Err(Error::ReadOnly);
            }
        }
        sqlx::query("UPDATE lead_candidates SET review_status=?,updated_at=unixepoch(),record_revision=record_revision+1 WHERE id=?")
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
    expected: Option<LeadVersion>,
    actor: Actor<'_>,
) -> Result<Reply> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let row = find(&mut tx, &id).await?;
    check_version(&row, expected.as_ref())?;
    if let Some(contact_id) = row.get::<Option<String>, _>("promoted_contact_id") {
        tx.commit().await?;
        return Ok(Reply::Saved { id: contact_id });
    }
    if row.get::<&str, _>("review_status") != "pending" {
        return Err(Error::ReadOnly);
    }
    if matches!(actor.source, Source::Agent) {
        // The last saved identity must have been reviewed in the desktop form.
        let human_checked: bool = sqlx::query_scalar(
            "SELECT COALESCE((SELECT source='manual' FROM audit_logs
             WHERE entity='lead' AND entity_id=? AND operation IN ('create','update')
             ORDER BY rowid DESC LIMIT 1),0)",
        )
        .bind(&id)
        .fetch_one(&mut *tx)
        .await?;
        if !human_checked || allow_duplicate {
            return Err(Error::Invalid("lead_promotion_review_required"));
        }
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
        sqlx::query("UPDATE lead_candidates SET review_status='promoted',promoted_contact_id=?,updated_at=unixepoch(),record_revision=record_revision+1 WHERE id=?")
            .bind(contact_id).bind(&id).execute(&mut *tx).await?;
        audit(&mut tx, actor, "lead", &id, "promote").await?;
    }
    tx.commit().await?;
    Ok(result)
}
