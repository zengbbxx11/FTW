use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Copy, Serialize)]
pub struct Guidance {
    pub retryable: bool,
    pub requires_human: bool,
}

pub fn guidance(code: &str) -> Guidance {
    Guidance {
        retryable: matches!(
            code,
            "crm_revision_conflict"
                | "mail_revision_conflict"
                | "template_revision_conflict"
                | "mail_link_changed"
                | "bulk_scope_changed"
                | "agent_crm_revision_required"
        ),
        requires_human: matches!(
            code,
            "approval_required"
                | "approval_expired"
                | "approval_snapshot_changed"
                | "agent_request_uncertain"
                | "core_exchange_unknown"
                | "result_encoding_failed"
                | "agent_collaboration_not_authorized"
                | "mail_agent_unregistered"
                | "mail_agent_not_authorized"
                | "agent_policy_not_authorized"
                | "lead_promotion_review_required"
                | "mail_link_human_review_required"
                | "contact_method_verification_required"
                | "mail_reply_link_required"
                | "mail_reply_header_missing"
                | "mail_reply_context_changed"
        ),
    }
}

#[derive(Serialize)]
pub struct Receipt {
    pub version: u8,
    pub operation: String,
    pub status: String,
    pub success: Option<bool>,
    #[serde(flatten)]
    pub guidance: Guidance,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_version: Option<i64>,
}

impl Receipt {
    pub fn failure(code: &str, detail: Option<String>) -> Self {
        let uncertain = matches!(
            code,
            "agent_request_uncertain" | "core_exchange_unknown" | "result_encoding_failed"
        );
        Self {
            version: 1,
            operation: String::new(),
            status: if uncertain { "unknown" } else { "error" }.into(),
            success: (!uncertain).then_some(false),
            guidance: guidance(
                detail
                    .as_deref()
                    .filter(|_| code == "crm_invalid_field")
                    .unwrap_or(code),
            ),
            code: Some(code.into()),
            detail,
            entity_id: None,
            job_id: None,
            entity_version: None,
        }
    }

    pub fn result(operation: &str, data: &Value) -> Self {
        let kind = data["kind"].as_str().unwrap_or("ok");
        let record = &data["record"];
        let state = data["state"].as_str().or_else(|| record["state"].as_str());
        let (status, success, requires_human) = match (kind, state) {
            (_, Some("delivery_unknown")) => ("delivery_unknown", None, true),
            (_, Some("smtp_accepted")) => ("smtp_accepted", Some(true), false),
            (_, Some("failed")) => ("failed", Some(false), true),
            (_, Some("pending_approval")) => ("pending_approval", None, true),
            ("queued", _) | (_, Some("queued" | "sending")) => {
                (state.unwrap_or("queued"), None, false)
            }
            ("sync_started", _) => ("sync_started", None, false),
            ("document_upload_progress", _) if data["document_id"].is_null() => {
                ("upload_in_progress", None, false)
            }
            ("document_upload_progress", _) => ("uploaded", Some(true), false),
            ("duplicates" | "agent_review_pending", _) => (kind, Some(false), true),
            ("saved", _) if operation == "request_email_approval" => {
                ("approval_requested", Some(true), true)
            }
            (_, Some(state)) => (state, Some(true), false),
            _ => (kind, Some(true), false),
        };
        let get_string = |key: &str| {
            data[key]
                .as_str()
                .or_else(|| record[key].as_str())
                .map(str::to_owned)
        };
        let entity_id = ["id", "contact_id", "draft_id", "document_id", "account_id"]
            .into_iter()
            .find_map(get_string);
        let entity_version = ["revision", "record_revision", "draft_revision"]
            .into_iter()
            .find_map(|key| data[key].as_i64().or_else(|| record[key].as_i64()));
        let code = match status {
            "failed" | "delivery_unknown" => {
                Some(get_string("outcome").unwrap_or_else(|| status.into()))
            }
            "duplicates" | "agent_review_pending" => Some(status.into()),
            _ => None,
        };
        Self {
            version: 1,
            operation: operation.into(),
            status: status.into(),
            success,
            guidance: Guidance {
                retryable: false,
                requires_human,
            },
            code,
            detail: None,
            entity_id,
            job_id: get_string("job_id"),
            entity_version,
        }
    }
}
