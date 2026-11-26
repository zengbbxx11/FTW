use super::{Reply, Result};
use crate::mail::file_workspace::FILES_SQL;
use ftw_ipc::data::{SearchCategory as Category, SearchCounts, SearchHit, SearchTarget as Target};
use sqlx::{Row, SqlitePool};

pub(crate) async fn search(
    pool: &SqlitePool,
    keyword: &str,
    category: Category,
    offset: u32,
) -> Result<Reply> {
    search_records(pool, keyword, category, offset, true).await
}

pub(crate) async fn search_for_agent(
    pool: &SqlitePool,
    keyword: &str,
    category: Category,
    offset: u32,
) -> Result<Reply> {
    search_records(pool, keyword, category, offset, false).await
}

async fn search_records(
    pool: &SqlitePool,
    keyword: &str,
    category: Category,
    offset: u32,
    include_private_assets: bool,
) -> Result<Reply> {
    let terms: Vec<_> = keyword.split_whitespace().collect();
    if terms.is_empty() || keyword.chars().count() > 160 || keyword.contains('\0') {
        return Err("invalid_search_keyword");
    }
    let terms = serde_json::to_string(&terms).map_err(|_| "invalid_search_keyword")?;
    let kind = match category {
        Category::All => "",
        Category::Customer => "customer",
        Category::Mail => "mail",
        Category::Activity => "activity",
        Category::File => "file",
    };
    let statement = format!(
        "WITH files AS ({FILES_SQL}), terms(t) AS (SELECT lower(value) FROM json_each(?1)),
         records(category,record_type,id,contact_id,file_index,title,detail,preview,contents,occurred_at) AS (
            SELECT 'customer','customer',c.id,c.id,0,c.display_name,
                   trim(c.company_name || ' ' || c.country_region),c.main_need,coalesce(s.searchable,''),c.updated_at
            FROM contacts c LEFT JOIN contact_search s ON s.rowid=c.rowid WHERE c.merged_into IS NULL
            UNION ALL
            SELECT 'mail','inbox',i.id,i.contact_id,0,i.subject,i.sender,i.body_text,i.body_text,i.received_at
            FROM inbox_messages i
            UNION ALL
            SELECT 'mail','draft',d.id,d.contact_id,0,d.subject,d.recipient,d.body_text,d.body_text,d.updated_at
            FROM email_drafts d
            UNION ALL
            SELECT 'activity','activity',a.id,a.contact_id,0,a.summary,c.display_name,
                   coalesce(nullif(a.customer_feedback,''),a.details),
                   a.details || ' ' || a.customer_feedback || ' ' || a.sample_model,a.occurred_at
            FROM activities a JOIN contacts c ON c.id=a.contact_id
            UNION ALL
            SELECT 'file',f.origin,coalesce(f.document_id,f.message_id),c.id,f.file_index,f.filename,
                   c.display_name,f.subject,f.subject,f.occurred_at
            FROM files f JOIN contacts original ON original.id=f.contact_id
            JOIN contacts c ON c.id=coalesce(original.merged_into,original.id)
            WHERE f.filename IS NOT NULL AND f.filename<>''
            AND (f.origin='document' OR ?5 OR f.agent_allowed)
         ), matched AS MATERIALIZED (
            SELECT category,record_type,id,contact_id,file_index,title,detail,substr(preview,1,350) preview,occurred_at,
                   CASE WHEN lower(title)=lower(?2) THEN 0
                        WHEN category='customer' AND instr(lower(detail),lower(?2))=1 THEN 1
                        WHEN NOT EXISTS(SELECT 1 FROM terms WHERE instr(lower(title),t)=0) THEN 2
                        WHEN NOT EXISTS(SELECT 1 FROM terms WHERE instr(lower(title || ' ' || detail),t)=0) THEN 3
                        ELSE 4 END rank
            FROM records r WHERE NOT EXISTS(
                SELECT 1 FROM terms WHERE instr(lower(r.title || ' ' || r.detail || ' ' || r.contents),t)=0
            )
         ), counts AS (
            SELECT count(*) total,count(*) FILTER (WHERE category='customer') customers,
                   count(*) FILTER (WHERE category='mail') mail,count(*) FILTER (WHERE category='activity') activities,
                   count(*) FILTER (WHERE category='file') files FROM matched
         ), paged AS (
            SELECT * FROM matched WHERE ?3='' OR category=?3
            ORDER BY rank,occurred_at DESC,record_type,id,file_index LIMIT 20 OFFSET ?4
         ) SELECT counts.*,paged.* FROM counts LEFT JOIN paged ON true
         ORDER BY rank,occurred_at DESC,record_type,id,file_index"
    );
    let results = sqlx::query(&statement)
        .bind(terms)
        .bind(keyword.trim())
        .bind(kind)
        .bind(i64::from(offset))
        .bind(include_private_assets)
        .fetch_all(pool)
        .await
        .map_err(|_| "database_unavailable")?;
    let totals = results.first().ok_or("database_unavailable")?;
    let counts = SearchCounts {
        all: totals.get("total"),
        customer: totals.get("customers"),
        mail: totals.get("mail"),
        activity: totals.get("activities"),
        file: totals.get("files"),
    };
    let mut rows = Vec::with_capacity(results.len());
    for row in results {
        let Some(id) = row.get::<Option<String>, _>("id") else {
            continue;
        };
        let index: i64 = row.get("file_index");
        let (target, category) = match row.get::<&str, _>("record_type") {
            "customer" => (Target::Customer { id }, Category::Customer),
            "inbox" if row.get::<&str, _>("category") == "mail" => {
                (Target::Inbox { id }, Category::Mail)
            }
            "draft" => (Target::Draft { id }, Category::Mail),
            "activity" => (Target::Activity { id }, Category::Activity),
            "document" => (
                Target::Document {
                    id,
                    contact_id: row.get("contact_id"),
                },
                Category::File,
            ),
            "inbox" => (Target::InboxAttachment { id, index }, Category::File),
            "outbound" => (Target::OutboundAttachment { id, index }, Category::File),
            _ => return Err("database_unavailable"),
        };
        rows.push(SearchHit {
            target,
            category,
            title: row.get("title"),
            detail: row.get("detail"),
            preview: row.get("preview"),
            occurred_at: row.get("occurred_at"),
        });
    }
    Ok(Reply::Search { counts, rows })
}
