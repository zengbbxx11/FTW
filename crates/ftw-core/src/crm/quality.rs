use super::{page, Error, Result};
use ftw_ipc::crm::{QualityIssue, Result as Reply};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

const ISSUES: &str = "
WITH duplicate_pairs AS (
    SELECT a.contact_id id,b.contact_id other_id,1 priority
    FROM contact_methods a JOIN contact_methods b ON a.normalized=b.normalized
    WHERE a.kind='email' AND b.kind='email' AND a.contact_id<b.contact_id
    UNION ALL
    SELECT a.id,b.id,2 FROM contacts a JOIN contacts b ON a.website_domain=b.website_domain
    WHERE a.website_domain<>'' AND a.archived_at IS NULL AND b.archived_at IS NULL AND a.id<b.id
    UNION ALL
    SELECT a.id,b.id,3 FROM contacts a JOIN contacts b
      ON a.company_name=b.company_name COLLATE NOCASE AND a.country_region=b.country_region COLLATE NOCASE
    WHERE a.company_name<>'' AND a.country_region<>'' AND a.archived_at IS NULL AND b.archived_at IS NULL AND a.id<b.id
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
    SELECT 'mail_problem',c.id,c.display_name,NULL,NULL,o.mail_status,4
    FROM contacts c JOIN mail_contact_outreach o ON o.contact_id=c.id
    WHERE c.archived_at IS NULL AND o.mail_status IN ('failed','delivery_unknown','bounced')
    UNION ALL
    SELECT 'missing_market',c.id,c.display_name,NULL,NULL,'missing_market',6 FROM contacts c
    WHERE c.archived_at IS NULL AND c.country_region=''
)";

pub async fn list(pool: &SqlitePool, category: &str, offset: u32) -> Result<Reply> {
    if !matches!(
        category,
        "" | "duplicate" | "missing_email" | "mail_problem" | "missing_market"
    ) {
        return Err(Error::Invalid("quality_category"));
    }
    let mut tx = pool.begin().await?;
    let counts = sqlx::query_as(&format!(
        "{ISSUES} SELECT category,COUNT(*) FROM issues GROUP BY category"
    ))
    .fetch_all(&mut *tx)
    .await?;
    let rows = sqlx::query(&format!(
        "{ISSUES} SELECT * FROM issues WHERE (?='' OR category=?)
         ORDER BY rank,contact_name COLLATE NOCASE,contact_id,other_id LIMIT 21 OFFSET ?",
    ))
    .bind(category)
    .bind(category)
    .bind(i64::from(offset))
    .map(|row: SqliteRow| QualityIssue {
        category: row.get("category"),
        contact_id: row.get("contact_id"),
        contact_name: row.get("contact_name"),
        other_id: row.get("other_id"),
        other_name: row.get("other_name"),
        detail: row.get("detail"),
    })
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let (rows, has_more) = page(rows);
    Ok(Reply::CustomerQuality {
        rows,
        counts,
        has_more,
    })
}
