use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Accounts,
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
    },
    Get {
        id: String,
    },
    Thread {
        id: String,
        offset: u32,
    },
    Link {
        id: String,
        contact_id: String,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentCommand {
    Sync {
        account_id: String,
    },
    List {
        contact_id: Option<String>,
        offset: u32,
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
    Accounts {
        rows: Vec<Account>,
    },
    Rows {
        rows: Vec<Row>,
        has_more: bool,
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
