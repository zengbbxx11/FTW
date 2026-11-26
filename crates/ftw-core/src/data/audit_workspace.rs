use super::{audit_row, Reply, Result};
use ftw_ipc::data::{AuditEntry, AuditTarget};
use sqlx::{Row, SqlitePool};

pub(super) async fn list(
    pool: &SqlitePool,
    offset: u32,
    source: &str,
    entity: &str,
    keyword: &str,
    days: u16,
) -> Result<Reply> {
    list_scoped(pool, offset, source, entity, keyword, days, false).await
}

pub(crate) async fn list_scoped(
    pool: &SqlitePool,
    offset: u32,
    source: &str,
    entity: &str,
    keyword: &str,
    days: u16,
    shared_only: bool,
) -> Result<Reply> {
    let keyword = keyword.trim();
    if !matches!(source, "" | "manual" | "agent" | "system")
        || !matches!(days, 0 | 7 | 30 | 90)
        || keyword.chars().count() > 160
        || keyword.contains('\0')
        || entity.len() > 96
        || entity.contains('\0')
    {
        return Err("invalid_audit_filter");
    }
    let allowed = shared_only.then(|| {
        serde_json::json!([
            "contact",
            "lead",
            "activity",
            "company",
            "email_draft",
            "email_outbox",
            "inbox_message",
            "suppression",
            "email_template",
            "customer_document",
            "custom_field",
            "customer_view",
            "customer_csv"
        ])
        .to_string()
    });
    let mut tx = pool.begin().await.map_err(|_| "database_unavailable")?;
    let entities =
        sqlx::query_scalar("SELECT DISTINCT entity FROM audit_logs
            WHERE (?1 IS NULL OR entity IN (SELECT value FROM json_each(?1))) ORDER BY entity LIMIT 100")
            .bind(&allowed)
            .fetch_all(&mut *tx)
            .await
            .map_err(|_| "database_unavailable")?;
    let mut rows = sqlx::query(
        "WITH enriched AS (
            SELECT a.*,r.name actor_name,a.rowid log_order,coalesce(c.display_name,l.display_name,x.summary,d.subject,i.subject,f.filename) label,
               coalesce(c.company_name,l.company_name,'') organization,
               CASE WHEN c.id IS NOT NULL THEN 'customer' WHEN l.id IS NOT NULL THEN 'lead'
                 WHEN x.id IS NOT NULL THEN 'activity' WHEN d.id IS NOT NULL THEN 'draft' WHEN i.id IS NOT NULL THEN 'inbox' END target,
               coalesce(c.id,l.id,x.id,d.id,i.id) target_id
            FROM audit_logs a
            LEFT JOIN mail_agent_clients r ON r.id=a.actor_id
            LEFT JOIN contacts c ON a.entity='contact' AND a.entity_id=c.id
            LEFT JOIN lead_candidates l ON a.entity='lead' AND a.entity_id=l.id
            LEFT JOIN activities x ON a.entity='activity' AND a.entity_id=x.id
            LEFT JOIN email_outbox_jobs j ON a.entity='email_outbox' AND a.entity_id=j.id
            LEFT JOIN email_drafts d ON d.id=CASE WHEN a.entity='email_draft' THEN a.entity_id ELSE j.draft_id END
            LEFT JOIN inbox_messages i ON a.entity='inbox_message' AND a.entity_id=i.id
            LEFT JOIN customer_documents f ON a.entity='customer_document' AND a.entity_id=f.id
            WHERE (?1='' OR a.source=?1) AND (?2='' OR a.entity=?2)
              AND (?6 IS NULL OR a.entity IN (SELECT value FROM json_each(?6)))
              AND (?4=0 OR a.created_at>=unixepoch(date('now','localtime',printf('-%d days',?4-1)),'utc'))
         ) SELECT * FROM enriched WHERE ?3='' OR instr(lower(actor_id || ' ' || coalesce(actor_name,'') || ' ' || coalesce(task_id,'')
           || ' ' || entity || ' ' || entity_id || ' ' || operation || ' ' || organization || ' ' || coalesce(label,'')),lower(?3))>0
           ORDER BY created_at DESC, log_order DESC LIMIT 21 OFFSET ?5",
    ).bind(source).bind(entity).bind(keyword).bind(days).bind(i64::from(offset)).bind(&allowed)
        .map(|row| {
            let target = row.get::<Option<String>,_>("target_id").and_then(|id| match row.get::<&str,_>("target") {
                "customer" => Some(AuditTarget::Customer { id }), "lead" => Some(AuditTarget::Lead { id }),
                "activity" => Some(AuditTarget::Activity { id }), "draft" => Some(AuditTarget::Draft { id }),
                "inbox" => Some(AuditTarget::Inbox { id }), _ => None,
            });
            AuditEntry { id: row.get("id"), label: row.get("label"), target, row: audit_row(row) }
        }).fetch_all(&mut *tx).await.map_err(|_| "database_unavailable")?;
    tx.commit().await.map_err(|_| "database_unavailable")?;
    let has_more = rows.len() > 20;
    rows.truncate(20);
    Ok(Reply::AuditWorkspace {
        rows,
        entities,
        has_more,
    })
}
