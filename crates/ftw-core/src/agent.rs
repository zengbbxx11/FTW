use ftw_ipc::{
    agent::{Command, ContactPatch},
    crm::{self, Source},
    Response,
};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

pub async fn crm_call(pool: &SqlitePool, command: Command) -> Response {
    match command {
        Command::StateCatalog => run(pool, crm::Command::StateCatalog, None).await,
        Command::Timeline {
            id,
            offset,
            event_type,
        } => {
            run(
                pool,
                crm::Command::ListActivities {
                    contact_id: Some(id),
                    event_type,
                    offset,
                },
                None,
            )
            .await
        }
        Command::Leads { status, offset } => {
            run(
                pool,
                crm::Command::ListLeads {
                    offset,
                    review_status: status,
                },
                None,
            )
            .await
        }
        Command::CreateLead { request_id, input } => {
            let claim = claim(pool, "create_lead", &request_id, &input).await;
            if let Err(reply) = claim {
                return reply;
            }
            let response = run(
                pool,
                crm::Command::SaveLead {
                    id: None,
                    input,
                    allow_duplicate: false,
                },
                Some(&request_id),
            )
            .await;
            finish(pool, "create_lead", &request_id, response).await
        }
        Command::CreateCustomer { request_id, input } => {
            let claim = claim(pool, "create_customer", &request_id, &input).await;
            if let Err(reply) = claim {
                return reply;
            }
            let response = run(
                pool,
                crm::Command::SaveContact {
                    id: None,
                    input,
                    allow_duplicate: false,
                },
                Some(&request_id),
            )
            .await;
            finish(pool, "create_customer", &request_id, response).await
        }
        Command::UpdateCustomer {
            request_id,
            id,
            expected_revision,
            expected_epoch,
            patch,
        } => {
            let claim = if expected_revision.is_none() && expected_epoch.is_none() {
                claim(pool, "update_customer", &request_id, &(&id, &patch)).await
            } else {
                claim(
                    pool,
                    "update_customer",
                    &request_id,
                    &(&id, &patch, expected_revision, &expected_epoch),
                )
                .await
            };
            if let Err(reply) = claim {
                return reply;
            }
            let response = update(
                pool,
                id,
                *patch,
                &request_id,
                expected_revision,
                expected_epoch,
            )
            .await;
            finish(pool, "update_customer", &request_id, response).await
        }
        Command::AddActivity { request_id, input } => {
            let claim = claim(pool, "add_activity", &request_id, &input).await;
            if let Err(reply) = claim {
                return reply;
            }
            let response = run(
                pool,
                crm::Command::SaveActivity { id: None, input },
                Some(&request_id),
            )
            .await;
            finish(pool, "add_activity", &request_id, response).await
        }
    }
}

async fn run(pool: &SqlitePool, cmd: crm::Command, request_id: Option<&str>) -> Response {
    crate::crm::dispatch(
        pool,
        cmd,
        crate::crm::Actor {
            source: Source::Agent,
            id: "mcp",
            agent_run_id: request_id,
        },
    )
    .await
}

async fn update(
    pool: &SqlitePool,
    id: String,
    patch: ContactPatch,
    request_id: &str,
    expected_revision: Option<i64>,
    expected_epoch: Option<String>,
) -> Response {
    let reply = run(
        pool,
        crm::Command::GetContact { id: id.clone() },
        Some(request_id),
    )
    .await;
    let Response::Crm { data } = reply else {
        return reply;
    };
    let crm::Result::Contact { record } = *data else {
        return error("agent_invalid_response");
    };
    let revision = expected_revision.unwrap_or(record.record_revision);
    let epoch = expected_epoch.or(Some(record.data_epoch));
    let mut input = record.input;

    for (field, value) in [
        (&mut input.stage, patch.stage),
        (&mut input.customer_level, patch.customer_level),
        (&mut input.primary_language, patch.primary_language),
        (&mut input.main_need, patch.main_need),
        (&mut input.sample_status, patch.sample_status),
        (&mut input.notes, patch.notes),
    ] {
        if let Some(value) = value {
            *field = value;
        }
    }
    // The patch leaves the identity and contact methods untouched. Existing duplicate
    // warnings should not block changes to status or notes.
    run(
        pool,
        crm::Command::SaveContactChecked {
            id,
            expected_revision: revision,
            expected_epoch: epoch,
            input: Box::new(input),
            allow_duplicate: true,
        },
        Some(request_id),
    )
    .await
}

pub async fn claim(
    pool: &SqlitePool,
    operation: &str,
    id: &str,
    payload: &impl serde::Serialize,
) -> std::result::Result<(), Response> {
    if !(8..=120).contains(&id.len())
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b))
    {
        return Err(error("agent_request_id_invalid"));
    }
    let serialized = serde_json::to_vec(payload).map_err(|_| error("agent_request_invalid"))?;
    let digest = format!("{:x}", Sha256::digest(serialized));
    let affected = sqlx::query(
        "INSERT INTO agent_requests(operation,request_id,payload_sha256) VALUES (?,?,?)
        ON CONFLICT(operation,request_id) DO NOTHING",
    )
    .bind(operation)
    .bind(id)
    .bind(&digest)
    .execute(pool)
    .await
    .map_err(|_| error("database_unavailable"))?
    .rows_affected();
    if affected == 1 {
        return Ok(());
    }

    let (old, body): (String, Option<String>) = sqlx::query_as("SELECT payload_sha256,response_json FROM agent_requests WHERE operation=? AND request_id=?")
        .bind(operation).bind(id).fetch_one(pool).await.map_err(|_| error("database_unavailable"))?;
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

pub async fn finish(pool: &SqlitePool, operation: &str, id: &str, response: Response) -> Response {
    if let Ok(body) = serde_json::to_string(&response) {
        let result = sqlx::query(
            "UPDATE agent_requests SET response_json=? WHERE operation=? AND request_id=?",
        )
        .bind(body)
        .bind(operation)
        .bind(id)
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
