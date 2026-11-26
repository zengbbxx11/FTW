use super::{page, validation, Error, Result};
use ftw_ipc::crm::{QualityIssue, Result as Reply};
use futures_util::TryStreamExt;
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};
use std::collections::BTreeSet;

const ISSUES: &str = "
WITH duplicate_pairs AS (
    SELECT a.contact_id id,b.contact_id other_id,1 priority
    FROM contact_methods a JOIN contact_methods b ON a.normalized=b.normalized
    WHERE a.kind='email' AND b.kind='email' AND a.normalized<>'' AND a.contact_id<b.contact_id
    UNION ALL
    SELECT a.id,b.id,2 FROM contacts a JOIN contacts b ON a.website_domain=b.website_domain
    WHERE a.website_domain<>'' AND a.display_name=b.display_name COLLATE NOCASE
      AND a.archived_at IS NULL AND b.archived_at IS NULL AND a.id<b.id
    UNION ALL
    SELECT a.id,b.id,3 FROM contacts a JOIN contacts b
      ON a.company_name=b.company_name COLLATE NOCASE AND a.country_region=b.country_region COLLATE NOCASE
    WHERE a.company_name<>'' AND a.country_region<>'' AND a.display_name=b.display_name COLLATE NOCASE
      AND a.archived_at IS NULL AND b.archived_at IS NULL AND a.id<b.id
), issues AS (
    SELECT 'duplicate' category, a.id contact_id,a.display_name contact_name,b.id other_id,b.display_name other_name,
           CASE MIN(p.priority) WHEN 1 THEN 'email' WHEN 2 THEN 'domain' ELSE 'company_country' END detail,
           MIN(p.priority) rank
    FROM duplicate_pairs p JOIN contacts a ON a.id=p.id JOIN contacts b ON b.id=p.other_id
    WHERE a.archived_at IS NULL AND b.archived_at IS NULL GROUP BY a.id,b.id
    UNION ALL
    SELECT 'missing_email',c.id,c.display_name,NULL,NULL,'missing_email',5 FROM contacts c
    WHERE c.archived_at IS NULL AND NOT EXISTS (
      SELECT 1 FROM contact_methods m WHERE m.contact_id=c.id AND m.kind='email')
    UNION ALL
    SELECT 'invalid_email',c.id,c.display_name,NULL,NULL,'invalid_email',4
    FROM json_each(?1) bad JOIN contacts c ON c.id=bad.value
    UNION ALL
    SELECT 'mail_problem',c.id,c.display_name,NULL,NULL,o.mail_status,0
    FROM contacts c JOIN mail_contact_outreach o ON o.contact_id=c.id
    WHERE ?3 AND c.archived_at IS NULL AND o.mail_status IN ('failed','delivery_unknown','bounced')
    UNION ALL
    SELECT 'missing_market',c.id,c.display_name,NULL,NULL,'missing_market',6 FROM contacts c
    WHERE c.archived_at IS NULL AND c.country_region=''
    UNION ALL
    SELECT 'invalid_website',c.id,c.display_name,NULL,NULL,'invalid_website',5
    FROM json_each(?2) bad JOIN contacts c ON c.id=bad.value
    UNION ALL
    SELECT 'date_order',c.id,c.display_name,NULL,NULL,'date_order',5 FROM contacts c
    WHERE c.archived_at IS NULL AND c.first_contact_at>c.last_contact_at
    UNION ALL
    SELECT 'sample_mismatch',c.id,c.display_name,NULL,NULL,'sample_mismatch',5 FROM contacts c
    WHERE c.archived_at IS NULL AND c.stage='sample_in_progress' AND
      COALESCE((SELECT code FROM crm_state_aliases WHERE field='sample_status' AND alias=trim(c.sample_status)),c.sample_status)='none'
    UNION ALL
    SELECT 'excluded_mail',c.id,c.display_name,NULL,NULL,'excluded_mail',0 FROM contacts c
    WHERE ?3 AND c.archived_at IS NULL AND c.stage='excluded' AND EXISTS (
      SELECT 1 FROM email_drafts d WHERE d.contact_id=c.id AND d.state IN ('pending_approval','approved','queued','sending'))
)";

pub async fn list(pool: &SqlitePool, category: &str, keyword: &str, offset: u32) -> Result<Reply> {
    list_scoped(pool, category, keyword, offset, true).await
}

pub(crate) async fn list_scoped(
    pool: &SqlitePool,
    category: &str,
    keyword: &str,
    offset: u32,
    include_mail: bool,
) -> Result<Reply> {
    if !matches!(
        category,
        "" | "duplicate"
            | "missing_email"
            | "invalid_email"
            | "mail_problem"
            | "missing_market"
            | "invalid_website"
            | "date_order"
            | "sample_mismatch"
            | "excluded_mail"
    ) {
        return Err(Error::Invalid("quality_category"));
    }
    let mut keyword = keyword.to_owned();
    validation::text(&mut keyword, "keyword", 300, false)?;
    let mut tx = pool.begin().await?;
    let active_customers: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM contacts WHERE archived_at IS NULL")
            .fetch_one(&mut *tx)
            .await?;
    let mut invalid_contacts = BTreeSet::new();
    {
        let mut emails = sqlx::query_as::<_, (String, String, String)>(
            "SELECT m.contact_id,m.value,m.normalized FROM contact_methods m
             JOIN contacts c ON c.id=m.contact_id WHERE m.kind='email' AND c.archived_at IS NULL",
        )
        .fetch(&mut *tx);
        while let Some((id, email, normalized)) = emails.try_next().await? {
            if validation::email(email.trim()).as_ref().ok() != Some(&normalized) {
                invalid_contacts.insert(id);
            }
        }
    }
    let invalid_contacts = serde_json::to_string(&invalid_contacts)
        .map_err(|error| sqlx::Error::Encode(error.into()))?;
    let mut invalid_websites = BTreeSet::new();
    {
        let mut websites = sqlx::query_as::<_, (String, String, String)>(
            "SELECT id,website_url,website_domain FROM contacts WHERE archived_at IS NULL AND (website_url<>'' OR website_domain<>'')",
        ).fetch(&mut *tx);
        while let Some((id, url, domain)) = websites.try_next().await? {
            if validation::website(url.trim(), "website_url").as_ref().ok() != Some(&domain) {
                invalid_websites.insert(id);
            }
        }
    }
    let invalid_websites = serde_json::to_string(&invalid_websites)
        .map_err(|error| sqlx::Error::Encode(error.into()))?;
    let counts = sqlx::query_as(&format!(
        "{ISSUES} SELECT category,COUNT(*) FROM issues GROUP BY category"
    ))
    .bind(&invalid_contacts)
    .bind(&invalid_websites)
    .bind(include_mail)
    .fetch_all(&mut *tx)
    .await?;
    let rows = sqlx::query(&format!(
        "{ISSUES} SELECT i.*,
           CASE WHEN i.detail IN ('failed','delivery_unknown') THEN (
               SELECT d.id FROM email_drafts d WHERE (d.contact_id=i.contact_id OR d.contact_id IN (
                   SELECT id FROM contacts WHERE merged_into=i.contact_id)) AND d.state=i.detail
               ORDER BY d.updated_at DESC,d.rowid DESC LIMIT 1
           ) WHEN i.detail='excluded_mail' THEN (
               SELECT d.id FROM email_drafts d WHERE d.contact_id=i.contact_id AND d.state IN ('pending_approval','approved','queued','sending')
               ORDER BY d.updated_at DESC,d.rowid DESC LIMIT 1
           ) WHEN i.detail='bounced' THEN (
               SELECT j.draft_id FROM email_messages e JOIN email_outbox_jobs j ON j.id=e.outbox_id
               WHERE e.contact_id=i.contact_id ORDER BY e.submitted_at DESC,e.id DESC LIMIT 1
           ) END draft_id
         FROM issues i JOIN contacts c ON c.id=i.contact_id LEFT JOIN contacts other ON other.id=i.other_id
         WHERE (?4='' OR i.category=?4)
           AND (?5='' OR instr(lower(i.contact_name), lower(?5))>0 OR instr(lower(i.other_name), lower(?5))>0
             OR instr(lower(c.company_name), lower(?5))>0 OR instr(lower(other.company_name), lower(?5))>0)
         ORDER BY i.rank,i.contact_name COLLATE NOCASE,i.contact_id,i.other_id,i.category LIMIT 21 OFFSET ?6",
    ))
    .bind(&invalid_contacts)
    .bind(&invalid_websites)
    .bind(include_mail)
    .bind(category)
    .bind(keyword)
    .bind(i64::from(offset))
    .map(|row: SqliteRow| QualityIssue {
        category: row.get("category"),
        contact_id: row.get("contact_id"),
        contact_name: row.get("contact_name"),
        other_id: row.get("other_id"),
        other_name: row.get("other_name"),
        detail: row.get("detail"),
        draft_id: row.get("draft_id"),
    })
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let (rows, has_more) = page(rows);
    Ok(Reply::CustomerQuality {
        rows,
        counts,
        active_customers,
        has_more,
    })
}
