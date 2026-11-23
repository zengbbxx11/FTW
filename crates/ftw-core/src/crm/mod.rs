pub(crate) mod contacts;
mod duplicates;
mod fields;
mod leads;
mod timeline;
mod validation;

use ftw_ipc::{
    crm::{Command, Result as Reply, Source},
    Response,
};
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

const PAGE_SIZE: usize = 20;

#[derive(Clone, Copy)]
pub struct Actor<'a> {
    pub source: Source,
    pub id: &'a str,
    pub agent_run_id: Option<&'a str>,
}

impl Actor<'_> {
    pub const DESKTOP: Self = Self {
        source: Source::Manual,
        id: "desktop-user",
        agent_run_id: None,
    };
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("{0}")]
    Invalid(&'static str),
    #[error("record_not_found")]
    NotFound,
    #[error("record_is_read_only")]
    ReadOnly,
    #[error("record_revision_conflict")]
    Conflict(i64),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

type Result<T> = std::result::Result<T, Error>;

pub async fn dispatch(pool: &SqlitePool, command: Command, actor: Actor<'_>) -> Response {
    match execute(pool, command, actor).await {
        Ok(data) => Response::Crm {
            data: Box::new(data),
        },
        Err(error) => {
            let (code, detail) = match error {
                Error::Invalid(field) => ("crm_invalid_field", Some(field.to_owned())),
                Error::NotFound => ("crm_not_found", None),
                Error::ReadOnly => ("crm_read_only", None),
                Error::Conflict(revision) => ("crm_revision_conflict", Some(revision.to_string())),
                Error::Database(_) => ("database_unavailable", None),
            };
            Response::Error {
                code: code.to_owned(),
                detail,
            }
        }
    }
}

async fn execute(pool: &SqlitePool, command: Command, actor: Actor<'_>) -> Result<Reply> {
    match command {
        Command::ListContacts { query } => contacts::list(pool, query).await,
        Command::GetContact { id } => contacts::get(pool, &id).await,
        Command::StateCatalog => contacts::state_catalog(pool).await,
        Command::ListFields { include_archived } => {
            fields::list(pool, actor, include_archived).await
        }
        Command::SaveField { definition } => fields::save(pool, actor, definition).await,
        Command::ArchiveField {
            id,
            expected_revision,
            archived,
        } => fields::archive(pool, actor, &id, expected_revision, archived).await,
        Command::MoveField {
            id,
            expected_revision,
            direction,
        } => fields::move_field(pool, actor, &id, expected_revision, direction).await,
        Command::GetFieldValues { contact_id } => fields::values(pool, actor, &contact_id).await,
        Command::PatchFieldValues { patch } => fields::patch(pool, actor, patch).await,
        Command::CreateContactWithFields {
            input,
            allow_duplicate,
            entries,
        } => {
            let mut tx = pool.begin().await?;
            let result = contacts::save(&mut tx, None, *input, allow_duplicate, actor).await?;
            if let Reply::Saved { id } = &result {
                fields::apply(&mut tx, actor, id, entries).await?;
            }
            tx.commit().await?;
            Ok(result)
        }
        Command::SaveContactChecked {
            id,
            input,
            allow_duplicate,
            expected_revision,
            expected_epoch,
        } => {
            let mut tx = pool.begin().await?;
            contacts::check_revision(&mut tx, &id, expected_revision, expected_epoch.as_deref())
                .await?;
            let result = contacts::save(&mut tx, Some(id), *input, allow_duplicate, actor).await?;
            tx.commit().await?;
            Ok(result)
        }
        Command::SaveContact {
            id,
            input,
            allow_duplicate,
        } => {
            let mut tx = pool.begin().await?;
            let result = contacts::save(&mut tx, id, *input, allow_duplicate, actor).await?;
            tx.commit().await?;
            Ok(result)
        }
        Command::SetArchived { id, archived } => contacts::archive(pool, id, archived, actor).await,
        Command::ListLeads {
            offset,
            review_status,
        } => leads::list(pool, offset, &review_status).await,
        Command::GetLead { id } => leads::get(pool, &id).await,
        Command::SaveLead {
            id,
            input,
            allow_duplicate,
        } => leads::save(pool, id, *input, allow_duplicate, actor).await,
        Command::SetLeadReview { id, rejected } => leads::review(pool, id, rejected, actor).await,
        Command::PromoteLead {
            id,
            allow_duplicate,
        } => leads::promote(pool, id, allow_duplicate, actor).await,
        Command::ListActivities {
            contact_id,
            event_type,
            offset,
        } => timeline::list(pool, contact_id, &event_type, offset).await,
        Command::SaveActivity { id, input } => timeline::save(pool, id, input, actor).await,
    }
}

fn new_id() -> String {
    Uuid::new_v4().to_string()
}

fn id_or_new(id: Option<String>) -> Result<(String, bool)> {
    match id {
        Some(id) => {
            Uuid::parse_str(&id).map_err(|_| Error::Invalid("id"))?;
            Ok((id, false))
        }
        None => Ok((new_id(), true)),
    }
}

fn page<T>(mut rows: Vec<T>) -> (Vec<T>, bool) {
    let more = rows.len() > PAGE_SIZE;
    rows.truncate(PAGE_SIZE);
    (rows, more)
}

async fn audit(
    connection: &mut SqliteConnection,
    actor: Actor<'_>,
    entity: &str,
    id: &str,
    operation: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO audit_logs(id, actor_id, source, entity, entity_id, operation) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(new_id()).bind(actor.id).bind(actor.source.as_str()).bind(entity).bind(id).bind(operation)
        .execute(connection).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{dispatch, Actor};
    use ftw_ipc::{
        crm::{ActivityInput, Command, ContactInput, Result as Reply, Source},
        Response,
    };

    #[tokio::test]
    async fn shared_service_records_agent_source_and_rejects_system_email_activity() {
        let directory = tempfile::tempdir().unwrap();
        let pool = crate::storage::open(directory.path(), directory.path())
            .await
            .unwrap();
        let created = dispatch(
            &pool,
            Command::SaveContact {
                id: None,
                input: Box::new(ContactInput {
                    display_name: "Agent source test".into(),
                    ..Default::default()
                }),
                allow_duplicate: false,
            },
            Actor::DESKTOP,
        )
        .await;
        let Response::Crm { data } = created else {
            panic!("{created:?}")
        };
        let Reply::Saved { id } = *data else {
            panic!("{data:?}")
        };
        let agent = Actor {
            source: Source::Agent,
            id: "test-agent",
            agent_run_id: Some("test-run"),
        };
        for event in ["email_sent", "call"] {
            let response = dispatch(
                &pool,
                Command::SaveActivity {
                    id: None,
                    input: ActivityInput {
                        counts_as_contact: None,
                        contact_id: id.clone(),
                        event_type: event.into(),
                        occurred_at: 1_760_000_000,
                        summary: "Verified communication".into(),
                        details: String::new(),
                        ..Default::default()
                    },
                },
                agent,
            )
            .await;
            if event == "email_sent" {
                assert!(
                    matches!(response, Response::Error { code, .. } if code == "crm_invalid_field")
                );
            } else {
                assert!(
                    matches!(response, Response::Crm { data } if matches!(*data, Reply::Saved { .. }))
                );
            }
        }
        let (source, actor, run): (String, String, String) = sqlx::query_as(
            "SELECT source,actor_id,agent_run_id FROM activities WHERE contact_id=?",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            (source.as_str(), actor.as_str(), run.as_str()),
            ("agent", "test-agent", "test-run")
        );
        pool.close().await;
    }
}
