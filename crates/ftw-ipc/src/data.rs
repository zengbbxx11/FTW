use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Dashboard,
    Backups,
    CreateBackup,
    Restore {
        filename: String,
    },
    Audit {
        offset: u32,
    },
    ImportContacts {
        csv: String,
        apply: bool,
        default_stage: String,
    },
    ExportContacts,
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
    pub source: String,
    pub entity: String,
    pub entity_id: String,
    pub operation: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ImportIssue {
    pub row: usize,
    pub reason: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SummaryRow {
    pub title: String,
    pub detail: String,
    pub occurred_at: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Result {
    Dashboard {
        pending: i64,
        unmatched: i64,
        recent: Vec<SummaryRow>,
        issues: Vec<SummaryRow>,
    },
    Backups {
        directory: String,
        rows: Vec<Backup>,
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
    ImportReport {
        accepted: usize,
        rejected: usize,
        issues: Vec<ImportIssue>,
        applied: bool,
    },
}
