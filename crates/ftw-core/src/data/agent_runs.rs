use super::{Reply, Result};
use crate::mail::tasks::{CANCELLED, CAN_CANCEL, CAN_REOPEN};
use ftw_ipc::data::{
    AgentMailJob, AgentRun, AgentRunCounts, AgentRunFilter as Filter, AgentRunRecord,
};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

const BOUNCES: &str = "SELECT b.outbox_id,max(i.received_at) bounced_at FROM mail_delivery_events b
    JOIN inbox_messages i ON i.id=b.inbox_id WHERE b.action='failed' GROUP BY b.outbox_id";

pub(crate) async fn list(
    pool: &SqlitePool,
    filter: Filter,
    keyword: &str,
    offset: u32,
    agent_id: Option<&str>,
) -> Result<Reply> {
    let keyword = keyword.trim();
    if keyword.chars().count() > 160 || keyword.contains('\0') {
        return Err("invalid_search_keyword");
    }
    let filter = match filter {
        Filter::All => "",
        Filter::Active => "active",
        Filter::Recorded => "recorded",
        Filter::Attention => "attention",
    };
    let statement = format!(
        "WITH bounced AS ({BOUNCES}), mail_groups AS (
            SELECT j.agent_id,a.name,j.task_id,COUNT(DISTINCT coalesce(c.merged_into,c.id)) customers,
                   COUNT(*) jobs,COUNT(*) FILTER(WHERE j.state IN ('queued','sending')) active,
                   COUNT(*) FILTER(WHERE j.state='smtp_accepted') accepted,
                   COUNT(*) FILTER(WHERE j.state='failed' AND j.outcome IS NOT '{CANCELLED}') failures,
                   COUNT(*) FILTER(WHERE j.state='delivery_unknown') unknown,
                   COUNT(*) FILTER(WHERE b.outbox_id IS NOT NULL) bounced,
                   MAX(max(coalesce(e.submitted_at,j.last_attempt_at,j.created_at),coalesce(b.bounced_at,0))) occurred_at
            FROM email_outbox_jobs j JOIN mail_agent_clients a ON a.id=j.agent_id
            JOIN email_drafts d ON d.id=j.draft_id JOIN contacts c ON c.id=d.contact_id
            LEFT JOIN email_messages e ON e.outbox_id=j.id
            LEFT JOIN bounced b ON b.outbox_id=j.id
            WHERE j.task_id IS NOT NULL AND (?4 IS NULL OR j.agent_id=?4) GROUP BY j.agent_id,j.task_id
         ), requests AS (
            SELECT operation,
                   CASE WHEN actor_id IS NULL THEN request_id ELSE substr(request_id,length(actor_id)+3) END request_id,
                   actor_id,created_at,CASE WHEN json_valid(response_json) THEN response_json END response
            FROM agent_requests WHERE (?4 IS NULL OR actor_id=?4)
         ), normalized AS (
            SELECT operation,request_id,actor_id,created_at,coalesce(json_extract(response,'$.data.kind'),'uncertain') kind,
                   json_extract(response,'$.status') status,json_extract(response,'$.code') outcome,
                   CASE WHEN operation IN ('add_lead_evidence','add_contact_evidence') THEN
                      (SELECT COALESCE(e.lead_id,e.contact_id) FROM lead_evidence e WHERE e.id=json_extract(response,'$.data.id'))
                   ELSE coalesce(json_extract(response,'$.data.id'),json_extract(response,'$.data.record.contact_id')) END entity_id
            FROM requests
         ), combined(run_type,record_id,agent_id,name,operation,result,outcome,entity_id,customers,jobs,active,accepted,failures,unknown,bounced,occurred_at,state) AS (
            SELECT 'mail',task_id,agent_id,name,'','mail',NULL,NULL,customers,jobs,active,accepted,failures,unknown,bounced,occurred_at,
                   CASE WHEN failures+unknown+bounced>0 THEN 'attention' WHEN active>0 THEN 'active' ELSE 'recorded' END
            FROM mail_groups
            UNION ALL
            SELECT 'request',request_id,coalesce(n.actor_id,''),coalesce(a.name,'MCP'),operation,
                   CASE WHEN status='error' THEN 'error' ELSE kind END,outcome,entity_id,0,0,0,0,0,0,0,created_at,
                   CASE WHEN status IS NULL OR status='error' OR kind IN ('uncertain','duplicates','agent_review_pending')
                        THEN 'attention' ELSE 'recorded' END
            FROM normalized n LEFT JOIN mail_agent_clients a ON a.id=n.actor_id
         ), matched AS MATERIALIZED (
            SELECT * FROM combined WHERE ?1='' OR instr(lower(record_id || ' ' || name || ' ' || operation),lower(?1))>0
         ), counts AS (
            SELECT count(*) total,count(*) FILTER(WHERE state='active') total_active,
                   count(*) FILTER(WHERE state='recorded') total_recorded,count(*) FILTER(WHERE state='attention') total_attention
            FROM matched
         ), paged AS (
            SELECT * FROM matched WHERE ?2='' OR state=?2
            ORDER BY occurred_at DESC,record_id,run_type,agent_id,operation LIMIT 20 OFFSET ?3
         ) SELECT counts.*,paged.* FROM counts LEFT JOIN paged ON true
         ORDER BY occurred_at DESC,record_id,run_type,agent_id,operation",
    );
    let records = sqlx::query(&statement)
        .bind(keyword)
        .bind(filter)
        .bind(i64::from(offset))
        .bind(agent_id)
        .fetch_all(pool)
        .await
        .map_err(|_| "database_unavailable")?;
    let totals = records.first().ok_or("database_unavailable")?;
    let counts = AgentRunCounts {
        all: totals.get("total"),
        active: totals.get("total_active"),
        recorded: totals.get("total_recorded"),
        attention: totals.get("total_attention"),
    };
    let rows = records
        .into_iter()
        .filter_map(|row| {
            let id = row.get::<Option<String>, _>("record_id")?;
            let record = if row.get::<&str, _>("run_type") == "mail" {
                AgentRunRecord::Mail {
                    agent_id: row.get("agent_id"),
                    agent_name: row.get("name"),
                    task_id: id,
                    customers: row.get("customers"),
                    jobs: row.get("jobs"),
                    active: row.get("active"),
                    accepted: row.get("accepted"),
                    failures: row.get("failures"),
                    unknown: row.get("unknown"),
                    bounced: row.get("bounced"),
                }
            } else {
                let result: String = row.get("result");
                let outcome: Option<String> = row.get("outcome");
                let guidance = ftw_ipc::receipt::guidance(outcome.as_deref().unwrap_or(""));
                let (success, requires_human) = match result.as_str() {
                    "uncertain" => (None, true),
                    "duplicates" | "agent_review_pending" => (Some(false), true),
                    "error" => (Some(false), guidance.requires_human),
                    _ => (Some(true), false),
                };
                AgentRunRecord::Request {
                    agent_id: row.get("agent_id"),
                    agent_name: row.get("name"),
                    operation: row.get("operation"),
                    request_id: id,
                    result,
                    outcome,
                    entity_id: row.get("entity_id"),
                    success,
                    retryable: guidance.retryable,
                    requires_human,
                }
            };
            let state = match row.get::<&str, _>("state") {
                "active" => Filter::Active,
                "attention" => Filter::Attention,
                _ => Filter::Recorded,
            };
            Some(AgentRun {
                record,
                state,
                occurred_at: row.get("occurred_at"),
            })
        })
        .collect();
    Ok(Reply::AgentRuns { counts, rows })
}

pub(crate) async fn mail_jobs(
    pool: &SqlitePool,
    agent: Option<&str>,
    task: &str,
    offset: u32,
) -> Result<Reply> {
    let statement = format!("WITH bounced AS ({BOUNCES})
         SELECT j.id,d.id draft_id,c.id contact_id,c.display_name contact_name,d.subject,j.purpose,
                CASE WHEN b.outbox_id IS NOT NULL THEN 'bounced' WHEN j.outcome='{CANCELLED}' THEN 'cancelled' ELSE j.state END state,
                CASE WHEN b.outbox_id IS NULL THEN j.outcome END outcome,
                ({CAN_CANCEL}) can_cancel, ({CAN_REOPEN}) can_reopen,
                coalesce(b.bounced_at,e.submitted_at,j.last_attempt_at,j.created_at) occurred_at
         FROM email_outbox_jobs j JOIN email_drafts d ON d.id=j.draft_id
         JOIN contacts original ON original.id=d.contact_id JOIN contacts c ON c.id=coalesce(original.merged_into,original.id)
         LEFT JOIN email_messages e ON e.outbox_id=j.id
         LEFT JOIN bounced b ON b.outbox_id=j.id
         WHERE (?1 IS NULL OR j.agent_id=?1) AND j.task_id=?2 ORDER BY occurred_at DESC,j.id LIMIT 21 OFFSET ?3");
    let mut rows = sqlx::query(&statement)
        .bind(agent)
        .bind(task)
        .bind(i64::from(offset))
        .map(|row: SqliteRow| AgentMailJob {
            id: row.get("id"),
            draft_id: row.get("draft_id"),
            contact_id: row.get("contact_id"),
            contact_name: row.get("contact_name"),
            subject: row.get("subject"),
            purpose: row.get("purpose"),
            state: row.get("state"),
            outcome: row.get("outcome"),
            can_cancel: row.get("can_cancel"),
            can_reopen: row.get("can_reopen"),
            occurred_at: row.get("occurred_at"),
        })
        .fetch_all(pool)
        .await
        .map_err(|_| "database_unavailable")?;
    let has_more = rows.len() > 20;
    rows.truncate(20);
    Ok(Reply::AgentRunMail { rows, has_more })
}
