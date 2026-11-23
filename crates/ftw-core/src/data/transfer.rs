use super::Result;
use crate::crm::{self, contacts};
use ftw_ipc::{
    crm::{ContactInput, Method, Result as CrmReply},
    data::{ImportIssue, Result as Reply},
    display_path,
};
use sqlx::{Acquire, Row, SqlitePool};
use std::{fs, path::Path};
use uuid::Uuid;

const HEADERS: [(&str, &[&str]); 13] = [
    ("客户名称", &["display_name", "name", "联系人", "客户名称"]),
    (
        "公司或店铺",
        &["company_name", "company", "公司", "公司名称", "公司或店铺"],
    ),
    (
        "国家或地区",
        &[
            "country_region",
            "country",
            "国家",
            "国家或地区",
            "国家/地区",
        ],
    ),
    ("客户阶段", &["stage", "status", "客户阶段"]),
    ("客户等级", &["customer_level", "grade", "客户等级"]),
    ("样品进度", &["sample_status", "样品进度"]),
    ("客户来源", &["customer_source", "source", "客户来源"]),
    ("邮箱", &["email", "邮箱", "客户邮箱", "电子邮件"]),
    ("电话", &["phone", "电话", "联系电话", "手机号"]),
    ("WhatsApp", &["whatsapp", "WhatsApp", "WhatsApp号码"]),
    (
        "网站",
        &["website_url", "website", "主页", "网站", "网站或店铺地址"],
    ),
    ("主要需求", &["main_need", "主要需求", "需求"]),
    ("备注", &["notes", "备注", "客户备注"]),
];

pub async fn import(
    pool: &SqlitePool,
    csv: &str,
    apply: bool,
    default_stage: &str,
) -> Result<Reply> {
    if csv.is_empty() || csv.len() > 512 * 1024 {
        return Err("csv_size_limit");
    }
    let mut reader = csv::Reader::from_reader(csv.trim_start_matches('\u{feff}').as_bytes());
    let header = reader.headers().map_err(|_| "csv_invalid")?;
    let indexes: Vec<_> = HEADERS
        .iter()
        .map(|(_, names)| {
            header.iter().position(|field| {
                names
                    .iter()
                    .any(|name| field.trim().eq_ignore_ascii_case(name))
            })
        })
        .collect();
    if indexes[0].is_none() {
        return Err("csv_name_column_required");
    }

    let mut tx = pool.begin().await.map_err(|_| "database_unavailable")?;
    let mut accepted = 0;
    let mut rejected = 0;
    let mut issues = Vec::new();
    for (row, record) in reader.records().enumerate() {
        if row >= 500 {
            return Err("csv_row_limit");
        }
        let reason = match record {
            Ok(record) => {
                let mut savepoint = tx.begin().await.map_err(|_| "database_unavailable")?;
                let input = contact_from(&record, &indexes, default_stage);
                match contacts::save(&mut savepoint, None, input, false, crm::Actor::DESKTOP).await
                {
                    Ok(CrmReply::Saved { .. }) => {
                        savepoint
                            .commit()
                            .await
                            .map_err(|_| "database_unavailable")?;
                        accepted += 1;
                        None
                    }
                    Ok(CrmReply::Duplicates { .. }) => Some("可能重复，请复核邮箱、网站或公司"),
                    _ => Some("资料无效，请检查名称、阶段、邮箱或网址"),
                }
            }
            Err(_) => Some("CSV 列数或内容格式有误"),
        };
        if let Some(reason) = reason {
            rejected += 1;
            if issues.len() < 100 {
                issues.push(ImportIssue {
                    row: row + 2,
                    reason: reason.into(),
                });
            }
        }
    }

    if apply {
        tx.commit().await.map_err(|_| "database_unavailable")?;
    } else {
        tx.rollback().await.map_err(|_| "database_unavailable")?;
    }
    Ok(Reply::ImportReport {
        accepted,
        rejected,
        issues,
        applied: apply,
    })
}

fn contact_from(
    row: &csv::StringRecord,
    indexes: &[Option<usize>],
    default_stage: &str,
) -> ContactInput {
    let value = |index: usize| {
        indexes[index]
            .and_then(|column| row.get(column))
            .unwrap_or_default()
            .trim()
    };
    let channels = [(7, "email"), (8, "phone"), (9, "whatsapp")]
        .into_iter()
        .filter_map(|(column, kind)| {
            let text = value(column);
            let text = text
                .strip_prefix("'")
                .filter(|value| value.starts_with('+'))
                .unwrap_or(text);
            if text.is_empty() {
                return None;
            }
            Some(Method {
                id: None,
                kind: kind.into(),
                value: text.into(),
                label: String::new(),
                is_primary: true,
                is_verified: false,
            })
        })
        .collect();
    let stage = match value(3) {
        "" => default_stage,
        "待开发" => "prospect",
        "已联系" => "contacted",
        "已回复" => "replied",
        "需求沟通" => "requirements",
        "报价或样品" => "quote_sample",
        "样品推进中" => "sample_in_progress",
        "商务谈判" => "negotiation",
        "已成交" | "成交" => "won",
        "暂停" => "paused",
        "不开发" => "excluded",
        "客户流失" => "lost",
        other => other,
    };
    ContactInput {
        display_name: value(0).into(),
        company_name: value(1).into(),
        country_region: value(2).into(),
        stage: stage.into(),
        customer_level: value(4).into(),
        sample_status: value(5).into(),
        customer_source: value(6).into(),
        website_url: value(10).into(),
        main_need: value(11).into(),
        notes: value(12).into(),
        methods: channels,
        ..Default::default()
    }
}

pub async fn export(pool: &SqlitePool, root: &Path) -> Result<Reply> {
    let folder = root.join("exports");
    fs::create_dir_all(&folder).map_err(|_| "csv_export_failed")?;
    let filename = format!("contacts-{}", Uuid::new_v4());
    let temporary = folder.join(format!("{filename}.partial"));
    let destination = folder.join(format!("{filename}.csv"));
    let result = export_to(pool, &temporary).await;
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    fs::rename(temporary, &destination).map_err(|_| "csv_export_failed")?;
    Ok(Reply::File {
        path: display_path(&destination),
    })
}

async fn export_to(pool: &SqlitePool, path: &Path) -> Result<()> {
    use futures_util::TryStreamExt;
    use std::io::Write;

    let mut file = fs::File::create(path).map_err(|_| "csv_export_failed")?;
    file.write_all(&[0xef, 0xbb, 0xbf])
        .map_err(|_| "csv_export_failed")?;
    let mut writer = csv::Writer::from_writer(file);
    writer
        .write_record(HEADERS.map(|(label, _)| label))
        .map_err(|_| "csv_export_failed")?;
    let mut rows = sqlx::query("SELECT display_name,company_name,country_region,stage,customer_level,sample_status,customer_source,
        COALESCE((SELECT value FROM contact_methods WHERE contact_id=c.id AND kind='email' ORDER BY is_primary DESC,position LIMIT 1),'') email,
        COALESCE((SELECT value FROM contact_methods WHERE contact_id=c.id AND kind='phone' ORDER BY is_primary DESC,position LIMIT 1),'') phone,
        COALESCE((SELECT value FROM contact_methods WHERE contact_id=c.id AND kind='whatsapp' ORDER BY is_primary DESC,position LIMIT 1),'') whatsapp,
        website_url,main_need,notes FROM contacts c WHERE archived_at IS NULL ORDER BY display_name,id").fetch(pool);
    while let Some(row) = rows.try_next().await.map_err(|_| "database_unavailable")? {
        let values: Vec<_> = (0..HEADERS.len())
            .map(|index| spreadsheet_safe(row.get::<String, _>(index)))
            .collect();
        writer
            .write_record(values)
            .map_err(|_| "csv_export_failed")?;
    }
    writer.flush().map_err(|_| "csv_export_failed")?;
    Ok(())
}

fn spreadsheet_safe(text: String) -> String {
    if text.trim_start().starts_with(['=', '+', '-', '@']) {
        format!("'{text}")
    } else {
        text
    }
}
