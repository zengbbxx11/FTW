mod cells;
pub(crate) mod changes;
mod company;
pub(crate) mod contacts;
mod detail_changes;
mod duplicates;
mod evidence;
pub(crate) mod fields;
pub(crate) mod history;
mod leads;
mod merge;
pub(crate) mod quality;
mod query;
mod samples;
mod timeline;
mod validation;
pub(crate) mod views;

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
    // Keep the CRM command future out of the nested desktop and MCP dispatcher stacks.
    response(Box::pin(execute(pool, command, actor)).await)
}

pub(crate) fn response(result: Result<Reply>) -> Response {
    match result {
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
        Command::UpdateCells {
            targets,
            field,
            value,
        } => cells::update(pool, actor, targets, &field, &value).await,
        Command::PreviewBulkContacts { query } => cells::scope(pool, actor, query).await,
        Command::UpdateMatchingContacts {
            query,
            fingerprint,
            field,
            value,
        } => cells::update_scope(pool, actor, query, &fingerprint, &field, value).await,
        Command::ListAgentChanges {
            offset,
            pending,
            scope,
        } => {
            changes::list(
                pool,
                offset,
                scope
                    .as_deref()
                    .unwrap_or(if pending { "pending" } else { "" }),
            )
            .await
        }
        Command::ResolveAgentChange { id, accept } => {
            changes::resolve(pool, actor, &id, accept).await
        }
        Command::SampleWorkspace {
            keyword,
            status,
            offset,
            by_batch,
        } => samples::list(pool, keyword, &status, offset, by_batch).await,
        Command::MergeContacts { primary, secondary } => {
            merge::merge(pool, actor, primary, secondary).await
        }
        Command::CustomerQuality {
            category,
            keyword,
            offset,
        } => quality::list(pool, &category, &keyword, offset).await,
        Command::CompanyContacts { id, offset } => company::list(pool, &id, offset).await,
        Command::AssociateCompany { anchor, contact } => {
            company::associate(pool, actor, anchor, contact).await
        }
        Command::FindCompanyProfiles { keyword } => company::find(pool, keyword).await,
        Command::SaveCompanyProfile {
            anchor,
            profile,
            input,
            include_group,
        } => company::save_profile(pool, actor, anchor, profile, input, include_group).await,
        Command::SetCompanyMembership {
            contact,
            profile,
            branch,
        } => company::membership(pool, actor, contact, profile, branch).await,
        Command::CustomerStats => contacts::stats(pool).await,
        Command::ListContacts { query } => contacts::list(pool, query, actor).await,
        Command::PreviewContacts { query } => contacts::preview(pool, query, actor).await,
        Command::ListViews { include_archived } => views::list(pool, actor, include_archived).await,
        Command::SaveView { view } => views::save(pool, actor, view).await,
        Command::DeleteView {
            id,
            expected_revision,
        } => views::delete(pool, actor, &id, expected_revision).await,
        Command::ArchiveView {
            id,
            expected_revision,
            archived,
        } => views::archive(pool, actor, &id, expected_revision, archived).await,
        Command::MoveView {
            id,
            expected_revision,
            direction,
        } => views::move_view(pool, actor, &id, expected_revision, direction).await,
        Command::GetContact { id } => contacts::get(pool, &id).await,
        Command::DevelopmentOverview { id } => contacts::development_overview(pool, &id).await,
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
            company,
        } => {
            let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
            let result = contacts::save(&mut tx, None, *input, allow_duplicate, actor).await?;
            if let Reply::Saved { id } = &result {
                fields::apply(&mut tx, actor, id, 1, entries).await?;
                if company.is_some() {
                    company::set_membership(&mut tx, actor, id, company, String::new()).await?;
                }
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
        Command::SetContactArchive { target, archived } => {
            contacts::archive_checked(pool, target, archived, actor).await
        }
        Command::ListLeads {
            offset,
            review_status,
        } => leads::list(pool, offset, &review_status).await,
        Command::LeadWorkspace {
            offset,
            review_status,
            keyword,
            source,
        } => leads::workspace(pool, offset, &review_status, keyword, &source).await,
        Command::GetLead { id } => leads::get(pool, &id).await,
        Command::ListLeadEvidence { lead_id, offset } => {
            evidence::list(pool, evidence::Owner::Lead(&lead_id), offset).await
        }
        Command::AddLeadEvidence { lead_id, input } => {
            evidence::add(pool, actor, evidence::Owner::Lead(&lead_id), input).await
        }
        Command::ListContactEvidence { contact_id, offset } => {
            evidence::list(pool, evidence::Owner::Contact(&contact_id), offset).await
        }
        Command::AddContactEvidence { contact_id, input } => {
            evidence::add(pool, actor, evidence::Owner::Contact(&contact_id), input).await
        }
        Command::ReviewLeadEvidence {
            id,
            revision,
            epoch,
            status,
        } => evidence::review(pool, actor, &id, revision, &epoch, &status).await,
        Command::SaveLead {
            id,
            input,
            allow_duplicate,
            expected,
        } => leads::save(pool, id, *input, allow_duplicate, expected, actor).await,
        Command::PatchLead {
            id,
            expected,
            patch,
        } => leads::patch(pool, id, expected, *patch, actor).await,
        Command::SetLeadReview {
            id,
            rejected,
            expected,
        } => leads::review(pool, id, rejected, expected, actor).await,
        Command::PromoteLead {
            id,
            allow_duplicate,
            expected,
        } => leads::promote(pool, id, allow_duplicate, expected, actor).await,
        Command::ListActivities {
            contact_id,
            event_type,
            offset,
            keyword,
            source,
        } => timeline::list(pool, contact_id, &event_type, offset, keyword, source).await,
        Command::GetActivity { id } => timeline::get(pool, &id).await,
        Command::SaveActivity { id, input } => timeline::save(pool, id, input, actor).await,
        Command::UpdateActivity {
            id,
            expected_revision,
            input,
        } => timeline::save_guarded(pool, Some(id), input, actor, Some(expected_revision)).await,
        Command::ContactHistory { id, offset } => history::list(pool, &id, offset).await,
        Command::CustomFieldHistory { id, offset } => {
            history::custom_fields(pool, actor, &id, offset).await
        }
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
    sqlx::query("INSERT INTO audit_logs(id, actor_id, source, entity, entity_id, operation, task_id) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(new_id()).bind(actor.id).bind(actor.source.as_str()).bind(entity).bind(id).bind(operation)
        .bind(actor.agent_run_id)
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
