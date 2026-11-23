use super::Result;
use ftw_ipc::crm::Duplicate;
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection};

pub struct Criteria<'a> {
    pub emails: &'a [&'a str],
    pub domain: &'a str,
    pub company: &'a str,
    pub country: &'a str,
    pub contact_id: &'a str,
    // Candidates are compared to one another during entry and review, and can ignore their own row.
    pub lead_id: Option<&'a str>,
}

pub async fn find(connection: &mut SqliteConnection, keys: Criteria<'_>) -> Result<Vec<Duplicate>> {
    let emails = serde_json::json!(keys.emails).to_string();
    let rows = sqlx::query(
        "WITH keys AS (SELECT ? emails, ? domain, ? company, ? country, ? contact_id, ? lead_id),
         email_keys AS (SELECT value FROM keys, json_each(keys.emails)),
         hits AS (
            SELECT m.contact_id id,'contact' entity,1 rank FROM contact_methods m JOIN contacts active ON active.id=m.contact_id, keys
             WHERE m.kind='email' AND active.merged_into IS NULL AND m.normalized IN (SELECT value FROM email_keys) AND m.contact_id<>keys.contact_id
            UNION ALL SELECT c.id,'contact',2 FROM contacts c, keys
             WHERE c.merged_into IS NULL AND keys.domain<>'' AND c.website_domain=keys.domain AND c.website_domain<>'' AND c.id<>keys.contact_id
            UNION ALL SELECT c.id,'contact',3 FROM contacts c, keys
             WHERE c.merged_into IS NULL AND keys.company<>'' AND keys.country<>'' AND c.company_name=keys.company COLLATE NOCASE
               AND c.country_region=keys.country COLLATE NOCASE AND c.id<>keys.contact_id
            UNION ALL SELECT l.id,'lead',1 FROM lead_candidates l, keys
             WHERE keys.lead_id IS NOT NULL AND l.id<>keys.lead_id AND l.review_status<>'promoted'
               AND l.email_normalized<>'' AND l.email_normalized IN (SELECT value FROM email_keys)
            UNION ALL SELECT l.id,'lead',2 FROM lead_candidates l, keys
             WHERE keys.lead_id IS NOT NULL AND l.id<>keys.lead_id AND l.review_status<>'promoted'
               AND keys.domain<>'' AND l.website_domain=keys.domain AND l.website_domain<>''
            UNION ALL SELECT l.id,'lead',3 FROM lead_candidates l, keys
             WHERE keys.lead_id IS NOT NULL AND l.id<>keys.lead_id AND l.review_status<>'promoted'
               AND keys.company<>'' AND keys.country<>'' AND l.company_name=keys.company COLLATE NOCASE
               AND l.country_region=keys.country COLLATE NOCASE
         )
         SELECT h.id, h.entity, COALESCE(c.display_name,l.display_name) display_name,
           CASE MIN(h.rank) WHEN 1 THEN 'email' WHEN 2 THEN 'domain' ELSE 'company_country' END reason
         FROM hits h LEFT JOIN contacts c ON c.id=h.id AND h.entity='contact'
          LEFT JOIN lead_candidates l ON l.id=h.id AND h.entity='lead'
         GROUP BY h.entity,h.id ORDER BY MIN(h.rank),display_name,h.id LIMIT 12",
    ).bind(emails).bind(keys.domain).bind(keys.company).bind(keys.country)
        .bind(keys.contact_id).bind(keys.lead_id).map(|row: SqliteRow| Duplicate {
        id: row.get("id"), display_name: row.get("display_name"), entity: row.get("entity"), reason: row.get("reason"),
    }).fetch_all(connection).await?;
    Ok(rows)
}
