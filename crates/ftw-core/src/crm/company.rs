use super::{audit, contacts, new_id, page, validation, Actor, Error, Result};
use ftw_ipc::{
    crm::{CompanyContact, CompanyProfile, CompanyProfileInput, Result as Reply},
    views::CellTarget,
};
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection, SqlitePool};

const MEMBERS: &str = "
    FROM contacts c JOIN contacts anchor ON anchor.id=?
    JOIN crm_contact_dates dates ON dates.id=c.id
    JOIN mail_contact_outreach o ON o.contact_id=c.id
    LEFT JOIN crm_company_memberships owner ON owner.contact_id=anchor.id
    LEFT JOIN crm_company_memberships member ON member.contact_id=c.id
    WHERE c.archived_at IS NULL AND (
      member.company_id=owner.company_id OR (owner.company_id IS NULL AND member.company_id IS NULL AND (
      (anchor.company_name<>'' AND anchor.country_region<>''
       AND c.company_name=anchor.company_name COLLATE NOCASE
       AND c.country_region=anchor.country_region COLLATE NOCASE)
      OR (c.id=anchor.id AND (anchor.company_name='' OR anchor.country_region=''))
    )))";

pub async fn list(pool: &SqlitePool, id: &str, offset: u32) -> Result<Reply> {
    let mut tx = pool.begin().await?;
    let associated = sqlx::query(
        "SELECT p.*,m.branch,(SELECT epoch FROM app_change_epoch WHERE singleton=1) epoch
         FROM crm_company_memberships m JOIN crm_company_profiles p ON p.id=m.company_id WHERE m.contact_id=?",
    ).bind(id).fetch_optional(&mut *tx).await?;
    let (profile, branch) = match associated {
        Some(row) => (Some(profile_row(&row)?), row.get("branch")),
        None => (None, String::new()),
    };
    let (total, contacted) = sqlx::query_as(&format!(
        "SELECT COUNT(*), COUNT(CASE WHEN o.outreach_count>0 THEN 1 END) {MEMBERS}"
    ))
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let rows = sqlx::query(&format!(
        "SELECT c.id,c.display_name,c.stage,c.country_region,COALESCE(member.branch,'') branch,o.mail_status,dates.effective_last,
            (SELECT value FROM contact_methods WHERE contact_id=c.id AND kind='email'
             ORDER BY is_primary DESC, position, id LIMIT 1) email
         {MEMBERS}
         ORDER BY c.display_name COLLATE NOCASE,c.id LIMIT 21 OFFSET ?"
    ))
    .bind(id)
    .bind(i64::from(offset))
    .map(|row: sqlx::sqlite::SqliteRow| CompanyContact {
        id: row.get("id"),
        display_name: row.get("display_name"),
        email: row.get("email"),
        stage: row.get("stage"),
        mail_status: row.get("mail_status"),
        last_contact_at: row.get("effective_last"),
        country_region: row.get("country_region"),
        branch: row.get("branch"),
    })
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let (rows, has_more) = page(rows);
    Ok(Reply::CompanyContacts {
        rows,
        total,
        contacted,
        has_more,
        profile,
        branch,
    })
}

pub async fn associate(
    pool: &SqlitePool,
    actor: Actor<'_>,
    anchor: CellTarget,
    contact: CellTarget,
) -> Result<Reply> {
    if anchor.id == contact.id {
        return Err(Error::Invalid("company_contact"));
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let explicit: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM crm_company_memberships WHERE contact_id IN (?,?))",
    )
    .bind(&anchor.id)
    .bind(&contact.id)
    .fetch_one(&mut *tx)
    .await?;
    if explicit {
        return Err(Error::Invalid("company_profile_required"));
    }
    for target in [&anchor, &contact] {
        contacts::check_revision(&mut tx, &target.id, target.revision, Some(&target.epoch)).await?;
    }
    let company = contacts::record_on(&mut tx, &anchor.id).await?;
    let member = contacts::record_on(&mut tx, &contact.id).await?;
    if company.archived_at.is_some() || member.archived_at.is_some() {
        return Err(Error::ReadOnly);
    }
    if company.input.company_name.is_empty() || company.input.country_region.is_empty() {
        return Err(Error::Invalid("company_incomplete"));
    }
    let mut input = member.input;
    let mut changed = false;
    for (current, value) in [
        (&mut input.company_name, company.input.company_name),
        (&mut input.country_region, company.input.country_region),
    ] {
        if current.is_empty() {
            *current = value;
            changed = true;
        } else if !current.eq_ignore_ascii_case(&value) {
            return Err(Error::Invalid("company_identity_conflict"));
        }
    }
    let reply = if changed {
        contacts::save(&mut tx, Some(contact.id), input, true, actor).await?
    } else {
        Reply::Saved { id: contact.id }
    };
    tx.commit().await?;
    Ok(reply)
}

fn profile_row(row: &SqliteRow) -> Result<CompanyProfile> {
    Ok(CompanyProfile {
        id: row.get("id"),
        revision: row.get("revision"),
        epoch: row.get("epoch"),
        input: CompanyProfileInput {
            name: row.get("name"),
            website: row.get("website"),
            notes: row.get("notes"),
            aliases: serde_json::from_str(row.get("aliases_json"))
                .map_err(|error| sqlx::Error::Decode(Box::new(error)))?,
        },
    })
}

pub async fn find(pool: &SqlitePool, mut keyword: String) -> Result<Reply> {
    validation::text(&mut keyword, "keyword", 300, false)?;
    let rows = sqlx::query(
        "SELECT p.*,(SELECT epoch FROM app_change_epoch WHERE singleton=1) epoch FROM crm_company_profiles p
         WHERE ?1='' OR instr(lower(p.name),lower(?1))>0 OR EXISTS(
             SELECT 1 FROM json_each(p.aliases_json) a WHERE instr(lower(a.value),lower(?1))>0)
         ORDER BY p.name COLLATE NOCASE,p.id LIMIT 21",
    ).bind(keyword).fetch_all(pool).await?;
    let rows = rows.iter().map(profile_row).collect::<Result<Vec<_>>>()?;
    let (rows, has_more) = page(rows);
    Ok(Reply::CompanyProfiles { rows, has_more })
}

pub async fn save_profile(
    pool: &SqlitePool,
    actor: Actor<'_>,
    anchor: CellTarget,
    profile: Option<CellTarget>,
    mut input: CompanyProfileInput,
    include_group: bool,
) -> Result<Reply> {
    validation::text(&mut input.name, "company_name", 300, true)?;
    validation::text(&mut input.website, "website_url", 1000, false)?;
    validation::website(&input.website, "website_url")?;
    validation::text(&mut input.notes, "notes", 4000, false)?;
    if input.aliases.len() > 20 {
        return Err(Error::Invalid("company_aliases"));
    }
    let mut names = std::collections::HashSet::new();
    names.insert(input.name.to_ascii_lowercase());
    for alias in &mut input.aliases {
        validation::text(alias, "company_aliases", 300, true)?;
        if !names.insert(alias.to_ascii_lowercase()) {
            return Err(Error::Invalid("company_aliases"));
        }
    }
    let aliases =
        serde_json::to_string(&input.aliases).map_err(|_| Error::Invalid("company_aliases"))?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    contacts::check_revision(&mut tx, &anchor.id, anchor.revision, Some(&anchor.epoch)).await?;
    let record = contacts::record_on(&mut tx, &anchor.id).await?;
    if record.archived_at.is_some() {
        return Err(Error::ReadOnly);
    }
    let linked = association(&mut tx, &anchor.id).await?;
    let id = if let Some(profile) = profile {
        check_profile(&mut tx, &profile).await?;
        if linked.as_deref() != Some(profile.id.as_str()) {
            return Err(Error::Invalid("company_membership_conflict"));
        }
        sqlx::query("UPDATE crm_company_profiles SET name=?,website=?,notes=?,aliases_json=?,revision=revision+1 WHERE id=?")
            .bind(input.name).bind(input.website).bind(input.notes).bind(aliases).bind(&profile.id).execute(&mut *tx).await?;
        profile.id
    } else {
        if linked.is_some() {
            return Err(Error::Invalid("company_membership_conflict"));
        }
        let id = new_id();
        sqlx::query("INSERT INTO crm_company_profiles(id,name,website,notes,aliases_json) VALUES(?,?,?,?,?)")
            .bind(&id).bind(input.name).bind(input.website).bind(input.notes).bind(aliases).execute(&mut *tx).await?;
        let mut members = vec![anchor.id.clone()];
        let source = &record.input;
        if include_group && !source.company_name.is_empty() && !source.country_region.is_empty() {
            members.extend(
                sqlx::query_scalar::<_, String>(
                    "SELECT c.id FROM contacts c WHERE c.archived_at IS NULL AND c.id<>?
                 AND c.company_name=? COLLATE NOCASE AND c.country_region=? COLLATE NOCASE
                 AND NOT EXISTS(SELECT 1 FROM crm_company_memberships m WHERE m.contact_id=c.id)",
                )
                .bind(&anchor.id)
                .bind(&source.company_name)
                .bind(&source.country_region)
                .fetch_all(&mut *tx)
                .await?,
            );
        }
        for member in members {
            apply_membership(&mut tx, actor, &member, Some(&id), "").await?;
        }
        id
    };
    audit(
        &mut tx,
        actor,
        "contact",
        &anchor.id,
        "save_company_profile",
    )
    .await?;
    tx.commit().await?;
    Ok(Reply::Saved { id })
}

pub async fn membership(
    pool: &SqlitePool,
    actor: Actor<'_>,
    contact: CellTarget,
    profile: Option<CellTarget>,
    branch: String,
) -> Result<Reply> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    contacts::check_revision(&mut tx, &contact.id, contact.revision, Some(&contact.epoch)).await?;
    set_membership(&mut tx, actor, &contact.id, profile, branch).await?;
    tx.commit().await?;
    Ok(Reply::Saved { id: contact.id })
}

pub(super) async fn set_membership(
    db: &mut SqliteConnection,
    actor: Actor<'_>,
    contact: &str,
    profile: Option<CellTarget>,
    mut branch: String,
) -> Result<()> {
    validation::text(&mut branch, "company_branch", 150, false)?;
    let active: bool = sqlx::query_scalar("SELECT archived_at IS NULL FROM contacts WHERE id=?")
        .bind(contact)
        .fetch_optional(&mut *db)
        .await?
        .ok_or(Error::NotFound)?;
    if !active {
        return Err(Error::ReadOnly);
    }
    if let Some(profile) = &profile {
        check_profile(db, profile).await?;
        if association(db, contact)
            .await?
            .is_some_and(|id| id != profile.id)
        {
            return Err(Error::Invalid("company_membership_conflict"));
        }
    }
    apply_membership(
        db,
        actor,
        contact,
        profile.as_ref().map(|value| value.id.as_str()),
        &branch,
    )
    .await
}

async fn association(db: &mut SqliteConnection, contact: &str) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT company_id FROM crm_company_memberships WHERE contact_id=?")
            .bind(contact)
            .fetch_optional(db)
            .await?,
    )
}

async fn check_profile(db: &mut SqliteConnection, profile: &CellTarget) -> Result<()> {
    let (revision, epoch): (i64, String) = sqlx::query_as(
        "SELECT revision,(SELECT epoch FROM app_change_epoch WHERE singleton=1) FROM crm_company_profiles WHERE id=?",
    ).bind(&profile.id).fetch_optional(db).await?.ok_or(Error::NotFound)?;
    if revision == profile.revision && epoch == profile.epoch {
        Ok(())
    } else {
        Err(Error::Conflict(revision))
    }
}

async fn apply_membership(
    db: &mut SqliteConnection,
    actor: Actor<'_>,
    contact: &str,
    company: Option<&str>,
    branch: &str,
) -> Result<()> {
    let outcome = if let Some(company) = company {
        sqlx::query("INSERT INTO crm_company_memberships(contact_id,company_id,branch) VALUES(?,?,?)
            ON CONFLICT(contact_id) DO UPDATE SET branch=excluded.branch WHERE branch<>excluded.branch")
            .bind(contact).bind(company).bind(branch).execute(&mut *db).await?
    } else {
        sqlx::query("DELETE FROM crm_company_memberships WHERE contact_id=?")
            .bind(contact)
            .execute(&mut *db)
            .await?
    };
    if outcome.rows_affected() > 0 {
        sqlx::query("UPDATE contacts SET record_revision=record_revision+1,updated_at=unixepoch(),updated_by=? WHERE id=?")
            .bind(actor.id).bind(contact).execute(&mut *db).await?;
        audit(
            db,
            actor,
            "contact",
            contact,
            if company.is_some() {
                "set_company_membership"
            } else {
                "remove_company_membership"
            },
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ftw_ipc::crm::{Contact, ContactInput, Method};

    async fn create(pool: &SqlitePool, name: &str, company: &str, country: &str) -> Contact {
        let mut tx = pool.begin().await.unwrap();
        let Reply::Saved { id } = contacts::save(
            &mut tx,
            None,
            ContactInput {
                display_name: name.into(),
                company_name: company.into(),
                country_region: country.into(),
                stage: "contacted".into(),
                methods: vec![Method {
                    id: None,
                    kind: "email".into(),
                    value: format!("{name}@sample.example.test"),
                    label: String::new(),
                    is_primary: true,
                    is_verified: false,
                }],
                ..Default::default()
            },
            true,
            Actor::DESKTOP,
        )
        .await
        .unwrap() else {
            panic!("contact not saved")
        };
        tx.commit().await.unwrap();
        let mut db = pool.acquire().await.unwrap();
        contacts::record_on(&mut db, &id).await.unwrap()
    }

    fn target(record: &Contact) -> CellTarget {
        CellTarget {
            id: record.id.clone(),
            epoch: record.data_epoch.clone(),
            revision: record.record_revision,
        }
    }

    #[tokio::test]
    async fn association_keeps_contact_identity_and_quality_distinguishes_coworkers() {
        let dir = tempfile::tempdir().unwrap();
        let pool = crate::storage::open(dir.path(), dir.path()).await.unwrap();
        let anchor = create(&pool, "Avery", "Sample Optics", "UK").await;
        let member = create(&pool, "Morgan", "sample optics", "").await;
        let other = create(&pool, "Taylor", "Sample Optics", "US").await;
        assert!(matches!(
            associate(&pool, Actor::DESKTOP, target(&anchor), target(&other)).await,
            Err(Error::Invalid("company_identity_conflict"))
        ));
        associate(&pool, Actor::DESKTOP, target(&anchor), target(&member))
            .await
            .unwrap();
        assert!(matches!(
            associate(&pool, Actor::DESKTOP, target(&anchor), target(&member)).await,
            Err(Error::Conflict(_))
        ));
        let Reply::CompanyContacts { rows, total, .. } = list(&pool, &anchor.id, 0).await.unwrap()
        else {
            panic!("company contacts unavailable")
        };
        assert_eq!(total, 2);
        assert_eq!(rows[1].id, member.id);
        assert_eq!(rows[1].stage, "contacted");
        assert_eq!(rows[1].email.as_deref(), Some("Morgan@sample.example.test"));
        let Reply::CustomerQuality { rows, .. } =
            super::super::quality::list(&pool, "duplicate", "", 0)
                .await
                .unwrap()
        else {
            panic!("quality check unavailable")
        };
        assert!(rows.is_empty());

        sqlx::query("UPDATE contact_methods SET value='legacy-invalid' WHERE contact_id=?")
            .bind(&member.id)
            .execute(&pool)
            .await
            .unwrap();
        let Reply::CustomerQuality {
            rows,
            active_customers,
            ..
        } = super::super::quality::list(&pool, "invalid_email", "sample OPTICS", 0)
            .await
            .unwrap()
        else {
            panic!("quality check unavailable")
        };
        assert_eq!(active_customers, 3);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].contact_id, member.id);

        let Reply::Saved { id } = save_profile(
            &pool,
            Actor::DESKTOP,
            target(&anchor),
            None,
            CompanyProfileInput {
                name: "Sample Optics International".into(),
                website: String::new(),
                notes: "Independent profile".into(),
                aliases: vec!["Legacy Optics".into()],
            },
            true,
        )
        .await
        .unwrap() else {
            panic!("company profile not saved")
        };
        let Reply::CompanyProfiles { mut rows, .. } =
            find(&pool, "legacy optics".into()).await.unwrap()
        else {
            panic!("company search unavailable")
        };
        let profile = rows.pop().unwrap();
        assert_eq!(profile.id, id);
        membership(
            &pool,
            Actor::DESKTOP,
            target(&other),
            Some(CellTarget {
                id: profile.id.clone(),
                revision: profile.revision,
                epoch: profile.epoch,
            }),
            "US office".into(),
        )
        .await
        .unwrap();
        let Reply::CompanyContacts {
            rows,
            total,
            profile: saved,
            ..
        } = list(&pool, &other.id, 0).await.unwrap()
        else {
            panic!("company contacts unavailable")
        };
        assert_eq!(total, 3);
        assert_eq!(saved.unwrap().input.notes, "Independent profile");
        let foreign = rows.iter().find(|row| row.id == other.id).unwrap();
        assert_eq!(
            (foreign.country_region.as_str(), foreign.branch.as_str()),
            ("US", "US office")
        );
        pool.close().await;
    }
}
