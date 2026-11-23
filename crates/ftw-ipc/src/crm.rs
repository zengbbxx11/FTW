use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Manual,
    Agent,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Agent => "agent",
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ContactInput {
    pub display_name: String,
    pub company_name: String,
    pub country_region: String,
    pub customer_type: String,
    pub customer_source: String,
    pub customer_level: String,
    pub stage: String,
    pub first_contact_channel: String,
    pub primary_language: String,
    pub main_need: String,
    pub sample_status: String,
    pub bulk_order_status: String,
    pub first_contact_at: Option<i64>,
    pub last_contact_at: Option<i64>,
    pub website_url: String,
    pub alibaba_buyer_id: String,
    pub notes: String,
    pub methods: Vec<Method>,
    pub interests: Vec<Interest>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Method {
    #[serde(default)]
    pub id: Option<String>,
    pub kind: String,
    pub value: String,
    pub label: String,
    pub is_primary: bool,
    pub is_verified: bool,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Interest {
    #[serde(default)]
    pub id: Option<String>,
    pub product_text: String,
    pub intent_quantity: Option<i64>,
    pub quantity_is_estimate: bool,
    pub requirement: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct LeadInput {
    pub display_name: String,
    pub company_name: String,
    pub country_region: String,
    pub email: String,
    pub website_url: String,
    pub business_summary: String,
    pub source_urls: Vec<String>,
    pub research_notes: String,
    pub confidence: Option<u8>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[derive(Default)]
pub struct ActivityInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counts_as_contact: Option<bool>,
    pub contact_id: String,
    pub event_type: String,
    pub occurred_at: i64,
    pub summary: String,
    pub details: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub followup_category: String,
    #[serde(default)]
    pub customer_feedback: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ContactQuery {
    pub filter: Option<crate::views::Filter>,
    pub order: Vec<crate::views::Order>,
    pub custom_columns: Vec<String>,
    pub page_size: Option<u32>,
    pub group: Option<(String, Option<String>)>,
    pub keyword: String,
    pub quick_view: String,
    pub country: String,
    pub stage: String,
    pub level: String,
    pub sample_status: String,
    pub customer_type: String,
    pub source: String,
    pub archive: ArchiveFilter,
    pub sort: ContactSort,
    pub offset: u32,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFilter {
    #[default]
    Active,
    Archived,
    All,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContactSort {
    #[default]
    Updated,
    Name,
    LastContact,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    UpdateCells {
        targets: Vec<crate::views::CellTarget>,
        field: String,
        value: String,
    },
    ListViews {
        #[serde(default)]
        include_archived: bool,
    },
    SaveView {
        view: crate::views::ViewInput,
    },
    ArchiveView {
        id: String,
        expected_revision: i64,
        archived: bool,
    },
    MoveView {
        id: String,
        expected_revision: i64,
        direction: i8,
    },
    CustomerStats,
    ListContacts {
        query: ContactQuery,
    },
    GetContact {
        id: String,
    },
    StateCatalog,
    ListFields {
        #[serde(default)]
        include_archived: bool,
    },
    SaveField {
        definition: crate::fields::Definition,
    },
    ArchiveField {
        id: String,
        expected_revision: i64,
        archived: bool,
    },
    MoveField {
        id: String,
        expected_revision: i64,
        direction: i8,
    },
    GetFieldValues {
        contact_id: String,
    },
    PatchFieldValues {
        patch: crate::fields::ValuePatch,
    },
    CreateContactWithFields {
        input: Box<ContactInput>,
        allow_duplicate: bool,
        entries: Vec<crate::fields::Entry>,
    },
    SaveContactChecked {
        id: String,
        input: Box<ContactInput>,
        allow_duplicate: bool,
        expected_revision: i64,
        expected_epoch: Option<String>,
    },
    SaveContact {
        id: Option<String>,
        input: Box<ContactInput>,
        allow_duplicate: bool,
    },
    SetArchived {
        id: String,
        archived: bool,
    },
    ListLeads {
        offset: u32,
        review_status: String,
    },
    GetLead {
        id: String,
    },
    SaveLead {
        id: Option<String>,
        input: Box<LeadInput>,
        allow_duplicate: bool,
    },
    SetLeadReview {
        id: String,
        rejected: bool,
    },
    PromoteLead {
        id: String,
        allow_duplicate: bool,
    },
    ListActivities {
        contact_id: Option<String>,
        event_type: String,
        offset: u32,
    },
    SaveActivity {
        id: Option<String>,
        input: ActivityInput,
    },
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Result {
    CellsUpdated {
        count: usize,
    },
    Views {
        views: Vec<crate::views::SavedView>,
    },
    Fields {
        fields: Vec<crate::fields::Field>,
    },
    FieldValues {
        record: crate::fields::Values,
    },
    StateCatalog {
        options: Vec<StateOption>,
    },
    CustomerStats {
        active: i64,
        priority: i64,
        sample: i64,
        in_progress: i64,
        won: i64,
    },
    Contacts {
        rows: Vec<ContactRow>,
        has_more: bool,
    },
    Contact {
        record: Box<Contact>,
    },
    Leads {
        rows: Vec<LeadRow>,
        has_more: bool,
    },
    Lead {
        record: Box<Lead>,
        duplicates: Vec<Duplicate>,
    },
    Activities {
        rows: Vec<Activity>,
        has_more: bool,
    },
    Saved {
        id: String,
    },
    Duplicates {
        matches: Vec<Duplicate>,
    },
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct StateOption {
    pub field: String,
    pub code: String,
    pub label: String,
    pub aliases: Vec<String>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct MailOutreach {
    pub status: String,
    pub last_sent_at: Option<i64>,
    pub last_reply_at: Option<i64>,
    pub outreach_count: i64,
    pub recent_subject: Option<String>,
    pub thread_id: Option<String>,
    pub sync_outcome: Option<String>,
    pub sync_at: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ContactRow {
    #[serde(default)]
    pub mail_outreach: MailOutreach,
    #[serde(default)]
    pub custom_values: Vec<crate::fields::Entry>,
    #[serde(default)]
    pub record_revision: i64,
    #[serde(default)]
    pub data_epoch: String,
    #[serde(default)]
    pub primary_interest: String,
    pub id: String,
    pub display_name: String,
    pub company_name: String,
    pub country_region: String,
    pub stage: String,
    pub customer_level: String,
    pub sample_status: String,
    pub email: Option<String>,
    pub last_contact_at: Option<i64>,
    pub archived_at: Option<i64>,
    pub source: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct Contact {
    #[serde(default)]
    pub mail_outreach: MailOutreach,
    pub id: String,
    #[serde(default)]
    pub record_revision: i64,
    #[serde(default)]
    pub data_epoch: String,
    #[serde(default)]
    pub effective_first_contact_at: Option<i64>,
    #[serde(default)]
    pub effective_last_contact_at: Option<i64>,
    #[serde(default)]
    pub updated_at: i64,
    pub latest_thread_reply_at: Option<i64>,
    pub input: ContactInput,
    pub archived_at: Option<i64>,
    pub source: String,
    pub observed_first_contact_at: Option<i64>,
    pub observed_last_contact_at: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct LeadRow {
    pub id: String,
    pub display_name: String,
    pub company_name: String,
    pub country_region: String,
    pub email: String,
    pub review_status: String,
    pub source: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct Lead {
    pub id: String,
    pub input: LeadInput,
    pub review_status: String,
    pub promoted_contact_id: Option<String>,
    pub source: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct Activity {
    pub counts_as_contact: Option<bool>,
    pub id: String,
    pub contact_id: String,
    pub contact_name: String,
    pub event_type: String,
    pub occurred_at: i64,
    pub summary: String,
    pub details: String,
    pub channel: String,
    pub followup_category: String,
    pub customer_feedback: String,
    pub source: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct Duplicate {
    pub id: String,
    pub display_name: String,
    pub entity: String,
    pub reason: String,
}
