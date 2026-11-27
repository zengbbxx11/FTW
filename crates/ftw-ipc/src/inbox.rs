use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HandlingTarget {
    pub id: String,
    pub expected_revision: i64,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InboxFilter {
    #[default]
    All,
    Review,
    Unassociated,
    NonCustomer,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    DownloadFull {
        id: String,
    },
    ReadBody {
        id: String,
        offset: u32,
    },
    SaveAttachment {
        id: String,
        index: u16,
        destination: String,
    },
    Accounts,
    ReplyUpdates {
        after: Option<i64>,
    },
    Configure {
        account_id: String,
        host: String,
        port: u16,
        username: String,
        password: crate::mail::Secret,
    },
    Test {
        account_id: String,
    },
    Sync {
        account_id: String,
    },
    List {
        account_id: Option<String>,
        contact_id: Option<String>,
        unmatched: bool,
        offset: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keyword: Option<String>,
        #[serde(default)]
        filter: InboxFilter,
    },
    Classify {
        targets: Vec<HandlingTarget>,
        non_customer: bool,
    },
    Get {
        id: String,
    },
    ReviewCandidates {
        id: String,
    },
    ReviewLink {
        id: String,
        contact_id: String,
        expected_contact: Option<String>,
        expected_match: String,
        expected_handling_revision: Option<i64>,
    },
    Thread {
        id: String,
        offset: u32,
    },
    Link {
        id: String,
        contact_id: String,
        expected_handling_revision: Option<i64>,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentCommand {
    SyncStatus {
        account_id: Option<String>,
    },
    DownloadFull {
        id: String,
    },
    ReadBody {
        id: String,
        offset: u32,
    },
    Sync {
        account_id: String,
    },
    List {
        contact_id: Option<String>,
        offset: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keyword: Option<String>,
        #[serde(default)]
        filter: InboxFilter,
    },
    Thread {
        id: String,
        offset: u32,
    },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Account {
    pub account_id: String,
    pub display_name: String,
    pub email_address: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub last_sync: Option<i64>,
    pub outcome: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Row {
    pub id: String,
    pub account_id: String,
    pub contact_id: Option<String>,
    pub contact_name: Option<String>,
    pub thread_id: String,
    pub sender: String,
    pub subject: String,
    pub received_at: i64,
    pub kind: String,
    pub received: bool,
    #[serde(default)]
    pub message_role: String,
    #[serde(default)]
    pub match_state: String,
    #[serde(default)]
    pub body_complete: bool,
    #[serde(default)]
    pub download_outcome: Option<String>,
    #[serde(default)]
    pub non_customer: bool,
    #[serde(default)]
    pub handling_revision: i64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ReviewCandidate {
    pub id: String,
    pub name: String,
    pub company: String,
    pub country: String,
    pub exact_sender: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_by: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Message {
    #[serde(flatten)]
    pub row: Row,
    pub body: String,
    pub attachments: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Result {
    ReplyUpdates {
        cursor: i64,
        count: i64,
    },
    SyncStatus {
        rows: Vec<SyncStatus>,
    },
    Body {
        text: String,
        next_offset: Option<u32>,
    },
    Accounts {
        rows: Vec<Account>,
    },
    Rows {
        rows: Vec<Row>,
        has_more: bool,
    },
    ReviewCandidates {
        rows: Vec<ReviewCandidate>,
    },
    Message {
        record: Box<Message>,
    },
    Thread {
        messages: Vec<Message>,
        has_more: bool,
    },
    Saved {
        id: String,
    },
    SyncStarted {
        account_id: String,
    },
    ConnectionVerified,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SyncStatus {
    pub account_id: String,
    pub configured: bool,
    pub last_sync: Option<i64>,
    pub outcome: Option<String>,
    pub complete: bool,
}
