use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    ReadCrm,
    WriteCrm,
    ReadMail,
    CreateDrafts,
    ManageFields,
    ArchiveContacts,
    MergeContacts,
    ImportCustomers,
    ExportCustomers,
    ManageViews,
    EditActivities,
    ReadDocuments,
    UploadDocuments,
    EditDrafts,
    ReadAudit,
    ReadTasks,
    ReadMailAttachments,
    SuggestReplyLinks,
    ManageMailTasks,
    ManageTemplates,
    EditLeads,
    ManageCompanies,
    UpdateCustomerBatches,
    ExportMailReports,
    ReviewLeads,
    ConfirmReplyLinks,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessArea {
    Crm,
    Mail,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Collaboration {
    Search {
        keyword: String,
        category: crate::data::SearchCategory,
        offset: u32,
    },
    CustomerQuality {
        category: crate::crm::QualityCategory,
        keyword: String,
        offset: u32,
    },
    PreviewCustomerBatch {
        query: Box<crate::crm::ContactQuery>,
    },
    UpdateCustomerBatch {
        request_id: String,
        change: Box<CustomerBatchChange>,
    },
    MailAnalytics {
        days: u16,
        include_legacy: bool,
        export: bool,
    },
    AuditWorkspace {
        source: String,
        entity: String,
        keyword: String,
        days: u16,
        offset: u32,
    },
    ReviewLead {
        request_id: String,
        id: String,
        expected: crate::crm::LeadVersion,
        decision: LeadDecision,
    },
    ReviewEvidence {
        request_id: String,
        id: String,
        expected: crate::crm::LeadVersion,
        decision: EvidenceDecision,
    },
    ConfirmReplyLink {
        request_id: String,
        id: String,
        contact_id: String,
        expected_contact: Option<String>,
        expected_match: String,
    },
    CreateReplyDraft {
        request_id: String,
        input: crate::mail::ReplyDraftInput,
    },
    CreateCustomerProfile {
        request_id: String,
        input: Box<crate::crm::ContactInput>,
        entries: Vec<crate::fields::Entry>,
    },
    ImportCustomerCsv {
        csv: String,
        apply: bool,
        default_stage: String,
        request_id: Option<String>,
    },
    ExportCustomerCsv {
        scope: crate::crm::ArchiveFilter,
        fields: Vec<String>,
        offset: u32,
    },
    SetContactArchive {
        target: crate::views::CellTarget,
        archived: bool,
    },
    MergeContacts {
        primary: crate::views::CellTarget,
        secondary: crate::views::CellTarget,
    },
    SaveField {
        input: crate::fields::Definition,
    },
    ArchivedFields,
    SetFieldArchive {
        request_id: String,
        id: String,
        expected_revision: i64,
        archived: bool,
    },
    MoveField {
        request_id: String,
        id: String,
        expected_revision: i64,
        direction: i8,
    },
    SaveView {
        input: crate::views::ViewInput,
    },
    ArchiveView {
        id: String,
        expected_revision: i64,
    },
    RestoreView {
        id: String,
        expected_revision: i64,
    },
    MoveView {
        request_id: String,
        id: String,
        expected_revision: i64,
        direction: i8,
    },
    ManageCompany {
        request_id: String,
        change: Box<CompanyChange>,
    },
    ArchivedViews,
    ArchivedTemplates {
        purpose: String,
        offset: u32,
    },
    ManageTemplate {
        request_id: String,
        change: crate::mail::TemplateChange,
    },
    ListDocuments {
        contact_id: String,
        offset: u32,
    },
    UploadDocument {
        contact_id: String,
        filename: String,
        content_base64: String,
    },
    UploadDocumentChunk {
        input: DocumentUploadChunk,
    },
    ReadMailAttachment {
        source: MailAttachmentSource,
        id: String,
        index: u16,
        offset: u32,
    },
    ReviewReplies {
        offset: u32,
    },
    ReplyCandidates {
        id: String,
    },
    SuggestReplyLink {
        id: String,
        contact_id: String,
    },
    ListRuns {
        filter: crate::data::AgentRunFilter,
        keyword: String,
        offset: u32,
    },
    UpdateMailJob {
        job_id: String,
        job_action: crate::data::MailJobAction,
    },
    ListSuppressions {
        offset: u32,
    },
    ReadDocument {
        id: String,
        offset: u32,
    },
    EditDraft {
        id: String,
        expected_revision: i64,
        input: crate::mail::DraftInput,
    },
    EditActivity {
        id: String,
        expected_revision: i64,
        input: crate::crm::ActivityInput,
    },
    EditLead {
        request_id: String,
        id: String,
        expected: crate::crm::LeadVersion,
        patch: Box<crate::crm::LeadPatch>,
    },
    MailTask {
        task_id: String,
        offset: u32,
    },
    CustomerHistory {
        contact_id: String,
        offset: u32,
    },
    CustomFieldHistory {
        contact_id: String,
        offset: u32,
    },
}

impl Collaboration {
    pub fn scope(&self) -> Option<Scope> {
        Some(match self {
            Self::Search { .. } => return None,
            Self::CustomerQuality { .. } => Scope::ReadCrm,
            Self::PreviewCustomerBatch { .. } | Self::UpdateCustomerBatch { .. } => {
                Scope::UpdateCustomerBatches
            }
            Self::MailAnalytics { export, .. } => {
                if *export {
                    Scope::ExportMailReports
                } else {
                    Scope::ReadMail
                }
            }
            Self::AuditWorkspace { .. } => Scope::ReadAudit,
            Self::ReviewLead { .. } | Self::ReviewEvidence { .. } => Scope::ReviewLeads,
            Self::ConfirmReplyLink { .. } => Scope::ConfirmReplyLinks,
            Self::CreateReplyDraft { .. } => Scope::CreateDrafts,
            Self::CreateCustomerProfile { .. } => Scope::WriteCrm,
            Self::ImportCustomerCsv { .. } => Scope::ImportCustomers,
            Self::ExportCustomerCsv { .. } => Scope::ExportCustomers,
            Self::SetContactArchive { .. } => Scope::ArchiveContacts,
            Self::MergeContacts { .. } => Scope::MergeContacts,
            Self::SaveField { .. }
            | Self::ArchivedFields
            | Self::SetFieldArchive { .. }
            | Self::MoveField { .. } => Scope::ManageFields,
            Self::SaveView { .. }
            | Self::ArchiveView { .. }
            | Self::RestoreView { .. }
            | Self::MoveView { .. }
            | Self::ArchivedViews => Scope::ManageViews,
            Self::ArchivedTemplates { .. } | Self::ManageTemplate { .. } => Scope::ManageTemplates,
            Self::EditActivity { .. } => Scope::EditActivities,
            Self::EditLead { .. } => Scope::EditLeads,
            Self::ManageCompany { .. } => Scope::ManageCompanies,
            Self::ListDocuments { .. } | Self::ReadDocument { .. } => Scope::ReadDocuments,
            Self::UploadDocument { .. } | Self::UploadDocumentChunk { .. } => {
                Scope::UploadDocuments
            }
            Self::EditDraft { .. } => Scope::EditDrafts,
            Self::CustomerHistory { .. } | Self::CustomFieldHistory { .. } => Scope::ReadAudit,
            Self::MailTask { .. } | Self::ListRuns { .. } => Scope::ReadTasks,
            Self::UpdateMailJob { .. } => Scope::ManageMailTasks,
            Self::ListSuppressions { .. } => Scope::ReadMail,
            Self::ReadMailAttachment { .. } => Scope::ReadMailAttachments,
            Self::ReviewReplies { .. }
            | Self::ReplyCandidates { .. }
            | Self::SuggestReplyLink { .. } => Scope::SuggestReplyLinks,
        })
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "selection", rename_all = "snake_case", deny_unknown_fields)]
pub enum CustomerBatchChange {
    Matching {
        query: crate::crm::ContactQuery,
        fingerprint: String,
        field: BatchField,
        value: String,
    },
    Selected {
        targets: Vec<crate::views::CellTarget>,
        field: BatchField,
        value: String,
    },
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MoveDirection {
    Up,
    Down,
}

impl MoveDirection {
    pub fn step(&self) -> i8 {
        match self {
            Self::Up => -1,
            Self::Down => 1,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BatchField {
    Stage,
    Level,
    Sample,
}

impl BatchField {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stage => "stage",
            Self::Level => "level",
            Self::Sample => "sample",
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LeadDecision {
    Reject,
    Reopen,
    Promote,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceDecision {
    Verified,
    Rejected,
    Pending,
}

impl EvidenceDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Rejected => "rejected",
            Self::Pending => "pending",
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum CompanyChange {
    SaveProfile {
        anchor: crate::views::CellTarget,
        profile: Option<crate::views::CellTarget>,
        input: crate::crm::CompanyProfileInput,
        include_group: bool,
    },
    SetMembership {
        contact: crate::views::CellTarget,
        profile: Option<crate::views::CellTarget>,
        branch: String,
    },
    Associate {
        anchor: crate::views::CellTarget,
        contact: crate::views::CellTarget,
    },
    CreateContact {
        profile: crate::views::CellTarget,
        input: Box<crate::crm::ContactInput>,
        entries: Vec<crate::fields::Entry>,
        allow_duplicate: bool,
    },
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentUploadChunk {
    pub upload_id: String,
    pub contact_id: String,
    pub filename: String,
    pub total_size: u32,
    pub offset: u32,
    pub content_base64: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MailAttachmentSource {
    Inbox,
    Outbound,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Research,
    Inferred,
    Unverified,
}

impl EvidenceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Research => "research",
            Self::Inferred => "inferred",
            Self::Unverified => "unverified",
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub kind: EvidenceKind,
    pub url: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContactDetailSuggestion {
    ContactMethod {
        kind: String,
        value: String,
        #[serde(default)]
        label: String,
    },
    ProductInterest {
        product_text: String,
        intent_quantity: Option<i64>,
        #[serde(default)]
        quantity_is_estimate: bool,
        #[serde(default)]
        requirement: String,
    },
    CorrectContactMethod {
        method_id: String,
        value: String,
        label: Option<String>,
    },
    CorrectProductInterest {
        interest_id: String,
        intent_quantity: Option<i64>,
        quantity_is_estimate: Option<bool>,
        requirement: Option<String>,
    },
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContactPatch {
    pub stage: Option<String>,
    pub customer_level: Option<String>,
    pub primary_language: Option<String>,
    pub main_need: Option<String>,
    pub sample_status: Option<String>,
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Evidence>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Views,
    GetCustomer {
        id: String,
    },
    QueryView {
        id: String,
        offset: u32,
    },
    QueryCustomers {
        query: crate::crm::ContactQuery,
    },
    StateCatalog,
    DuplicateCustomers {
        keyword: String,
        offset: u32,
    },
    SampleWorkspace {
        keyword: String,
        status: String,
        offset: u32,
        by_batch: bool,
    },
    Fields,
    FieldValues {
        contact_id: String,
    },
    PatchFieldValues {
        request_id: String,
        patch: crate::fields::ValuePatch,
    },
    Timeline {
        id: String,
        offset: u32,
        event_type: String,
    },
    Leads {
        status: String,
        offset: u32,
    },
    CreateLead {
        request_id: String,
        input: Box<crate::crm::LeadInput>,
    },
    ListLeadEvidence {
        lead_id: String,
        offset: u32,
    },
    GetLead {
        id: String,
    },
    ListContactEvidence {
        contact_id: String,
        offset: u32,
    },
    GetCustomerCompany {
        contact_id: String,
        offset: u32,
    },
    FindCompanyProfiles {
        keyword: String,
    },
    AddContactEvidence {
        request_id: String,
        contact_id: String,
        input: crate::crm::LeadEvidenceInput,
    },
    AddLeadEvidence {
        request_id: String,
        lead_id: String,
        input: crate::crm::LeadEvidenceInput,
    },
    CreateCustomer {
        request_id: String,
        input: Box<crate::crm::ContactInput>,
    },
    UpdateCustomer {
        request_id: String,
        id: String,
        expected_revision: Option<i64>,
        expected_epoch: Option<String>,
        patch: Box<ContactPatch>,
    },
    SuggestCustomerDetail {
        request_id: String,
        id: String,
        expected_revision: i64,
        expected_epoch: String,
        detail: ContactDetailSuggestion,
        evidence: Evidence,
    },
    AddActivity {
        request_id: String,
        input: crate::crm::ActivityInput,
    },
}

impl Command {
    pub fn scope(&self) -> Scope {
        match self {
            Self::PatchFieldValues { .. }
            | Self::CreateLead { .. }
            | Self::AddContactEvidence { .. }
            | Self::AddLeadEvidence { .. }
            | Self::CreateCustomer { .. }
            | Self::UpdateCustomer { .. }
            | Self::SuggestCustomerDetail { .. }
            | Self::AddActivity { .. } => Scope::WriteCrm,
            _ => Scope::ReadCrm,
        }
    }
}
