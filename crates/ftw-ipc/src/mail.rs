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
#[serde(deny_unknown_fields)]
pub struct AgentMailPolicy {
    pub revision: i64,
    pub mode: String,
    pub accounts: Vec<String>,
    pub purposes: Vec<String>,
    pub daily_limit: u16,
    pub task_limit: u16,
    pub interval_seconds: u16,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RegisteredAgent {
    pub id: String,
    pub name: String,
    pub revoked: bool,
    #[serde(default)]
    pub automatic: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PolicyReview {
    pub id: String,
    pub state: String,
    pub outcome: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    AgentMailSettings,
    ImportAssets {
        paths: Vec<String>,
    },
    ListAssets,
    SetAssetAccess {
        id: String,
        allowed: bool,
    },
    SaveDraftWithAssets {
        id: Option<String>,
        input: DraftInput,
        asset_ids: Vec<String>,
    },
    SetAgentMailPolicy {
        policy: AgentMailPolicy,
        #[serde(default)]
        agent_ids: Option<Vec<String>>,
    },
    CancelMailPolicyReview {
        id: String,
    },
    RegisterMailAgent {
        name: String,
        token: Secret,
    },
    RevokeMailAgent {
        id: String,
    },
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
    SetAccountPreferences {
        id: String,
        daily_limit: u16,
        signature: String,
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
    ListAssets {
        token: Secret,
    },
    AttachDraftAssets {
        token: Secret,
        id: String,
        revision: i64,
        asset_ids: Vec<String>,
    },
    CheckOutreach {
        recipient: String,
    },
    SendAuthorized {
        token: Secret,
        id: String,
        revision: i64,
        purpose: String,
        task_id: String,
        request_id: String,
    },
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
    ListPending {
        offset: u32,
    },
    CreateDraft {
        #[serde(default)]
        request_id: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daily_limit: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DraftRow {
    #[serde(default)]
    pub approval_expired: bool,
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub job_id: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub agent_name: Option<String>,
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
    #[serde(default)]
    pub asset_id: Option<String>,
    pub filename: String,
    pub size: usize,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Asset {
    pub id: String,
    pub filename: String,
    pub sha256: String,
    pub size: i64,
    pub content_type: String,
    pub agent_allowed: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ApprovalAttempt {
    pub id: String,
    pub revision: i64,
    pub state: String,
    pub outcome: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Draft {
    pub approval_attempt: Option<ApprovalAttempt>,
    #[serde(flatten)]
    pub row: DraftRow,
    pub sender: String,
    pub body: String,
    pub attachments: Vec<AttachmentInfo>,
    pub expires_at: Option<i64>,
    pub thread_id: Option<String>,
    pub has_reply: bool,
    pub outcome: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Result {
    Assets {
        rows: Vec<Asset>,
    },
    OutreachCheck {
        allowed_for_first_outreach: bool,
        outcome: Option<String>,
        contact_ids: Vec<String>,
        same_domain_contact_ids: Vec<String>,
    },
    AgentMailSettings {
        policy: AgentMailPolicy,
        agents: Vec<RegisteredAgent>,
        review: Option<PolicyReview>,
        automatic_available: bool,
    },
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
        #[serde(default)]
        attempt_id: String,
    },
    Queued {
        id: String,
        job_id: String,
        state: String,
    },
    ConnectionVerified,
}
