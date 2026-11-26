use super::{contacts, page, validation, Result, PAGE_SIZE};
use ftw_ipc::crm::{Result as Reply, SampleRow};
use sqlx::{sqlite::SqliteRow, QueryBuilder, Row, SqlitePool};

pub async fn list(
    pool: &SqlitePool,
    mut keyword: String,
    status: &str,
    offset: u32,
    by_batch: bool,
) -> Result<Reply> {
    validation::text(&mut keyword, "keyword", 300, false)?;
    let status_sql = contacts::state_sql("sample_status");
    let customer_source = format!(
        "WITH samples AS (
            SELECT c.id id,c.id contact_id,c.display_name contact_name,c.company_name,c.country_region,
                COALESCE(NULLIF({status_sql},''),CASE WHEN c.stage='sample_in_progress' THEN 'in_progress' ELSE '' END) status,
                COALESCE(a.sample_model,'') model,a.sample_quantity quantity,a.occurred_at,
                COALESCE(a.summary,'') summary,COALESCE(a.details,'') details,COALESCE(a.customer_feedback,'') feedback,
                COALESCE(a.sample_batch_ref,'') batch_ref,NULL events,a.id activity_id
            FROM contacts c LEFT JOIN activities a ON a.id=(
                SELECT id FROM activities WHERE contact_id=c.id AND event_type='sample' ORDER BY occurred_at DESC,id LIMIT 1
            )
            WHERE c.archived_at IS NULL AND (a.id IS NOT NULL OR {status_sql} NOT IN ('','none','not_sent') OR c.stage='sample_in_progress')
        )"
    );
    let batch_source = "WITH events AS (
            SELECT a.*,a.rowid rowid_key,CASE WHEN a.sample_batch_ref='' THEN 'event:'||a.id ELSE 'batch:'||lower(a.sample_batch_ref) END batch_key
            FROM activities a WHERE a.event_type='sample'
        ), ranked AS (
            SELECT *,ROW_NUMBER() OVER(PARTITION BY contact_id,batch_key ORDER BY occurred_at DESC,rowid_key DESC) rank,
                COUNT(*) OVER(PARTITION BY contact_id,batch_key) events
            FROM events
        ), samples AS (
            SELECT c.id||':'||a.batch_key id,c.id contact_id,c.display_name contact_name,c.company_name,c.country_region,
                COALESCE((SELECT e.sample_progress FROM ranked e WHERE e.contact_id=c.id AND e.batch_key=a.batch_key
                    AND e.sample_progress<>'' ORDER BY e.rank LIMIT 1),'') status,
                COALESCE((SELECT e.sample_model FROM ranked e WHERE e.contact_id=c.id AND e.batch_key=a.batch_key
                    AND e.sample_model<>'' ORDER BY e.rank LIMIT 1),'') model,
                (SELECT e.sample_quantity FROM ranked e WHERE e.contact_id=c.id AND e.batch_key=a.batch_key
                    AND e.sample_quantity IS NOT NULL ORDER BY e.rank LIMIT 1) quantity,
                a.occurred_at,a.summary,a.details,a.customer_feedback feedback,a.sample_batch_ref batch_ref,a.events,a.id activity_id
            FROM ranked a JOIN contacts c ON c.id=a.contact_id WHERE a.rank=1 AND c.archived_at IS NULL
        )";
    let source = if by_batch {
        batch_source
    } else {
        &customer_source
    };
    let mut tx = pool.begin().await?;
    let counts = sqlx::query_as(&format!(
        "{source} SELECT status,COUNT(*) FROM samples GROUP BY status"
    ))
    .fetch_all(&mut *tx)
    .await?;
    let mut query = QueryBuilder::new(format!("{source} SELECT * FROM samples WHERE 1=1"));
    if !status.is_empty() {
        query.push(" AND status=").push_bind(status);
    }
    if !keyword.is_empty() {
        query.push(" AND (");
        let mut predicates = query.separated(" OR ");
        for field in [
            "contact_name",
            "company_name",
            "country_region",
            "model",
            "batch_ref",
            "summary",
            "feedback",
        ] {
            predicates
                .push(format!("instr(lower({field}),lower("))
                .push_bind_unseparated(&keyword)
                .push_unseparated("))>0");
        }
        query.push(")");
    }
    query
        .push(" ORDER BY occurred_at DESC,contact_name COLLATE NOCASE,id LIMIT ")
        .push_bind((PAGE_SIZE + 1) as i64)
        .push(" OFFSET ")
        .push_bind(i64::from(offset));
    let rows = query
        .build()
        .map(|row: SqliteRow| SampleRow {
            id: row.get("id"),
            contact_id: row.get("contact_id"),
            contact_name: row.get("contact_name"),
            company_name: row.get("company_name"),
            country_region: row.get("country_region"),
            status: row.get("status"),
            model: row.get("model"),
            quantity: row.get("quantity"),
            summary: row.get("summary"),
            details: row.get("details"),
            feedback: row.get("feedback"),
            occurred_at: row.get("occurred_at"),
            batch_ref: row.get("batch_ref"),
            events: row.get("events"),
            activity_id: row.get("activity_id"),
        })
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;
    let (rows, has_more) = page(rows);
    Ok(Reply::SampleWorkspace {
        rows,
        counts,
        has_more,
    })
}
