use super::*;
use crate::crm::{cells, contacts};
use ftw_ipc::{
    crm::{ContactInput, ContactQuery, ContactRow},
    views::{CellTarget, Column, Filter, Layout, Logic, Operator},
};
use serde_json::json;

const AGENT: Actor<'_> = Actor {
    source: Source::Agent,
    id: "view-test",
    agent_run_id: None,
};

async fn add(pool: &SqlitePool, name: &str, country: &str, stage: &str) -> String {
    let mut tx = pool.begin().await.unwrap();
    let Reply::Saved { id } = contacts::save(
        &mut tx,
        None,
        ContactInput {
            display_name: name.into(),
            country_region: country.into(),
            stage: stage.into(),
            ..Default::default()
        },
        false,
        Actor::DESKTOP,
    )
    .await
    .unwrap() else {
        panic!("contact was not saved")
    };
    tx.commit().await.unwrap();
    id
}

async fn rows(pool: &SqlitePool, query: ContactQuery) -> Vec<ContactRow> {
    let Reply::Contacts { rows, .. } = contacts::list(pool, query, AGENT).await.unwrap() else {
        panic!("expected customer rows")
    };
    rows
}

fn rule(field: &str, operator: Operator, value: serde_json::Value) -> Filter {
    Filter::Rule {
        field: field.into(),
        operator,
        value: Some(value),
    }
}

#[tokio::test]
async fn view_persists_and_board_uses_the_same_filters() {
    let dir = tempfile::tempdir().unwrap();
    let pool = crate::storage::open(dir.path(), dir.path()).await.unwrap();
    let accepted = add(&pool, "Alpine Camera", "德国", "won").await;
    let contacted = add(&pool, "Berlin Camera", "德国", "contacted").await;
    add(&pool, "Other Camera", "法国", "contacted").await;
    let query = ContactQuery {
        filter: Some(Filter::Group {
            logic: Logic::And,
            children: vec![
                rule("country", Operator::Eq, json!("德国")),
                Filter::Group {
                    logic: Logic::Or,
                    children: vec![
                        rule("stage", Operator::Eq, json!("won")),
                        rule("stage", Operator::Eq, json!("contacted")),
                    ],
                },
            ],
        }),
        offset: 20,
        ..Default::default()
    };
    let Reply::Saved { id } = save(
        &pool,
        Actor::DESKTOP,
        ViewInput {
            id: None,
            expected_revision: None,
            name: "德国客户".into(),
            hidden: false,
            is_default: true,
            config: Config {
                columns: vec![Column {
                    field: "name".into(),
                    width: 251,
                    hidden: false,
                    label: Some("联系人".into()),
                }],
                query,
                layout: Layout::Board,
                group_field: Some("stage".into()),
                ..Default::default()
            },
        },
    )
    .await
    .unwrap() else {
        panic!("view was not saved")
    };
    pool.close().await;
    let pool = crate::storage::open(dir.path(), dir.path()).await.unwrap();
    let Reply::Views { views } = list(&pool, AGENT, false).await.unwrap() else {
        panic!("missing views")
    };
    assert_eq!(views.iter().filter(|view| view.is_default).count(), 1);
    let view = views.iter().find(|view| view.id == id).unwrap();
    assert!(view.is_default);
    assert_eq!(view.config.query.offset, 0);
    assert_eq!(view.config.columns[0].width, 251);
    assert!(matches!(view.config.layout, Layout::Board));
    assert!(matches!(
        archive(&pool, Actor::DESKTOP, &id, view.revision, true).await,
        Err(Error::Invalid("view_choose_default"))
    ));
    let query = view.config.query.clone();
    let table = rows(&pool, query.clone()).await;
    assert_eq!(table.len(), 2);
    for (stage, id) in [("won", accepted), ("contacted", contacted)] {
        let group = ContactQuery {
            group: Some(("stage".into(), Some(stage.into()))),
            ..query.clone()
        };
        let board = rows(&pool, group).await;
        assert_eq!(board.len(), 1);
        assert_eq!(board[0].id, id);
        assert!(table.iter().any(|row| row.id == board[0].id));
    }
    let injection = ContactQuery {
        filter: Some(rule("name", Operator::Contains, json!("%' OR 1=1 --"))),
        ..Default::default()
    };
    assert!(rows(&pool, injection).await.is_empty());
    pool.close().await;
}

#[tokio::test]
async fn stale_bulk_edit_rolls_back_every_customer() {
    let dir = tempfile::tempdir().unwrap();
    let pool = crate::storage::open(dir.path(), dir.path()).await.unwrap();
    add(&pool, "Cell One", "", "contacted").await;
    add(&pool, "Cell Two", "", "contacted").await;
    let selected = rows(&pool, ContactQuery::default()).await;
    let targets = |stale: bool| {
        selected
            .iter()
            .enumerate()
            .map(|(i, row)| CellTarget {
                id: row.id.clone(),
                revision: row.record_revision + i64::from(i == 1 && stale),
                epoch: row.data_epoch.clone(),
            })
            .collect()
    };
    assert!(matches!(
        cells::update(&pool, Actor::DESKTOP, targets(true), "stage", "won").await,
        Err(Error::Conflict(_))
    ));
    assert!(rows(&pool, ContactQuery::default())
        .await
        .iter()
        .all(|row| row.stage == "contacted"));
    assert!(matches!(
        cells::update(&pool, Actor::DESKTOP, targets(false), "stage", "won")
            .await
            .unwrap(),
        Reply::CellsUpdated { count: 2 }
    ));
    assert!(rows(&pool, ContactQuery::default())
        .await
        .iter()
        .all(|row| row.stage == "won"));
    pool.close().await;
}
