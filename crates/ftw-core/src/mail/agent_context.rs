use super::{response, Failure, MailService, Result};
use crate::storage;
use ftw_ipc::{
    agent::{Command, DefinitionKind},
    Response,
};
use serde_json::{json, Value};
use sqlx::Row;

impl MailService {
    pub(super) async fn agent_context(&self, id: &str) -> Result<Value> {
        let versions = storage::changes(&self.pool).await?;
        let schema_version = storage::schema_version(&self.pool).await?;
        let mut db = self.pool.acquire().await?;
        let (name, automatic): (String, bool) =
            sqlx::query_as("SELECT name,automatic FROM mail_agent_clients WHERE id=?")
                .bind(id)
                .fetch_one(&mut *db)
                .await?;
        let policy = super::policy::current(&mut db).await?;
        let accounts = sqlx::query(
            "SELECT id,display_name,email_address FROM email_accounts WHERE archived_at IS NULL ORDER BY display_name,id",
        ).map(|row: sqlx::sqlite::SqliteRow| {
            let id: String = row.get("id");
            json!({"id": id, "name": row.get::<String, _>("display_name"),
                "email": row.get::<String, _>("email_address"),
                "authorized": policy.mode == "automatic" && automatic && self.ledger.is_some() && policy.accounts.contains(&id)})
        }).fetch_all(&mut *db).await?;
        Ok(json!({
            "kind": "workbench_context", "connected": true, "status": "ready",
            "version": env!("CARGO_PKG_VERSION"), "schema_version": schema_version,
            "agent": {"id": id, "name": name},
            "modules": ["crm", "mail", "files", "reports", "tasks", "audit"],
            "definitions": {"fields": DefinitionKind::Fields.version(&versions),
                "states": DefinitionKind::States.version(&versions), "views": DefinitionKind::Views.version(&versions)},
            "sender_accounts": accounts,
            "send": {"mode": policy.mode, "authorized": policy.mode == "automatic" && automatic && self.ledger.is_some(),
                "purposes": policy.purposes, "daily_limit": policy.daily_limit, "task_limit": policy.task_limit,
                "interval_seconds": policy.interval_seconds}
        }))
    }

    pub(super) async fn agent_definitions(
        &self,
        id: &str,
        kind: DefinitionKind,
        known_version: Option<String>,
    ) -> Response {
        let versions = match storage::changes(&self.pool).await {
            Ok(versions) => versions,
            Err(error) => return response(Err(error.into())),
        };
        let version = kind.version(&versions);
        if known_version.as_deref() == Some(version.as_str()) {
            return agent_data(Ok(
                json!({"kind": "definitions", "version": version, "unchanged": true}),
            ));
        }
        let command = match kind {
            DefinitionKind::Fields => Command::Fields,
            DefinitionKind::States => Command::StateCatalog,
            DefinitionKind::Views => Command::Views,
        };
        match crate::agent::crm_call(&self.pool, command, Some(id)).await {
            Response::Crm { data } => agent_data(Ok(json!({
                "kind": "definitions", "version": version, "unchanged": false, "data": data
            }))),
            result => result,
        }
    }

    pub(super) async fn draft_statuses(&self, ids: Vec<String>) -> Result<Value> {
        if ids.is_empty() || ids.len() > 10 {
            return Err(Failure::Code("invalid_batch_size"));
        }
        let rows = sqlx::query(
            "SELECT requested.value id,d.revision,d.state,d.job_id,j.outcome,
                    EXISTS(SELECT 1 FROM mail_delivery_events e WHERE e.outbox_id=j.id AND e.action='failed') bounced
             FROM json_each(?) requested LEFT JOIN mail_drafts_review d ON d.id=requested.value
             LEFT JOIN email_outbox_jobs j ON j.id=d.job_id ORDER BY requested.key",
        ).bind(json!(ids).to_string()).map(|row: sqlx::sqlite::SqliteRow| {
            let id: String = row.get("id");
            let Some(state) = row.get::<Option<String>, _>("state") else {
                return json!({"id": id, "code": "mail_not_found", "retryable": false});
            };
            json!({"id": id, "state": state, "revision": row.get::<i64,_>("revision"),
                "job_id": row.get::<Option<String>,_>("job_id"), "outcome": row.get::<Option<String>,_>("outcome"),
                "bounced": row.get::<bool,_>("bounced")})
        }).fetch_all(&self.pool).await?;
        Ok(json!({"kind": "draft_statuses", "rows": rows}))
    }
}

pub(super) fn agent_data(result: Result<Value>) -> Response {
    match result {
        Ok(data) => Response::AgentData { data },
        Err(error) => response(Err(error)),
    }
}
