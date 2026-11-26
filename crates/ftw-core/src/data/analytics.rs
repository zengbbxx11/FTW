use super::{transfer, Reply, Result};
use ftw_ipc::{
    data::{MailAnalytics, MailAnalyticsDay, MailAnalyticsSource, MailAnalyticsTotals},
    display_path,
};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};
use std::{fs, io::Write, path::Path};

const PURPOSE: &str = "(j.purpose IN ('first_outreach','outreach_followup') OR (?3 AND j.purpose='legacy_unclassified'))";

fn cohort() -> String {
    format!("WITH evidence AS (
        SELECT e.submitted_at,coalesce(c.customer_source,'') source,
               (SELECT min(i.received_at) FROM inbox_messages i WHERE
                 (i.reply_to_sent_id=e.id OR (i.reply_to_sent_id IS NULL AND i.thread_id='sent:' || e.id))
                 AND i.account_id=e.account_id AND i.contact_id=e.contact_id
                 AND i.message_role='correspondence' AND i.match_state IN ('thread','manual')
                 AND i.received_at BETWEEN e.submitted_at AND ?2) reply_at,
               EXISTS(SELECT 1 FROM mail_delivery_events b WHERE b.outbox_id=j.id AND b.action='failed') bounced,
               CASE WHEN a.archived_at IS NULL AND s.outcome='ok' AND ia.account_id IS NOT NULL THEN s.last_sync END sync_at
        FROM email_messages e JOIN email_outbox_jobs j ON j.id=e.outbox_id
        JOIN email_accounts a ON a.id=e.account_id
        LEFT JOIN contacts original ON original.id=e.contact_id
        LEFT JOIN contacts c ON c.id=coalesce(original.merged_into,original.id)
        LEFT JOIN imap_accounts ia ON ia.account_id=e.account_id
        LEFT JOIN imap_sync_state s ON s.account_id=e.account_id AND s.folder='INBOX'
        WHERE e.submitted_at BETWEEN ?1 AND ?2 AND {PURPOSE}
    ), cohort AS (
        SELECT *,CASE WHEN bounced THEN 'bounced' WHEN reply_at IS NOT NULL THEN 'replied'
                      WHEN sync_at>submitted_at THEN 'waiting' ELSE 'unsynced' END state FROM evidence
    )")
}

pub(crate) async fn report(
    pool: &SqlitePool,
    days: u16,
    include_legacy: bool,
) -> Result<MailAnalytics> {
    if !matches!(days, 7 | 30 | 90) {
        return Err("invalid_analytics_range");
    }
    let mut tx = pool.begin().await.map_err(|_| "database_unavailable")?;
    let (start, end): (i64, i64) = sqlx::query_as(
        "SELECT unixepoch(date('now','localtime',printf('-%d days',?)),'utc'),unixepoch()",
    )
    .bind(i64::from(days) - 1)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| "database_unavailable")?;
    let cohort = cohort();
    let totals_sql = format!("{cohort}, exceptions AS (
        SELECT count(*) FILTER(WHERE j.state='failed' AND j.outcome IS NOT 'cancelled_before_submission') failed,count(*) FILTER(WHERE j.state='delivery_unknown') unknown
        FROM email_outbox_jobs j WHERE j.state IN ('failed','delivery_unknown')
          AND coalesce(j.last_attempt_at,j.created_at) BETWEEN ?1 AND ?2 AND {PURPOSE}
    ) SELECT count(*) accepted,count(*) FILTER(WHERE state='replied') replied,
             count(*) FILTER(WHERE state='bounced') bounced,count(*) FILTER(WHERE state='waiting') waiting,
             count(*) FILTER(WHERE state='unsynced') unsynced,max(sync_at) latest_sync,
             (SELECT failed FROM exceptions) failed,(SELECT unknown FROM exceptions) unknown FROM cohort");
    let totals = sqlx::query(&totals_sql)
        .bind(start)
        .bind(end)
        .bind(include_legacy)
        .map(|row: SqliteRow| MailAnalyticsTotals {
            accepted: row.get("accepted"),
            replied: row.get("replied"),
            bounced: row.get("bounced"),
            waiting: row.get("waiting"),
            unsynced: row.get("unsynced"),
            latest_sync: row.get("latest_sync"),
            failed: row.get("failed"),
            unknown: row.get("unknown"),
        })
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| "database_unavailable")?;
    let trend_sql = format!("{cohort}, days(day) AS (
        SELECT date(?1,'unixepoch','localtime') UNION ALL SELECT date(day,'+1 day') FROM days WHERE day<date(?2,'unixepoch','localtime')
    ), submissions AS (
        SELECT date(submitted_at,'unixepoch','localtime') day,count(*) accepted FROM cohort GROUP BY 1
    ), replies AS (
        SELECT date(reply_at,'unixepoch','localtime') day,count(*) replied FROM cohort WHERE state='replied' GROUP BY 1
    ) SELECT days.day,coalesce(submissions.accepted,0) accepted,coalesce(replies.replied,0) replied
      FROM days LEFT JOIN submissions USING(day) LEFT JOIN replies USING(day) ORDER BY 1");
    let trend = sqlx::query(&trend_sql)
        .bind(start)
        .bind(end)
        .bind(include_legacy)
        .map(|row: SqliteRow| MailAnalyticsDay {
            day: row.get("day"),
            accepted: row.get("accepted"),
            replied: row.get("replied"),
        })
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| "database_unavailable")?;
    let sources_sql = format!("{cohort}, grouped AS (
        SELECT source,count(*) accepted,count(*) FILTER(WHERE state='replied') replied,
            count(*) FILTER(WHERE state='bounced') bounced FROM cohort GROUP BY source
    ), ranked AS (
        SELECT *,row_number() OVER(ORDER BY accepted DESC,source) position FROM grouped
    ) SELECT CASE WHEN position<=12 THEN source END source_label,sum(accepted) accepted,sum(replied) replied,sum(bounced) bounced
        FROM ranked GROUP BY source_label ORDER BY min(position)");
    let sources = sqlx::query(&sources_sql)
        .bind(start)
        .bind(end)
        .bind(include_legacy)
        .map(|row: SqliteRow| MailAnalyticsSource {
            source: row.get("source_label"),
            accepted: row.get("accepted"),
            replied: row.get("replied"),
            bounced: row.get("bounced"),
        })
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| "database_unavailable")?;
    tx.commit().await.map_err(|_| "database_unavailable")?;
    Ok(MailAnalytics {
        days,
        include_legacy,
        totals,
        trend,
        sources,
    })
}

pub(super) async fn export(
    pool: &SqlitePool,
    root: &Path,
    days: u16,
    include_legacy: bool,
) -> Result<Reply> {
    let data = report(pool, days, include_legacy).await?;
    let folder = root.join("exports");
    fs::create_dir_all(&folder).map_err(|_| "csv_export_failed")?;
    let filename = format!("mail-analytics-{}", uuid::Uuid::new_v4());
    let temporary = folder.join(format!("{filename}.partial"));
    let destination = folder.join(format!("{filename}.csv"));
    let result = fs::File::options()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .and_then(|file| write_csv(&data, file))
        .and_then(|()| fs::rename(&temporary, &destination));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        return Err("csv_export_failed");
    }
    Ok(Reply::File {
        path: display_path(&destination),
    })
}

pub(crate) async fn export_csv(
    pool: &SqlitePool,
    days: u16,
    include_legacy: bool,
) -> Result<Reply> {
    let data = report(pool, days, include_legacy).await?;
    let mut bytes = Vec::new();
    write_csv(&data, &mut bytes).map_err(|_| "csv_export_failed")?;
    let csv = String::from_utf8(bytes).map_err(|_| "csv_export_failed")?;
    Ok(Reply::MailAnalyticsCsv { csv })
}

fn write_csv(data: &MailAnalytics, mut output: impl Write) -> std::io::Result<()> {
    output.write_all(&[0xef, 0xbb, 0xbf])?;
    let mut writer = csv::Writer::from_writer(output);
    writer.write_record([
        "类别",
        "名称或日期",
        "SMTP 已接受",
        "确认有回复",
        "明确退信",
        "数值",
    ])?;
    let totals = &data.totals;
    writer.serialize((
        "汇总",
        "全部",
        totals.accepted,
        totals.replied,
        totals.bounced,
        "",
    ))?;
    for (label, value) in [
        ("最近天数", i64::from(data.days)),
        ("含历史未分类", i64::from(data.include_legacy)),
        ("已发未见回复", totals.waiting),
        ("待同步或同步未确认", totals.unsynced),
        ("提交失败", totals.failed),
        ("提交结果未知", totals.unknown),
    ] {
        writer.serialize(("汇总", label, "", "", "", value))?;
    }
    for day in &data.trend {
        writer.serialize(("逐日", &day.day, day.accepted, day.replied, "", ""))?;
    }
    for group in &data.sources {
        let label = match group.source.as_deref() {
            Some("") => "未填写来源",
            Some(source) => source,
            None => "其他来源（合计）",
        };
        writer.serialize((
            "来源",
            transfer::spreadsheet_safe(label.to_owned()),
            group.accepted,
            group.replied,
            group.bounced,
            "",
        ))?;
    }
    writer.flush()
}
