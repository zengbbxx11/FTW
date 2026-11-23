use super::*;
use ftw_ipc::crm::{Command, ContactInput};

async fn add(pool: &SqlitePool, definition: Definition) -> String {
    let Reply::Saved { id } = save(pool, Actor::DESKTOP, definition).await.unwrap() else {
        panic!("field write")
    };
    id
}

fn definition(
    name: &str,
    kind: Kind,
    access: Access,
    value: Option<Value>,
    options: Vec<Choice>,
) -> Definition {
    Definition {
        id: None,
        expected_revision: None,
        name: name.into(),
        kind,
        color: "teal".into(),
        agent_access: access,
        show_in_details: true,
        default_value: value,
        options,
    }
}

async fn record(pool: &SqlitePool, actor: Actor<'_>, contact: &str) -> Values {
    let Reply::FieldValues { record } = values(pool, actor, contact).await.unwrap() else {
        panic!("expected values")
    };
    record
}

#[tokio::test]
async fn typed_values_and_archived_options_survive_reload() {
    let dir = tempfile::tempdir().unwrap();
    let pool = crate::storage::open(dir.path(), dir.path()).await.unwrap();
    let first = new_id();
    let second = new_id();
    let choices = vec![
        Choice {
            id: first.clone(),
            label: "目录".into(),
            color: "blue".into(),
            archived: false,
        },
        Choice {
            id: second.clone(),
            label: "样品".into(),
            color: "rose".into(),
            archived: false,
        },
    ];
    let mut ids = Vec::new();
    let expectations = [
        (Kind::Text, Value::Text("BK05".into()), vec![]),
        (Kind::Number, Value::Number(19.5), vec![]),
        (Kind::Date, Value::Date("2028-02-29".into()), vec![]),
        (
            Kind::SingleSelect,
            Value::SingleSelect(first.clone()),
            choices.clone(),
        ),
        (
            Kind::MultiSelect,
            Value::MultiSelect(vec![first.clone(), second]),
            choices.clone(),
        ),
        (Kind::Checkbox, Value::Checkbox(false), vec![]),
        (
            Kind::Url,
            Value::Url("https://example.test/product".into()),
            vec![],
        ),
    ];
    for (i, (kind, value, options)) in expectations.iter().enumerate() {
        ids.push(
            add(
                &pool,
                definition(
                    &format!("属性 {i}"),
                    *kind,
                    Access::Write,
                    Some(value.clone()),
                    options.clone(),
                ),
            )
            .await,
        );
    }
    let response = super::super::dispatch(
        &pool,
        Command::CreateContactWithFields {
            input: Box::new(ContactInput {
                display_name: "Custom fields fixture".into(),
                ..Default::default()
            }),
            allow_duplicate: false,
            entries: vec![Entry {
                field_id: ids[0].clone(),
                value: Some(Value::Text("BK06".into())),
            }],
        },
        Actor::DESKTOP,
    )
    .await;
    let ftw_ipc::Response::Crm { data } = response else {
        panic!("{response:?}")
    };
    let Reply::Saved { id: contact } = *data else {
        panic!("{data:?}")
    };
    let mut updated = definition(
        "开发类型（改名）",
        Kind::SingleSelect,
        Access::Write,
        None,
        choices,
    );
    updated.id = Some(ids[3].clone());
    updated.expected_revision = Some(1);
    updated.options[0].archived = true;
    save(&pool, Actor::DESKTOP, updated).await.unwrap();
    move_field(&pool, Actor::DESKTOP, &ids[3], 2, -1)
        .await
        .unwrap();
    let invalid = record(&pool, Actor::DESKTOP, &contact).await;
    let result = patch(
        &pool,
        Actor::DESKTOP,
        ValuePatch {
            contact_id: contact.clone(),
            expected_revision: invalid.revision,
            expected_epoch: invalid.epoch,
            entries: vec![Entry {
                field_id: ids[2].clone(),
                value: Some(Value::Date("2026-02-29".into())),
            }],
        },
    )
    .await;
    assert!(matches!(result, Err(Error::Invalid("field_value"))));
    pool.close().await;

    let restarted = crate::storage::open(dir.path(), dir.path()).await.unwrap();
    let values: HashMap<_, _> = record(&restarted, Actor::DESKTOP, &contact)
        .await
        .entries
        .into_iter()
        .map(|entry| (entry.field_id, entry.value.unwrap()))
        .collect();
    assert_eq!(values[&ids[0]], Value::Text("BK06".into()));
    for (index, (_, expected, _)) in expectations.iter().enumerate().skip(1) {
        assert_eq!(&values[&ids[index]], expected);
    }
    let Reply::Fields { fields } = list(&restarted, Actor::DESKTOP, false).await.unwrap() else {
        panic!("definitions")
    };
    assert_eq!(fields[2].id, ids[3]);
    assert_eq!(fields[2].name, "开发类型（改名）");
    assert!(fields[2].options[0].archived);
    archive(&restarted, Actor::DESKTOP, &ids[3], 3, true)
        .await
        .unwrap();
    assert!(record(&restarted, Actor::DESKTOP, &contact)
        .await
        .entries
        .iter()
        .all(|entry| entry.field_id != ids[3]));
    archive(&restarted, Actor::DESKTOP, &ids[3], 4, false)
        .await
        .unwrap();
    let recovered = record(&restarted, Actor::DESKTOP, &contact).await;
    assert!(recovered
        .entries
        .iter()
        .any(|entry| entry.field_id == ids[3]
            && entry.value == Some(Value::SingleSelect(first.clone()))));
    restarted.close().await;
}

#[tokio::test]
async fn agent_access_and_validation_are_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let pool = crate::storage::open(dir.path(), dir.path()).await.unwrap();
    let visible = add(
        &pool,
        definition(
            "公开",
            Kind::Text,
            Access::Read,
            Some(Value::Text("read only".into())),
            vec![],
        ),
    )
    .await;
    let hidden = add(
        &pool,
        definition(
            "私有",
            Kind::Text,
            Access::Hidden,
            Some(Value::Text("private".into())),
            vec![],
        ),
    )
    .await;
    let editable = add(
        &pool,
        definition("调查来源", Kind::Url, Access::Write, None, vec![]),
    )
    .await;
    let mut tx = pool.begin().await.unwrap();
    let Reply::Saved { id } = contacts::save(
        &mut tx,
        None,
        ContactInput {
            display_name: "Shared field workspace".into(),
            ..Default::default()
        },
        false,
        Actor::DESKTOP,
    )
    .await
    .unwrap() else {
        panic!("contact")
    };
    tx.commit().await.unwrap();
    let agent = Actor {
        source: Source::Agent,
        id: "mcp",
        agent_run_id: Some("field-atomic-test"),
    };
    let opened = record(&pool, agent, &id).await;
    assert_eq!(opened.entries.len(), 1);
    assert_eq!(opened.entries[0].field_id, visible);
    let Reply::Fields { fields } = list(&pool, agent, true).await.unwrap() else {
        panic!("fields")
    };
    assert!(fields.iter().all(|field| field.id != hidden));
    let valid = Entry {
        field_id: editable.clone(),
        value: Some(Value::Url("https://example.test/research".into())),
    };
    let forbidden = Entry {
        field_id: visible,
        value: Some(Value::Text("overwritten".into())),
    };
    let make = |entries| ValuePatch {
        contact_id: id.clone(),
        expected_revision: opened.revision,
        expected_epoch: opened.epoch.clone(),
        entries,
    };
    assert!(matches!(
        patch(&pool, agent, make(vec![valid.clone(), forbidden])).await,
        Err(Error::ReadOnly)
    ));
    assert_eq!(record(&pool, agent, &id).await.entries.len(), 1);
    patch(&pool, agent, make(vec![valid])).await.unwrap();
    assert!(matches!(
        patch(
            &pool,
            agent,
            make(vec![Entry {
                field_id: editable,
                value: None
            }])
        )
        .await,
        Err(Error::Conflict(_))
    ));
    assert!(record(&pool, Actor::DESKTOP, &id)
        .await
        .entries
        .iter()
        .any(|entry| entry.field_id == hidden));
    let source: String = sqlx::query_scalar(
        "SELECT source FROM audit_logs WHERE operation='patch_custom_fields' AND entity_id=?",
    )
    .bind(&id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(source, "agent");
    pool.close().await;
}
