use crate::crm::ContactQuery;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Logic {
    And,
    Or,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    Eq,
    NotEq,
    Contains,
    Gte,
    Lte,
    Empty,
    WithinDays,
    OlderThanDays,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Filter {
    Group {
        logic: Logic,
        children: Vec<Filter>,
    },
    Rule {
        field: String,
        operator: Operator,
        value: Option<serde_json::Value>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Order {
    pub field: String,
    pub descending: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Column {
    pub field: String,
    pub label: Option<String>,
    pub width: u16,
    pub hidden: bool,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    #[default]
    Table,
    Board,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    Compact,
    #[default]
    Standard,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub query: ContactQuery,
    pub layout: Layout,
    pub density: Density,
    pub group_field: Option<String>,
    pub columns: Vec<Column>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SavedView {
    pub id: String,
    pub revision: i64,
    pub name: String,
    pub hidden: bool,
    pub is_default: bool,
    pub archived: bool,
    pub config: Config,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CellTarget {
    pub id: String,
    pub revision: i64,
    pub epoch: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViewInput {
    pub id: Option<String>,
    pub expected_revision: Option<i64>,
    pub name: String,
    pub hidden: bool,
    pub is_default: bool,
    pub config: Config,
}
