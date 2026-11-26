use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Dashboard,
    DataOverview,
    Search {
        keyword: String,
        category: SearchCategory,
        offset: u32,
    },
    AgentRuns {
        filter: AgentRunFilter,
        keyword: String,
        offset: u32,
    },
    AgentRunMail {
        agent_id: String,
        task_id: String,
        offset: u32,
    },
    UpdateMailJob {
        job_id: String,
        job_action: MailJobAction,
    },
    MailAnalytics {
        days: u16,
        include_legacy: bool,
    },
    ExportMailAnalytics {
        days: u16,
        include_legacy: bool,
    },
    Backups,
    CreateBackup,
    InspectBackup {
        filename: String,
    },
    Restore {
        filename: String,
    },
    Audit {
        offset: u32,
    },
    AuditWorkspace {
        offset: u32,
        source: String,
        entity: String,
        keyword: String,
        days: u16,
    },
    ImportContacts {
        csv: String,
        apply: bool,
        default_stage: String,
    },
    ExportContacts,
    ExportSelectedContacts {
        scope: crate::crm::ArchiveFilter,
        fields: Vec<String>,
    },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Backup {
    pub filename: String,
    pub bytes: u64,
    pub created_at: i64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AuditRow {
    pub occurred_at: i64,
    pub actor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub source: String,
    pub entity: String,
    pub entity_id: String,
    pub operation: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "record_type", rename_all = "snake_case")]
pub enum AuditTarget {
    Customer { id: String },
    Lead { id: String },
    Activity { id: String },
    Draft { id: String },
    Inbox { id: String },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AuditEntry {
    pub id: String,
    #[serde(flatten)]
    pub row: AuditRow,
    pub label: Option<String>,
    pub target: Option<AuditTarget>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ImportIssue {
    pub row: usize,
    pub reason: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ImportMapping {
    pub source: String,
    pub field: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SummaryRow {
    pub title: String,
    pub detail: String,
    pub occurred_at: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RecentActivity {
    pub id: String,
    pub event_type: String,
    pub source: String,
    #[serde(flatten)]
    pub summary: SummaryRow,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DashboardStats {
    pub contacts: i64,
    pub smtp_accepted: i64,
    pub waiting: i64,
    pub replied: i64,
    pub sent_contacts: i64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DailySendCount {
    pub day: String,
    pub accepted: i64,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchCategory {
    #[default]
    All,
    Customer,
    Mail,
    Activity,
    File,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "record_type", rename_all = "snake_case")]
pub enum SearchTarget {
    Customer { id: String },
    Inbox { id: String },
    Draft { id: String },
    Activity { id: String },
    Document { id: String, contact_id: String },
    InboxAttachment { id: String, index: i64 },
    OutboundAttachment { id: String, index: i64 },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SearchHit {
    pub target: SearchTarget,
    pub category: SearchCategory,
    pub title: String,
    pub detail: String,
    pub preview: String,
    pub occurred_at: i64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SearchCounts {
    pub all: i64,
    pub customer: i64,
    pub mail: i64,
    pub activity: i64,
    pub file: i64,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunFilter {
    #[default]
    All,
    Active,
    Recorded,
    Attention,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MailJobAction {
    Cancel,
    Reopen,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AgentRunCounts {
    pub all: i64,
    pub active: i64,
    pub recorded: i64,
    pub attention: i64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "run_type", rename_all = "snake_case")]
pub enum AgentRunRecord {
    Mail {
        agent_id: String,
        agent_name: String,
        task_id: String,
        customers: i64,
        jobs: i64,
        active: i64,
        accepted: i64,
        failures: i64,
        unknown: i64,
        bounced: i64,
    },
    Request {
        #[serde(default)]
        agent_id: String,
        #[serde(default)]
        agent_name: String,
        operation: String,
        request_id: String,
        result: String,
        outcome: Option<String>,
        entity_id: Option<String>,
        #[serde(default)]
        success: Option<bool>,
        #[serde(default)]
        retryable: bool,
        #[serde(default)]
        requires_human: bool,
    },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AgentRun {
    pub record: AgentRunRecord,
    pub state: AgentRunFilter,
    pub occurred_at: i64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AgentMailJob {
    pub id: String,
    pub draft_id: String,
    pub contact_id: String,
    pub contact_name: String,
    pub subject: String,
    pub purpose: String,
    pub state: String,
    pub outcome: Option<String>,
    pub occurred_at: i64,
    #[serde(default)]
    pub can_cancel: bool,
    #[serde(default)]
    pub can_reopen: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MailAnalyticsTotals {
    pub accepted: i64,
    pub replied: i64,
    pub bounced: i64,
    pub waiting: i64,
    pub unsynced: i64,
    pub failed: i64,
    pub unknown: i64,
    pub latest_sync: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MailAnalyticsDay {
    pub day: String,
    pub accepted: i64,
    pub replied: i64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MailAnalyticsSource {
    pub source: Option<String>,
    pub accepted: i64,
    pub replied: i64,
    pub bounced: i64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MailAnalytics {
    pub days: u16,
    pub include_legacy: bool,
    pub totals: MailAnalyticsTotals,
    pub trend: Vec<MailAnalyticsDay>,
    pub sources: Vec<MailAnalyticsSource>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Result {
    MailAnalyticsCsv {
        csv: String,
    },
    MailJobUpdated {
        job_id: String,
        draft_id: String,
        draft_revision: i64,
        state: String,
    },
    CustomerCsv {
        csv: String,
        next_offset: Option<u32>,
    },
    DataOverview {
        customers: i64,
        mail_records: i64,
    },
    Dashboard {
        pending: i64,
        unmatched: i64,
        recent: Vec<RecentActivity>,
        issues: Vec<SummaryRow>,
        stats: DashboardStats,
        trend: Vec<DailySendCount>,
        agent: Vec<AuditRow>,
    },
    Search {
        counts: SearchCounts,
        rows: Vec<SearchHit>,
    },
    AgentRuns {
        counts: AgentRunCounts,
        rows: Vec<AgentRun>,
    },
    AgentRunMail {
        rows: Vec<AgentMailJob>,
        has_more: bool,
    },
    MailAnalytics {
        data: MailAnalytics,
    },
    Backups {
        directory: String,
        rows: Vec<Backup>,
    },
    BackupInspection {
        filename: String,
        schema_version: i64,
        customers: i64,
        mail_records: i64,
        objects: usize,
    },
    File {
        path: String,
    },
    Restored {
        safety_backup: String,
    },
    Audit {
        rows: Vec<AuditRow>,
        has_more: bool,
    },
    AuditWorkspace {
        rows: Vec<AuditEntry>,
        entities: Vec<String>,
        has_more: bool,
    },
    ImportReport {
        accepted: usize,
        rejected: usize,
        issues: Vec<ImportIssue>,
        applied: bool,
        #[serde(default)]
        mappings: Vec<ImportMapping>,
        #[serde(default)]
        unused_columns: usize,
    },
}
