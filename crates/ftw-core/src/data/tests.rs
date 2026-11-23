use super::*;

#[tokio::test]
async fn preview_import_duplicate_filter_export_and_restore() {
    let dir = tempfile::tempdir().unwrap();
    let pool = crate::storage::open(dir.path(), dir.path()).await.unwrap();
    let sample = "客户名称,邮箱,国家/地区,样品进度\n示例客户,one@example.test,法国,样品运输中\n重复客户,one@example.test,法国,\n";
    let Reply::ImportReport {
        accepted,
        rejected,
        applied,
        ..
    } = transfer::import(&pool, sample, false, "won").await.unwrap()
    else {
        panic!("preview")
    };
    assert_eq!((accepted, rejected, applied), (1, 1, false));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contacts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let Reply::ImportReport {
        accepted, applied, ..
    } = transfer::import(&pool, sample, true, "won").await.unwrap()
    else {
        panic!("import")
    };
    assert_eq!((accepted, applied), (1, true));
    let Reply::File { path } = transfer::export(&pool, dir.path()).await.unwrap() else {
        panic!("export")
    };
    let exported = std::fs::read_to_string(path).unwrap();
    assert!(exported.contains("one@example.test") && !exported.contains("重复客户"));

    let epoch = crate::storage::changes(&pool).await.unwrap().epoch;
    let copy = backup::create(&pool, dir.path(), "manual").await.unwrap();
    sqlx::query("UPDATE contacts SET display_name='changed'")
        .execute(&pool)
        .await
        .unwrap();
    let filename = copy.file_name().unwrap().to_str().unwrap();
    let safety = backup::restore(&pool, dir.path(), filename).await.unwrap();
    assert!(safety.contains("before-restore"));
    assert_ne!(crate::storage::changes(&pool).await.unwrap().epoch, epoch);
    let restored: (String, String) = sqlx::query_as("SELECT display_name,stage FROM contacts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(restored, ("示例客户".into(), "won".into()));
    assert!(backup::restore(&pool, dir.path(), "../workbench.db")
        .await
        .is_err());
    pool.close().await;
}
