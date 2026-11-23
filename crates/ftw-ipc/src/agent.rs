use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContactPatch {
    pub stage: Option<String>,
    pub customer_level: Option<String>,
    pub primary_language: Option<String>,
    pub main_need: Option<String>,
    pub sample_status: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    StateCatalog,
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
    AddActivity {
        request_id: String,
        input: crate::crm::ActivityInput,
    },
}
