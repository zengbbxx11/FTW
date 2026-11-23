use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Text,
    Number,
    Date,
    SingleSelect,
    MultiSelect,
    Checkbox,
    Url,
}

impl Kind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Number => "number",
            Self::Date => "date",
            Self::SingleSelect => "single_select",
            Self::MultiSelect => "multi_select",
            Self::Checkbox => "checkbox",
            Self::Url => "url",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Value {
    Text(String),
    Number(f64),
    Date(String),
    SingleSelect(String),
    MultiSelect(Vec<String>),
    Checkbox(bool),
    Url(String),
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub field_id: String,
    pub value: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValuePatch {
    pub contact_id: String,
    pub expected_revision: i64,
    pub expected_epoch: String,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub id: String,
    pub label: String,
    pub color: String,
    pub archived: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub id: String,
    pub revision: i64,
    pub name: String,
    pub kind: Kind,
    pub color: String,
    pub agent_access: Access,
    pub show_in_details: bool,
    pub default_value: Option<Value>,
    pub archived: bool,
    pub options: Vec<Choice>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    Hidden,
    #[default]
    Read,
    Write,
}

impl Access {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Read => "read",
            Self::Write => "write",
        }
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub id: Option<String>,
    pub expected_revision: Option<i64>,
    pub name: String,
    pub kind: Kind,
    pub color: String,
    pub agent_access: Access,
    pub show_in_details: bool,
    pub default_value: Option<Value>,
    pub options: Vec<Choice>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Values {
    pub revision: i64,
    pub epoch: String,
    pub entries: Vec<Entry>,
}
