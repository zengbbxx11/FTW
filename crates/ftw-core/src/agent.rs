use ftw_ipc::{
    agent::Command,
    crm::{self, Source},
    Response,
};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

struct Caller<'a> {
    pool: &'a SqlitePool,
    actor_id: Option<&'a str>,
}

pub async fn crm_call(pool: &SqlitePool, command: Command, actor_id: Option<&str>) -> Response {
    let caller = Caller { pool, actor_id };
    match command {
        Command::GetCustomer { id } => caller.run(crm::Command::GetContact { id }, None).await,
        Command::QueryView { id, offset } => crate::crm::response(
            crate::crm::views::query_saved(pool, caller.actor(None), &id, offset).await,
        ),
        Command::Views => {
            caller
                .run(
                    crm::Command::ListViews {
                        include_archived: false,
                    },
                    None,
                )
                .await
        }
        Command::QueryCustomers { query } => {
            caller.run(crm::Command::ListContacts { query }, None).await
        }
        Command::StateCatalog => caller.run(crm::Command::StateCatalog, None).await,
        Command::DuplicateCustomers { keyword, offset } => {
            caller
                .run(
                    crm::Command::CustomerQuality {
                        category: "duplicate".into(),
                        keyword,
                        offset,
                    },
                    None,
                )
                .await
        }
        Command::SampleWorkspace {
            keyword,
            status,
            offset,
            by_batch,
        } => {
            caller
                .run(
                    crm::Command::SampleWorkspace {
                        keyword,
                        status,
                        offset,
                        by_batch,
                    },
                    None,
                )
                .await
        }
        Command::Fields => {
            caller
                .run(
                    crm::Command::ListFields {
                        include_archived: false,
                    },
                    None,
                )
                .await
        }
        Command::FieldValues { contact_id } => {
            caller
                .run(crm::Command::GetFieldValues { contact_id }, None)
                .await
        }
        Command::PatchFieldValues { request_id, patch } => {
            if let Err(reply) = caller
                .claim("patch_field_values", &request_id, &patch)
                .await
            {
                return reply;
            }
            let response = caller
                .run(crm::Command::PatchFieldValues { patch }, Some(&request_id))
                .await;
            caller
                .finish("patch_field_values", &request_id, response)
                .await
        }
        Command::Timeline {
            id,
            offset,
            event_type,
        } => {
            caller
                .run(
                    crm::Command::ListActivities {
                        keyword: None,
                        source: None,
                        contact_id: Some(id),
                        event_type,
                        offset,
                    },
                    None,
                )
                .await
        }
        Command::Leads { status, offset } => {
            caller
                .run(
                    crm::Command::ListLeads {
                        offset,
                        review_status: status,
                    },
                    None,
                )
                .await
        }
        Command::CreateLead { request_id, input } => {
            let claim = caller.claim("create_lead", &request_id, &input).await;
            if let Err(reply) = claim {
                return reply;
            }
            let response = caller
                .run(
                    crm::Command::SaveLead {
                        id: None,
                        input,
                        allow_duplicate: false,
                        expected: None,
                    },
                    Some(&request_id),
                )
                .await;
            caller.finish("create_lead", &request_id, response).await
        }
        Command::GetLead { id } => caller.run(crm::Command::GetLead { id }, None).await,
        Command::ListLeadEvidence { lead_id, offset } => {
            caller
                .run(crm::Command::ListLeadEvidence { lead_id, offset }, None)
                .await
        }
        Command::ListContactEvidence { contact_id, offset } => {
            caller
                .run(
                    crm::Command::ListContactEvidence { contact_id, offset },
                    None,
                )
                .await
        }
        Command::GetCustomerCompany { contact_id, offset } => {
            caller
                .run(
                    crm::Command::CompanyContacts {
                        id: contact_id,
                        offset,
                    },
                    None,
                )
                .await
        }
        Command::FindCompanyProfiles { keyword } => {
            caller
                .run(crm::Command::FindCompanyProfiles { keyword }, None)
                .await
        }
        Command::AddContactEvidence {
            request_id,
            contact_id,
            input,
        } => {
            let operation = "add_contact_evidence";
            if let Err(reply) = caller
                .claim(operation, &request_id, &(&contact_id, &input))
                .await
            {
                return reply;
            }
            let response = caller
                .run(
                    crm::Command::AddContactEvidence { contact_id, input },
                    Some(&request_id),
                )
                .await;
            caller.finish(operation, &request_id, response).await
        }
        Command::AddLeadEvidence {
            request_id,
            lead_id,
            input,
        } => {
            let operation = "add_lead_evidence";
            if let Err(reply) = caller
                .claim(operation, &request_id, &(&lead_id, &input))
                .await
            {
                return reply;
            }
            let response = caller
                .run(
                    crm::Command::AddLeadEvidence { lead_id, input },
                    Some(&request_id),
                )
                .await;
            caller.finish(operation, &request_id, response).await
        }
        Command::CreateCustomer { request_id, input } => {
            let claim = caller.claim("create_customer", &request_id, &input).await;
            if let Err(reply) = claim {
                return reply;
            }
            let response = caller
                .run(
                    crm::Command::SaveContact {
                        id: None,
                        input,
                        allow_duplicate: false,
                    },
                    Some(&request_id),
                )
                .await;
            caller
                .finish("create_customer", &request_id, response)
                .await
        }
        Command::UpdateCustomer {
            request_id,
            id,
            expected_revision,
            expected_epoch,
            patch,
        } => {
            let claim = if expected_revision.is_none() && expected_epoch.is_none() {
                caller
                    .claim("update_customer", &request_id, &(&id, &patch))
                    .await
            } else {
                caller
                    .claim(
                        "update_customer",
                        &request_id,
                        &(&id, &patch, expected_revision, &expected_epoch),
                    )
                    .await
            };
            if let Err(reply) = claim {
                return reply;
            }
            let response = crate::crm::response(
                crate::crm::changes::apply(
                    pool,
                    id,
                    *patch,
                    caller.actor(Some(&request_id)),
                    expected_revision,
                    expected_epoch.as_deref(),
                )
                .await,
            );
            caller
                .finish("update_customer", &request_id, response)
                .await
        }
        Command::SuggestCustomerDetail {
            request_id,
            id,
            expected_revision,
            expected_epoch,
            detail,
            evidence,
        } => {
            let operation = "suggest_customer_detail";
            if let Err(reply) = caller
                .claim(
                    operation,
                    &request_id,
                    &(&id, expected_revision, &expected_epoch, &detail, &evidence),
                )
                .await
            {
                return reply;
            }
            let response = crate::crm::response(
                crate::crm::changes::suggest_detail(
                    pool,
                    caller.actor(Some(&request_id)),
                    ftw_ipc::views::CellTarget {
                        id,
                        revision: expected_revision,
                        epoch: expected_epoch,
                    },
                    detail,
                    evidence,
                )
                .await,
            );
            caller.finish(operation, &request_id, response).await
        }
        Command::AddActivity { request_id, input } => {
            let claim = caller.claim("add_activity", &request_id, &input).await;
            if let Err(reply) = claim {
                return reply;
            }
            let response = caller
                .run(
                    crm::Command::SaveActivity { id: None, input },
                    Some(&request_id),
                )
                .await;
            caller.finish("add_activity", &request_id, response).await
        }
    }
}

impl<'a> Caller<'a> {
    fn actor<'b>(&'b self, request_id: Option<&'b str>) -> crate::crm::Actor<'b> {
        crate::crm::Actor {
            source: Source::Agent,
            id: self.actor_id.unwrap_or("mcp"),
            agent_run_id: request_id,
        }
    }

    async fn run(&self, command: crm::Command, request_id: Option<&str>) -> Response {
        crate::crm::dispatch(self.pool, command, self.actor(request_id)).await
    }

    async fn claim(
        &self,
        operation: &str,
        id: &str,
        payload: &impl serde::Serialize,
    ) -> std::result::Result<(), Response> {
        claim_for(self.pool, operation, id, payload, self.actor_id).await
    }

    async fn finish(&self, operation: &str, id: &str, response: Response) -> Response {
        finish_for(self.pool, operation, id, response, self.actor_id).await
    }
}

// A dot is excluded by request ID validation, which keeps registered and legacy keys disjoint.
fn request_key<'a>(actor_id: Option<&str>, id: &'a str) -> std::borrow::Cow<'a, str> {
    match actor_id {
        Some(actor) => format!(".{actor}.{id}").into(),
        None => id.into(),
    }
}

pub(crate) async fn claim_for(
    pool: &SqlitePool,
    operation: &str,
    id: &str,
    payload: &impl serde::Serialize,
    actor_id: Option<&str>,
) -> std::result::Result<(), Response> {
    if !(8..=120).contains(&id.len())
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b))
    {
        return Err(error("agent_request_id_invalid"));
    }
    let key = request_key(actor_id, id);
    let serialized = serde_json::to_vec(payload).map_err(|_| error("agent_request_invalid"))?;
    let digest = format!("{:x}", Sha256::digest(serialized));
    let affected = sqlx::query(
        "INSERT INTO agent_requests(operation,request_id,payload_sha256,actor_id) VALUES (?,?,?,?)
        ON CONFLICT(operation,request_id) DO NOTHING",
    )
    .bind(operation)
    .bind(&*key)
    .bind(&digest)
    .bind(actor_id)
    .execute(pool)
    .await
    .map_err(|_| error("database_unavailable"))?
    .rows_affected();
    if affected == 1 {
        return Ok(());
    }

    let (old, body): (String, Option<String>) = sqlx::query_as("SELECT payload_sha256,response_json FROM agent_requests WHERE operation=? AND request_id=?")
        .bind(operation).bind(&*key).fetch_one(pool).await.map_err(|_| error("database_unavailable"))?;
    if old != digest {
        return Err(error("agent_request_conflict"));
    }
    match body {
        Some(body) => {
            Err(serde_json::from_str(&body).unwrap_or_else(|_| error("agent_invalid_response")))
        }
        // A crash between the change and the result record must not replay a write.
        None => Err(error("agent_request_uncertain")),
    }
}

pub(crate) async fn finish_for(
    pool: &SqlitePool,
    operation: &str,
    id: &str,
    response: Response,
    actor_id: Option<&str>,
) -> Response {
    if let Ok(body) = serde_json::to_string(&response) {
        let result = sqlx::query(
            "UPDATE agent_requests SET response_json=? WHERE operation=? AND request_id=?",
        )
        .bind(body)
        .bind(operation)
        .bind(&*request_key(actor_id, id))
        .execute(pool)
        .await;
        if result.is_ok() {
            return response;
        }
    }
    error("agent_request_uncertain")
}

fn error(code: &str) -> Response {
    Response::Error {
        code: code.into(),
        detail: None,
    }
}
