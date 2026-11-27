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

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
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

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
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

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LeadVersion {
    pub revision: i64,
    pub epoch: String,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct LeadPatch {
    pub display_name: Option<String>,
    pub company_name: Option<String>,
    pub country_region: Option<String>,
    pub email: Option<String>,
    pub website_url: Option<String>,
    pub business_summary: Option<String>,
    pub research_notes: Option<String>,
    pub confidence: Option<u8>,
    pub add_source_urls: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LeadEvidenceInput {
    pub subject: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub note: String,
    pub observed_at: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct LeadEvidence {
    pub id: String,
    pub subject: String,
    pub url: String,
    pub note: String,
    pub source: String,
    pub request_id: Option<String>,
    pub observed_at: Option<i64>,
    pub created_at: i64,
    pub status: String,
    pub reviewed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_source: Option<String>,
    pub revision: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample: Option<SampleActivity>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct SampleActivity {
    pub model: String,
    pub quantity: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<String>,
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
    PreviewBulkContacts {
        query: ContactQuery,
    },
    UpdateMatchingContacts {
        query: ContactQuery,
        fingerprint: String,
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
    CustomerQuality {
        category: String,
        #[serde(default)]
        keyword: String,
        offset: u32,
    },
    CompanyContacts {
        id: String,
        offset: u32,
    },
    AssociateCompany {
        anchor: crate::views::CellTarget,
        contact: crate::views::CellTarget,
    },
    FindCompanyProfiles {
        keyword: String,
    },
    SaveCompanyProfile {
        anchor: crate::views::CellTarget,
        profile: Option<crate::views::CellTarget>,
        input: CompanyProfileInput,
        #[serde(default)]
        include_group: bool,
    },
    SetCompanyMembership {
        contact: crate::views::CellTarget,
        profile: Option<crate::views::CellTarget>,
        branch: String,
    },
    MergeContacts {
        primary: crate::views::CellTarget,
        secondary: crate::views::CellTarget,
    },
    ListAgentChanges {
        offset: u32,
        pending: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
    },
    ResolveAgentChange {
        id: String,
        accept: bool,
    },
    SampleWorkspace {
        keyword: String,
        status: String,
        offset: u32,
        #[serde(default)]
        by_batch: bool,
    },
    ListContacts {
        query: ContactQuery,
    },
    PreviewContacts {
        query: ContactQuery,
    },
    GetContact {
        id: String,
    },
    DevelopmentOverview {
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        company: Option<crate::views::CellTarget>,
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
    SetContactArchive {
        target: crate::views::CellTarget,
        archived: bool,
    },
    ListLeads {
        offset: u32,
        review_status: String,
    },
    LeadWorkspace {
        offset: u32,
        review_status: String,
        keyword: String,
        source: String,
    },
    GetLead {
        id: String,
    },
    ListLeadEvidence {
        lead_id: String,
        offset: u32,
    },
    ListContactEvidence {
        contact_id: String,
        offset: u32,
    },
    AddContactEvidence {
        contact_id: String,
        input: LeadEvidenceInput,
    },
    AddLeadEvidence {
        lead_id: String,
        input: LeadEvidenceInput,
    },
    ReviewLeadEvidence {
        id: String,
        revision: i64,
        epoch: String,
        status: String,
    },
    SaveLead {
        id: Option<String>,
        input: Box<LeadInput>,
        allow_duplicate: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected: Option<LeadVersion>,
    },
    PatchLead {
        id: String,
        expected: LeadVersion,
        patch: Box<LeadPatch>,
    },
    SetLeadReview {
        id: String,
        rejected: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected: Option<LeadVersion>,
    },
    PromoteLead {
        id: String,
        allow_duplicate: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected: Option<LeadVersion>,
    },
    ListActivities {
        contact_id: Option<String>,
        event_type: String,
        offset: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        keyword: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
    GetActivity {
        id: String,
    },
    SaveActivity {
        id: Option<String>,
        input: ActivityInput,
    },
    UpdateActivity {
        id: String,
        expected_revision: i64,
        input: ActivityInput,
    },
    ContactHistory {
        id: String,
        offset: u32,
    },
    CustomFieldHistory {
        id: String,
        offset: u32,
    },
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Result {
    CustomFieldHistory {
        rows: Vec<CustomFieldChange>,
        has_more: bool,
    },
    ContactHistory {
        rows: Vec<ContactRevision>,
        has_more: bool,
    },
    CellsUpdated {
        count: usize,
    },
    BulkContacts {
        count: usize,
        fingerprint: String,
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
    AgentReviewPending {
        id: String,
        count: usize,
    },
    AgentChanges {
        rows: Vec<AgentChange>,
        has_more: bool,
        total: i64,
        pending: i64,
        applied: i64,
        conflicts: i64,
    },
    CustomerQuality {
        rows: Vec<QualityIssue>,
        counts: Vec<(String, i64)>,
        active_customers: i64,
        has_more: bool,
    },
    CompanyContacts {
        rows: Vec<CompanyContact>,
        total: i64,
        contacted: i64,
        has_more: bool,
        profile: Option<CompanyProfile>,
        branch: String,
    },
    CompanyProfiles {
        rows: Vec<CompanyProfile>,
        has_more: bool,
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
    ContactPreview {
        rows: Vec<ContactRow>,
        total: i64,
    },
    Contact {
        record: Box<Contact>,
    },
    DevelopmentOverview {
        record: Box<DevelopmentOverview>,
    },
    Leads {
        rows: Vec<LeadRow>,
        has_more: bool,
    },
    LeadWorkspace {
        rows: Vec<LeadWorkspaceRow>,
        has_more: bool,
    },
    Lead {
        record: Box<Lead>,
        duplicates: Vec<Duplicate>,
    },
    LeadUpdated {
        id: String,
        revision: i64,
    },
    LeadEvidence {
        rows: Vec<LeadEvidence>,
        total: i64,
        verified: i64,
        pending: i64,
        has_more: bool,
        epoch: String,
    },
    Activities {
        rows: Vec<Activity>,
        has_more: bool,
    },
    SampleWorkspace {
        rows: Vec<SampleRow>,
        counts: Vec<(String, i64)>,
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
pub struct DevelopmentOverview {
    pub id: String,
    pub record_revision: i64,
    pub data_epoch: String,
    pub display_name: String,
    pub company_name: String,
    pub country_region: String,
    pub stage: String,
    pub customer_level: String,
    pub sample_status: String,
    pub main_need: String,
    pub main_need_truncated: bool,
    pub product_interests: Vec<String>,
    pub interests_has_more: bool,
    pub email: Option<String>,
    pub mail_outreach: MailOutreach,
    pub activities: Vec<DevelopmentActivity>,
    pub related_contacts: Vec<DevelopmentRelated>,
    pub related_has_more: bool,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DevelopmentActivity {
    pub id: String,
    pub event_type: String,
    pub summary: String,
    pub occurred_at: i64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DevelopmentRelated {
    pub id: String,
    pub display_name: String,
    pub company_name: String,
    pub reason: String,
    pub last_contact_at: Option<i64>,
    pub mail_status: String,
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
    #[serde(default)]
    pub merged_into: Option<String>,
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
pub struct LeadWorkspaceRow {
    #[serde(flatten)]
    pub lead: LeadRow,
    pub website_url: String,
    pub business_summary: String,
    pub evidence_count: u32,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct Lead {
    pub id: String,
    pub record_revision: i64,
    pub data_epoch: String,
    pub input: LeadInput,
    pub review_status: String,
    pub promoted_contact_id: Option<String>,
    pub source: String,
    pub created_at: i64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct Activity {
    pub counts_as_contact: Option<bool>,
    pub id: String,
    pub revision: i64,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample: Option<SampleActivity>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ContactRevision {
    pub revision: i64,
    pub actor_id: String,
    pub actor_name: String,
    pub source: String,
    pub occurred_at: i64,
    pub fields: Vec<ContactRevisionField>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ContactRevisionField {
    pub field: String,
    pub before: serde_json::Value,
    pub after: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CustomFieldChange {
    pub revision: i64,
    pub field_id: String,
    pub field_name: String,
    pub actor_id: String,
    pub actor_name: String,
    pub source: String,
    pub task_id: Option<String>,
    pub before: Option<crate::fields::Value>,
    pub after: Option<crate::fields::Value>,
    pub occurred_at: i64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SampleRow {
    pub id: String,
    pub contact_id: String,
    pub contact_name: String,
    pub company_name: String,
    pub country_region: String,
    pub status: String,
    pub model: String,
    pub quantity: Option<u32>,
    pub summary: String,
    pub details: String,
    pub feedback: String,
    pub occurred_at: Option<i64>,
    pub batch_ref: String,
    pub events: Option<i64>,
    pub activity_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct AgentChange {
    pub id: String,
    pub contact_id: String,
    pub contact_name: String,
    pub field: String,
    pub previous_value: String,
    pub proposed_value: String,
    pub current_value: String,
    pub request_id: String,
    pub state: String,
    pub created_at: i64,
    pub conflict: bool,
    pub archived: bool,
    pub evidence_kind: Option<String>,
    pub evidence_url: Option<String>,
    pub evidence_note: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct QualityIssue {
    pub category: String,
    pub contact_id: String,
    pub contact_name: String,
    pub other_id: Option<String>,
    pub other_name: Option<String>,
    pub detail: String,
    pub draft_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QualityCategory {
    #[default]
    All,
    Duplicate,
    MissingEmail,
    InvalidEmail,
    MailProblem,
    MissingMarket,
    InvalidWebsite,
    DateOrder,
    SampleMismatch,
    ExcludedMail,
}

impl QualityCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::All => "",
            Self::Duplicate => "duplicate",
            Self::MissingEmail => "missing_email",
            Self::InvalidEmail => "invalid_email",
            Self::MailProblem => "mail_problem",
            Self::MissingMarket => "missing_market",
            Self::InvalidWebsite => "invalid_website",
            Self::DateOrder => "date_order",
            Self::SampleMismatch => "sample_mismatch",
            Self::ExcludedMail => "excluded_mail",
        }
    }

    pub fn mail_related(&self) -> bool {
        matches!(self, Self::MailProblem | Self::ExcludedMail)
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CompanyContact {
    pub id: String,
    pub display_name: String,
    pub email: Option<String>,
    pub stage: String,
    pub mail_status: String,
    pub last_contact_at: Option<i64>,
    pub country_region: String,
    pub branch: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompanyProfileInput {
    pub name: String,
    pub website: String,
    pub notes: String,
    pub aliases: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CompanyProfile {
    #[serde(flatten)]
    pub input: CompanyProfileInput,
    pub id: String,
    pub revision: i64,
    pub epoch: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct Duplicate {
    pub id: String,
    pub display_name: String,
    pub entity: String,
    pub reason: String,
}
