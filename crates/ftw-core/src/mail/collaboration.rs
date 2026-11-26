use super::{
    audit, check_text, file_workspace, policy, response, Failure, MailService, Reply, Result,
};
use crate::crm::{self, Actor};
use ftw_ipc::{
    agent::{AccessArea, Collaboration, Command, Scope},
    crm::{Command as CrmCommand, Source},
    Response,
};

impl MailService {
    pub(super) async fn set_collaboration(
        &self,
        id: &str,
        mut scopes: Vec<Scope>,
    ) -> Result<Reply> {
        scopes.sort_unstable_by_key(|scope| *scope as u8);
        scopes.dedup();
        let encoded = serde_json::to_string(&scopes)
            .map_err(|_| Failure::Code("invalid_agent_collaboration_scopes"))?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let changed = sqlx::query(
            "UPDATE mail_agent_clients SET collaboration_scopes=? WHERE id=? AND NOT revoked",
        )
        .bind(encoded)
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(Failure::Code("mail_agent_unregistered"));
        }
        audit(
            &mut tx,
            Source::Manual,
            "mail_agent_settings",
            id,
            "set_collaboration_scopes",
        )
        .await?;
        tx.commit().await?;
        Ok(Reply::Saved { id: id.into() })
    }

    async fn registered_collaborator(&self, token: &str) -> Result<(String, Vec<Scope>)> {
        let (id, encoded): (String, String) = sqlx::query_as(
            "SELECT id,collaboration_scopes FROM mail_agent_clients WHERE token_digest=? AND NOT revoked")
            .bind(policy::digest(token)?).fetch_optional(&self.pool).await?
            .ok_or(Failure::Code("mail_agent_unregistered"))?;
        let scopes: Vec<Scope> =
            serde_json::from_str(&encoded).map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
        Ok((id, scopes))
    }

    pub(super) async fn set_registration_required(
        &self,
        area: AccessArea,
        required: bool,
    ) -> Result<Reply> {
        let (column, allowed, restricted) = match area {
            AccessArea::Crm => (
                "require_registered_crm",
                "allow_legacy_crm",
                "require_registered_crm",
            ),
            AccessArea::Mail => (
                "require_registered_mail",
                "allow_legacy_mail",
                "require_registered_mail",
            ),
        };
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query(&format!(
            "UPDATE agent_collaboration_settings SET {column}=? WHERE id=1"
        ))
        .bind(required)
        .execute(&mut *tx)
        .await?;
        audit(
            &mut tx,
            Source::Manual,
            "mail_agent_settings",
            "profile",
            if required { restricted } else { allowed },
        )
        .await?;
        tx.commit().await?;
        Ok(Reply::Saved {
            id: "profile".into(),
        })
    }

    pub(super) async fn session_actor(
        &self,
        token: Option<&str>,
        area: AccessArea,
        accepted: &[Scope],
    ) -> Result<(Option<String>, bool)> {
        let statement = match area {
            AccessArea::Crm => {
                "SELECT require_registered_crm FROM agent_collaboration_settings WHERE id=1"
            }
            AccessArea::Mail => {
                "SELECT require_registered_mail FROM agent_collaboration_settings WHERE id=1"
            }
        };
        let required: bool = sqlx::query_scalar(statement).fetch_one(&self.pool).await?;
        match (token, required) {
            (Some(token), _) => {
                let (id, scopes) = self.registered_collaborator(token).await?;
                if required && !accepted.iter().any(|scope| scopes.contains(scope)) {
                    return Err(Failure::Code("agent_collaboration_not_authorized"));
                }
                Ok((Some(id), required))
            }
            (None, false) => Ok((None, false)),
            (None, true) => Err(Failure::Code("mail_agent_unregistered")),
        }
    }

    pub async fn collaborate_crm(&self, token: Option<&str>, command: Command) -> Response {
        let (actor, required) = match self
            .session_actor(token, AccessArea::Crm, &[command.scope()])
            .await
        {
            Ok(value) => value,
            Err(error) => return response(Err(error)),
        };
        if required
            && matches!(
                &command,
                Command::UpdateCustomer {
                    expected_revision: None,
                    ..
                } | Command::UpdateCustomer {
                    expected_epoch: None,
                    ..
                }
            )
        {
            return response(Err(Failure::Code("agent_crm_revision_required")));
        }
        crate::agent::crm_call(&self.pool, command, actor.as_deref()).await
    }

    pub async fn collaborate(&self, token: &str, command: Collaboration) -> Response {
        use ftw_ipc::agent::{CompanyChange, CustomerBatchChange, LeadDecision};
        let (id, scopes) = match self.registered_collaborator(token).await {
            Ok(collaborator) => collaborator,
            Err(error) => return response(Err(error)),
        };
        let creates_contact = matches!(&command, Collaboration::ManageCompany { change, .. }
            if matches!(change.as_ref(), CompanyChange::CreateContact { .. }));
        let requires_crm_read = matches!(
            &command,
            Collaboration::PreviewCustomerBatch { .. }
                | Collaboration::UpdateCustomerBatch { .. }
                | Collaboration::MailAnalytics { .. }
                | Collaboration::ReviewLead { .. }
                | Collaboration::ReviewEvidence { .. }
                | Collaboration::ConfirmReplyLink { .. }
        );
        let requires_crm_write = creates_contact
            || matches!(
                &command,
                Collaboration::UpdateCustomerBatch { .. }
                    | Collaboration::ReviewLead {
                        decision: LeadDecision::Promote,
                        ..
                    }
            );
        let requires_mail_read = matches!(
            &command,
            Collaboration::MailAnalytics { .. }
                | Collaboration::ConfirmReplyLink { .. }
                | Collaboration::CreateReplyDraft { .. }
        );
        if command
            .scope()
            .is_some_and(|scope| !scopes.contains(&scope))
            || (requires_crm_read && !scopes.contains(&Scope::ReadCrm))
            || (requires_crm_write && !scopes.contains(&Scope::WriteCrm))
            || (requires_mail_read && !scopes.contains(&Scope::ReadMail))
        {
            return response(Err(Failure::Code("agent_collaboration_not_authorized")));
        }
        let actor = Actor {
            source: Source::Agent,
            id: &id,
            agent_run_id: None,
        };
        let mail = match command {
            Collaboration::CreateCustomerProfile {
                request_id,
                input,
                entries,
            } => {
                let command = CrmCommand::CreateContactWithFields {
                    input,
                    entries,
                    allow_duplicate: false,
                    company: None,
                };
                return self
                    .run_crm_change(actor, "create_customer_profile", &request_id, command)
                    .await;
            }
            Collaboration::CreateReplyDraft { request_id, input } => {
                let operation = "create_reply_draft";
                if let Err(existing) =
                    crate::agent::claim_for(&self.pool, operation, &request_id, &input, Some(&id))
                        .await
                {
                    return existing;
                }
                let actor = Actor {
                    agent_run_id: Some(&request_id),
                    ..actor
                };
                let reply = response(self.create_reply_draft(input, actor, None).await);
                return crate::agent::finish_for(
                    &self.pool,
                    operation,
                    &request_id,
                    reply,
                    Some(&id),
                )
                .await;
            }
            Collaboration::Search {
                keyword,
                category,
                offset,
            } => {
                return data_response(
                    crate::data::search::search_for_agent(
                        &self.pool, &keyword, category, offset, &scopes,
                    )
                    .await,
                );
            }
            Collaboration::CustomerQuality {
                category,
                keyword,
                offset,
            } => {
                let include_mail = scopes.contains(&Scope::ReadMail);
                if category.mail_related() && !include_mail {
                    return response(Err(Failure::Code("agent_collaboration_not_authorized")));
                }
                return crm::response(
                    crm::quality::list_scoped(
                        &self.pool,
                        category.as_str(),
                        &keyword,
                        offset,
                        include_mail,
                    )
                    .await,
                );
            }
            Collaboration::PreviewCustomerBatch { query } => {
                return crm::dispatch(
                    &self.pool,
                    CrmCommand::PreviewBulkContacts { query: *query },
                    actor,
                )
                .await;
            }
            Collaboration::UpdateCustomerBatch { request_id, change } => {
                let command = match *change {
                    CustomerBatchChange::Matching {
                        query,
                        fingerprint,
                        field,
                        value,
                    } => CrmCommand::UpdateMatchingContacts {
                        query,
                        fingerprint,
                        field: field.as_str().to_owned(),
                        value,
                    },
                    CustomerBatchChange::Selected {
                        targets,
                        field,
                        value,
                    } => CrmCommand::UpdateCells {
                        targets,
                        field: field.as_str().to_owned(),
                        value,
                    },
                };
                return self
                    .run_crm_change(actor, "update_customer_batch", &request_id, command)
                    .await;
            }
            Collaboration::MailAnalytics {
                days,
                include_legacy,
                export,
            } => {
                let report = if export {
                    crate::data::analytics::export_csv(&self.pool, days, include_legacy).await
                } else {
                    crate::data::analytics::report(&self.pool, days, include_legacy)
                        .await
                        .map(|data| ftw_ipc::data::Result::MailAnalytics { data })
                };
                return data_response(report);
            }
            Collaboration::AuditWorkspace {
                source,
                entity,
                keyword,
                days,
                offset,
            } => {
                return data_response(
                    crate::data::audit_workspace::list_scoped(
                        &self.pool,
                        offset,
                        &source,
                        &entity,
                        &keyword,
                        days,
                        Some(&scopes),
                    )
                    .await,
                );
            }
            Collaboration::ReviewLead {
                request_id,
                id,
                expected,
                decision,
            } => {
                let (operation, command) = match decision {
                    LeadDecision::Promote => (
                        "promote_lead_candidate",
                        CrmCommand::PromoteLead {
                            id,
                            allow_duplicate: false,
                            expected: Some(expected),
                        },
                    ),
                    decision => (
                        "review_lead_candidate",
                        CrmCommand::SetLeadReview {
                            id,
                            rejected: matches!(decision, LeadDecision::Reject),
                            expected: Some(expected),
                        },
                    ),
                };
                return self
                    .run_crm_change(actor, operation, &request_id, command)
                    .await;
            }
            Collaboration::ReviewEvidence {
                request_id,
                id,
                expected,
                decision,
            } => {
                let command = CrmCommand::ReviewLeadEvidence {
                    id,
                    revision: expected.revision,
                    epoch: expected.epoch,
                    status: decision.as_str().to_owned(),
                };
                return self
                    .run_crm_change(actor, "review_customer_evidence", &request_id, command)
                    .await;
            }
            Collaboration::ConfirmReplyLink {
                request_id,
                id: message,
                contact_id,
                expected_contact,
                expected_match,
            } => {
                let operation = "confirm_reply_link";
                if let Err(existing) = crate::agent::claim_for(
                    &self.pool,
                    operation,
                    &request_id,
                    &(&message, &contact_id, &expected_contact, &expected_match),
                    Some(&id),
                )
                .await
                {
                    return existing;
                }
                let reply = self
                    .confirm_reply_link(
                        Actor {
                            agent_run_id: Some(&request_id),
                            ..actor
                        },
                        &message,
                        &contact_id,
                        expected_contact,
                        expected_match,
                    )
                    .await;
                return crate::agent::finish_for(
                    &self.pool,
                    operation,
                    &request_id,
                    reply,
                    Some(&id),
                )
                .await;
            }
            Collaboration::ManageCompany { request_id, change } => {
                let (operation, command) = match *change {
                    CompanyChange::SaveProfile {
                        anchor,
                        profile,
                        input,
                        include_group,
                    } => (
                        "save_company_profile",
                        CrmCommand::SaveCompanyProfile {
                            anchor,
                            profile,
                            input,
                            include_group,
                        },
                    ),
                    CompanyChange::SetMembership {
                        contact,
                        profile,
                        branch,
                    } => (
                        "set_company_membership",
                        CrmCommand::SetCompanyMembership {
                            contact,
                            profile,
                            branch,
                        },
                    ),
                    CompanyChange::Associate { anchor, contact } => (
                        "associate_company_contact",
                        CrmCommand::AssociateCompany { anchor, contact },
                    ),
                    CompanyChange::CreateContact {
                        profile,
                        input,
                        entries,
                        allow_duplicate,
                    } => (
                        "create_company_contact",
                        CrmCommand::CreateContactWithFields {
                            input,
                            allow_duplicate,
                            entries,
                            company: Some(profile),
                        },
                    ),
                };
                if let Err(existing) =
                    crate::agent::claim_for(&self.pool, operation, &request_id, &command, Some(&id))
                        .await
                {
                    return existing;
                }
                let actor = Actor {
                    agent_run_id: Some(&request_id),
                    ..actor
                };
                let reply = crm::dispatch(&self.pool, command, actor).await;
                return crate::agent::finish_for(
                    &self.pool,
                    operation,
                    &request_id,
                    reply,
                    Some(&id),
                )
                .await;
            }
            Collaboration::ImportCustomerCsv {
                csv,
                apply,
                default_stage,
                request_id,
            } => {
                if !apply {
                    return data_response(
                        crate::data::transfer::import_as(
                            &self.pool,
                            &csv,
                            false,
                            &default_stage,
                            actor,
                        )
                        .await,
                    );
                }
                let Some(request) = request_id.as_deref() else {
                    return response(Err(Failure::Code("agent_request_id_invalid")));
                };
                let operation = "import_customer_csv";
                if let Err(existing) = crate::agent::claim_for(
                    &self.pool,
                    operation,
                    request,
                    &(&csv, &default_stage),
                    Some(&id),
                )
                .await
                {
                    return existing;
                }
                let importer = Actor {
                    agent_run_id: Some(request),
                    ..actor
                };
                let reply = data_response(
                    crate::data::transfer::import_as(
                        &self.pool,
                        &csv,
                        true,
                        &default_stage,
                        importer,
                    )
                    .await,
                );
                return crate::agent::finish_for(&self.pool, operation, request, reply, Some(&id))
                    .await;
            }
            Collaboration::ExportCustomerCsv {
                scope,
                fields,
                offset,
            } => {
                let export =
                    crate::data::transfer::export_page(&self.pool, scope, &fields, offset).await;
                if export.is_ok() {
                    let logged = async {
                        let mut connection = self.pool.acquire().await?;
                        super::audit_as(
                            &mut connection,
                            Source::Agent,
                            &id,
                            "customer_csv",
                            "profile",
                            "export",
                            None,
                        )
                        .await
                    }
                    .await;
                    if let Err(error) = logged {
                        return response(Err(error));
                    }
                }
                return data_response(export);
            }
            Collaboration::SetContactArchive { target, archived } => {
                return crm::dispatch(
                    &self.pool,
                    CrmCommand::SetContactArchive { target, archived },
                    actor,
                )
                .await;
            }
            Collaboration::MergeContacts { primary, secondary } => {
                return crm::dispatch(
                    &self.pool,
                    CrmCommand::MergeContacts { primary, secondary },
                    actor,
                )
                .await;
            }
            Collaboration::SaveField { input } => {
                return crm::dispatch(
                    &self.pool,
                    CrmCommand::SaveField { definition: input },
                    actor,
                )
                .await;
            }
            Collaboration::ArchivedFields => {
                return crm::response(
                    crm::fields::list_archived_for_management(&self.pool, actor).await,
                );
            }
            Collaboration::MoveField {
                request_id,
                id,
                expected_revision,
                direction,
            } => {
                return self
                    .run_crm_change(
                        actor,
                        "move_customer_field",
                        &request_id,
                        CrmCommand::MoveField {
                            id,
                            expected_revision,
                            direction,
                        },
                    )
                    .await;
            }
            Collaboration::SetFieldArchive {
                request_id,
                id: field_id,
                expected_revision,
                archived,
            } => {
                let operation = "set_customer_field_archived";
                let command = CrmCommand::ArchiveField {
                    id: field_id,
                    expected_revision,
                    archived,
                };
                if let Err(existing) =
                    crate::agent::claim_for(&self.pool, operation, &request_id, &command, Some(&id))
                        .await
                {
                    return existing;
                }
                let actor = Actor {
                    agent_run_id: Some(&request_id),
                    ..actor
                };
                let reply = crm::dispatch(&self.pool, command, actor).await;
                return crate::agent::finish_for(
                    &self.pool,
                    operation,
                    &request_id,
                    reply,
                    Some(&id),
                )
                .await;
            }
            Collaboration::SaveView { input } => {
                return crm::response(crm::views::save(&self.pool, actor, input).await)
            }
            Collaboration::ArchiveView {
                id,
                expected_revision,
            } => {
                return crm::response(
                    crm::views::archive(&self.pool, actor, &id, expected_revision, true).await,
                );
            }
            Collaboration::MoveView {
                request_id,
                id,
                expected_revision,
                direction,
            } => {
                return self
                    .run_crm_change(
                        actor,
                        "move_customer_view",
                        &request_id,
                        CrmCommand::MoveView {
                            id,
                            expected_revision,
                            direction,
                        },
                    )
                    .await;
            }
            Collaboration::RestoreView {
                id,
                expected_revision,
            } => {
                return crm::response(
                    crm::views::archive(&self.pool, actor, &id, expected_revision, false).await,
                );
            }
            Collaboration::ArchivedViews => {
                return crm::response(crm::views::list_archived(&self.pool, actor).await);
            }
            Collaboration::ArchivedTemplates { purpose, offset } => {
                super::templates::list(&self.pool, &purpose, offset, true).await
            }
            Collaboration::ManageTemplate { request_id, change } => {
                use ftw_ipc::mail::TemplateChange;
                let operation = match &change {
                    TemplateChange::Save { .. } => "save_email_template",
                    TemplateChange::Archive { .. } => "set_email_template_archived",
                };
                if let Err(existing) =
                    crate::agent::claim_for(&self.pool, operation, &request_id, &change, Some(&id))
                        .await
                {
                    return existing;
                }
                let actor = Actor {
                    agent_run_id: Some(&request_id),
                    ..actor
                };
                let reply = super::templates::mutate(&self.pool, actor, change)
                    .await
                    .map(|(id, revision)| Reply::TemplateSaved { id, revision });
                return crate::agent::finish_for(
                    &self.pool,
                    operation,
                    &request_id,
                    response(reply),
                    Some(&id),
                )
                .await;
            }
            Collaboration::EditActivity {
                id,
                expected_revision,
                input,
            } => {
                return crm::dispatch(
                    &self.pool,
                    CrmCommand::UpdateActivity {
                        id,
                        expected_revision,
                        input,
                    },
                    actor,
                )
                .await;
            }
            Collaboration::CustomerHistory { contact_id, offset } => {
                return crm::dispatch(
                    &self.pool,
                    CrmCommand::ContactHistory {
                        id: contact_id,
                        offset,
                    },
                    actor,
                )
                .await;
            }
            Collaboration::CustomFieldHistory { contact_id, offset } => {
                return crm::dispatch(
                    &self.pool,
                    CrmCommand::CustomFieldHistory {
                        id: contact_id,
                        offset,
                    },
                    actor,
                )
                .await;
            }
            Collaboration::UploadDocumentChunk { input } => {
                self.upload_document_chunk(&id, input).await
            }
            Collaboration::ReadMailAttachment {
                source,
                id,
                index,
                offset,
            } => self.read_mail_attachment(source, &id, index, offset).await,
            Collaboration::ReviewReplies { offset } => {
                return self
                    .inbox(ftw_ipc::inbox::Command::List {
                        account_id: None,
                        contact_id: None,
                        unmatched: true,
                        offset,
                        keyword: None,
                    })
                    .await;
            }
            Collaboration::EditLead {
                request_id,
                id: lead_id,
                expected,
                patch,
            } => {
                let operation = "update_lead_candidate";
                if let Err(existing) = crate::agent::claim_for(
                    &self.pool,
                    operation,
                    &request_id,
                    &(&lead_id, &expected, &patch),
                    Some(&id),
                )
                .await
                {
                    return existing;
                }
                let actor = Actor {
                    agent_run_id: Some(&request_id),
                    ..actor
                };
                let reply = crm::dispatch(
                    &self.pool,
                    CrmCommand::PatchLead {
                        id: lead_id,
                        expected,
                        patch,
                    },
                    actor,
                )
                .await;
                return crate::agent::finish_for(
                    &self.pool,
                    operation,
                    &request_id,
                    reply,
                    Some(&id),
                )
                .await;
            }
            Collaboration::ReplyCandidates { id } => {
                return self
                    .inbox(ftw_ipc::inbox::Command::ReviewCandidates { id })
                    .await;
            }
            Collaboration::SuggestReplyLink {
                id: message,
                contact_id,
            } => {
                return super::inbox::response(
                    self.suggest_reply_link(&id, &message, &contact_id).await,
                );
            }
            Collaboration::ListRuns {
                filter,
                keyword,
                offset,
            } => {
                return data_response(
                    crate::data::agent_runs::list(&self.pool, filter, &keyword, offset, Some(&id))
                        .await,
                );
            }
            Collaboration::UpdateMailJob { job_id, job_action } => {
                return data_response(self.update_mail_job(actor, &job_id, job_action).await);
            }
            Collaboration::ListSuppressions { offset } => self.suppressions(offset).await,
            Collaboration::ListDocuments { contact_id, offset } => {
                file_workspace::list(&self.pool, Some(contact_id), "", "document", offset, None)
                    .await
            }
            Collaboration::UploadDocument {
                contact_id,
                filename,
                content_base64,
            } => {
                self.upload_document(&contact_id, &id, filename, &content_base64)
                    .await
            }
            Collaboration::ReadDocument { id, offset } => self.read_document(&id, offset).await,
            Collaboration::EditDraft {
                id,
                expected_revision,
                input,
            } => {
                if !input.attachments.is_empty() || input.replace_attachments {
                    return response(Err(Failure::Code("invalid_attachments")));
                }
                self.save_with_assets(Some(id), input, actor, None, Some(expected_revision))
                    .await
            }
            Collaboration::MailTask { task_id, offset } => {
                if let Err(error) = check_text(&task_id, 120) {
                    return response(Err(error));
                }
                return data_response(
                    crate::data::agent_runs::mail_jobs(&self.pool, &id, &task_id, offset).await,
                );
            }
        };
        response(mail)
    }

    async fn run_crm_change(
        &self,
        actor: Actor<'_>,
        operation: &str,
        request_id: &str,
        command: CrmCommand,
    ) -> Response {
        if let Err(existing) =
            crate::agent::claim_for(&self.pool, operation, request_id, &command, Some(actor.id))
                .await
        {
            return existing;
        }
        let reply = crm::dispatch(
            &self.pool,
            command,
            Actor {
                agent_run_id: Some(request_id),
                ..actor
            },
        )
        .await;
        crate::agent::finish_for(&self.pool, operation, request_id, reply, Some(actor.id)).await
    }
}

fn data_response(result: std::result::Result<ftw_ipc::data::Result, &'static str>) -> Response {
    match result {
        Ok(data) => Response::Data {
            data: Box::new(data),
        },
        Err(code) => response(Err(Failure::Code(code))),
    }
}
