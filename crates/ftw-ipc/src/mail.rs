use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Deserialize, Serialize)]
#[serde(transparent)]
pub struct Secret(pub String);

impl fmt::Debug for Secret {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str("<redacted>")
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Attachment {
    pub filename: String,
    pub content_base64: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DraftInput {
    pub contact_id: String,
    pub account_id: String,
    pub recipient: String,
    pub subject: String,
    pub body: String,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub replace_attachments: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    ListAccounts,
    SaveAccount {
        name: String,
        email: String,
        host: String,
        port: u16,
        security: String,
        username: String,
        password: Secret,
    },
    ReplacePassword {
        id: String,
        password: Secret,
    },
    TestAccount {
        id: String,
    },
    ListDrafts {
        contact_id: Option<String>,
        offset: u32,
    },
    GetDraft {
        id: String,
    },
    SaveDraft {
        id: Option<String>,
        input: DraftInput,
    },
    RequestApproval {
        id: String,
    },
    OpenApproval {
        id: String,
    },
    SendApproved {
        id: String,
    },
    SetSuppression {
        target: String,
        blocked: bool,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentCommand {
    ListCustomers {
        query: Box<crate::crm::ContactQuery>,
    },
    GetCustomer {
        id: String,
    },
    ListAccounts,
    ListDrafts {
        contact_id: Option<String>,
        offset: u32,
    },
    GetDraft {
        id: String,
    },
    CreateDraft {
        contact_id: String,
        account_id: String,
        recipient: String,
        subject: String,
        body: String,
    },
    RequestApproval {
        id: String,
    },
    SendApproved {
        id: String,
    },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DraftRow {
    pub id: String,
    pub contact_id: String,
    pub account_id: String,
    pub recipient: String,
    pub subject: String,
    pub state: String,
    pub revision: i64,
    pub source: String,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AttachmentInfo {
    pub filename: String,
    pub size: usize,
    pub sha256: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Draft {
    #[serde(flatten)]
    pub row: DraftRow,
    pub sender: String,
    pub body: String,
    pub attachments: Vec<AttachmentInfo>,
    pub expires_at: Option<i64>,
    pub outcome: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Result {
    Accounts {
        rows: Vec<Account>,
    },
    Drafts {
        rows: Vec<DraftRow>,
        has_more: bool,
    },
    Draft {
        record: Box<Draft>,
    },
    Saved {
        id: String,
    },
    ApprovalOpened {
        id: String,
    },
    Queued {
        id: String,
        job_id: String,
        state: String,
    },
    ConnectionVerified,
}
