use super::{contacts, Actor, Error, Result};
use ftw_ipc::{
    crm::{ContactQuery, Source},
    fields::Kind,
    views::{Filter, Logic, Operator},
};
use serde_json::Value;
use sqlx::{QueryBuilder, Sqlite, SqliteConnection};
use std::collections::{HashMap, HashSet};

pub struct Catalog(HashMap<String, Kind>);

pub(super) async fn catalog_on(db: &mut SqliteConnection, actor: Actor<'_>) -> Result<Catalog> {
    let desktop = matches!(actor.source, Source::Manual);
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT id,kind FROM crm_fields WHERE NOT archived AND (? OR agent_access<>'hidden')",
    )
    .bind(desktop)
    .fetch_all(db)
    .await?;
    let mut fields = HashMap::new();
    for (id, kind) in rows {
        let kind = serde_json::from_value(Value::String(kind))
            .map_err(|_| Error::Invalid("field_kind"))?;
        fields.insert(id, kind);
    }
    Ok(Catalog(fields))
}

struct Builtin(&'static str, Kind);

fn builtin(field: &str) -> Option<Builtin> {
    let (sql,kind) = match field {
        "name" => ("c.display_name",Kind::Text),
        "company" => ("c.company_name",Kind::Text),
        "country" => ("c.country_region",Kind::Text),
        "stage" => ("c.stage",Kind::SingleSelect),
        "level" => ("c.customer_level",Kind::SingleSelect),
        "sample" => ("c.sample_status",Kind::SingleSelect),
        "customer_type" => ("c.customer_type",Kind::SingleSelect),
        "customer_source" => ("c.customer_source",Kind::SingleSelect),
        "bulk_order_status" => ("c.bulk_order_status",Kind::SingleSelect),
        "source" => ("c.source",Kind::Text),
        "email" => ("(SELECT value FROM contact_methods m WHERE m.contact_id=c.id AND m.kind='email' ORDER BY m.is_primary DESC,m.position LIMIT 1)",Kind::Text),
        "product" => ("(SELECT group_concat(product_text,' · ') FROM customer_interests i WHERE i.contact_id=c.id)",Kind::Text),
        "last_contact" => ("dates.effective_last",Kind::Number),
        "updated" => ("c.updated_at",Kind::Number),
        "mail_status" => ("outreach.mail_status",Kind::SingleSelect),
        "mail_last_sent" => ("outreach.last_sent_at",Kind::Number),
        "mail_last_reply" => ("outreach.last_reply_at",Kind::Number),
        "mail_count" => ("outreach.outreach_count",Kind::Number),
        "mail_subject" => ("outreach.recent_subject",Kind::Text),
        _ => return None,
    };
    Some(Builtin(sql, kind))
}

impl Catalog {
    pub fn kind(&self, field: &str) -> Result<Kind> {
        builtin(field)
            .map(|definition| definition.1)
            .or_else(|| self.0.get(field).copied())
            .ok_or(Error::Invalid("query_field"))
    }

    pub fn group_kind(&self, field: &str) -> Result<()> {
        if field == "stage" || self.0.get(field) == Some(&Kind::SingleSelect) {
            Ok(())
        } else {
            Err(Error::Invalid("query_group_field"))
        }
    }
}

const MAX_RULES: usize = 30;
pub const MAX_PROJECTED_FIELDS: usize = 12;

pub fn conditions(
    builder: &mut QueryBuilder<'_, Sqlite>,
    query: &ContactQuery,
    catalog: &Catalog,
) -> Result<()> {
    if query.custom_columns.len() > MAX_PROJECTED_FIELDS {
        return Err(Error::Invalid("query_columns"));
    }
    if query
        .custom_columns
        .iter()
        .any(|id| !catalog.0.contains_key(id))
    {
        return Err(Error::Invalid("query_field"));
    }
    if let Some(filter) = &query.filter {
        builder.push(" AND ");
        expression(builder, filter, catalog, 0, &mut 0)?;
    }
    if let Some((field, value)) = &query.group {
        catalog.group_kind(field)?;
        builder.push(" AND ");
        if let Some(value) = value {
            let operand = Value::String(value.clone());
            rule(builder, catalog, field, Operator::Eq, Some(&operand))?;
        } else {
            rule(builder, catalog, field, Operator::Empty, None)?;
        }
    }
    Ok(())
}

pub fn sorting(
    builder: &mut QueryBuilder<'_, Sqlite>,
    query: &ContactQuery,
    catalog: &Catalog,
) -> Result<()> {
    if query.order.len() > 3 {
        return Err(Error::Invalid("query_order"));
    }
    let mut seen = HashSet::new();
    for (index, order) in query.order.iter().enumerate() {
        if !seen.insert(&order.field) || catalog.kind(&order.field)? == Kind::MultiSelect {
            return Err(Error::Invalid("query_order"));
        }
        if index != 0 {
            builder.push(",");
        }
        column(builder, catalog, &order.field)?;
        if matches!(catalog.kind(&order.field)?, Kind::Text | Kind::Url) {
            builder.push(" COLLATE NOCASE");
        }
        builder.push(if order.descending {
            " DESC NULLS LAST"
        } else {
            " ASC NULLS LAST"
        });
    }
    Ok(())
}

pub fn page_size(query: &ContactQuery) -> Result<i64> {
    match query.page_size {
        None | Some(20) => Ok(20),
        Some(50) => Ok(50),
        Some(100) => Ok(100),
        _ => Err(Error::Invalid("query_page_size")),
    }
}

fn expression(
    builder: &mut QueryBuilder<'_, Sqlite>,
    node: &Filter,
    catalog: &Catalog,
    depth: usize,
    rules: &mut usize,
) -> Result<()> {
    *rules += 1;
    if *rules > MAX_RULES || depth > 3 {
        return Err(Error::Invalid("query_depth"));
    }
    match node {
        Filter::Group { logic, children } => {
            if children.is_empty() {
                return Err(Error::Invalid("query_empty_group"));
            }
            builder.push("(");
            for (index, child) in children.iter().enumerate() {
                if index != 0 {
                    builder.push(match logic {
                        Logic::And => " AND ",
                        Logic::Or => " OR ",
                    });
                }
                expression(builder, child, catalog, depth + 1, rules)?;
            }
            builder.push(")");
            Ok(())
        }
        Filter::Rule {
            field,
            operator,
            value,
        } => rule(builder, catalog, field, *operator, value.as_ref()),
    }
}

fn column(builder: &mut QueryBuilder<'_, Sqlite>, catalog: &Catalog, field: &str) -> Result<()> {
    if let Some(Builtin(sql, _)) = builtin(field) {
        if let Some(state) = match field {
            "level" => Some("customer_level"),
            "sample" => Some("sample_status"),
            "customer_type" => Some("customer_type"),
            "customer_source" => Some("customer_source"),
            "bulk_order_status" => Some("bulk_order_status"),
            _ => None,
        } {
            builder.push(contacts::state_sql(state));
        } else {
            builder.push(sql);
        }
    } else {
        let value = match catalog.kind(field)? {
            Kind::Text | Kind::Url | Kind::Date => "text_value",
            Kind::Number => "number_value",
            Kind::Checkbox => "bool_value",
            Kind::SingleSelect => "option_id",
            Kind::MultiSelect => return Err(Error::Invalid("query_operator")),
        };
        builder
            .push("(SELECT ")
            .push(value)
            .push(" FROM crm_field_values WHERE contact_id=c.id AND field_id=")
            .push_bind(field.to_owned())
            .push(")");
    }
    Ok(())
}

fn rule(
    builder: &mut QueryBuilder<'_, Sqlite>,
    catalog: &Catalog,
    field: &str,
    op: Operator,
    value: Option<&Value>,
) -> Result<()> {
    let kind = catalog.kind(field)?;
    if kind == Kind::MultiSelect {
        match op {
            Operator::Contains => {
                let option = text(value)?;
                builder.push("EXISTS(SELECT 1 FROM crm_field_multi_values WHERE contact_id=c.id AND field_id=")
                    .push_bind(field.to_owned()).push(" AND option_id=").push_bind(option).push(")");
            }
            Operator::Empty => {
                builder.push("NOT EXISTS(SELECT 1 FROM crm_field_multi_values WHERE contact_id=c.id AND field_id=")
                    .push_bind(field.to_owned()).push(")");
            }
            _ => return Err(Error::Invalid("query_operator")),
        }
        return Ok(());
    }
    match op {
        Operator::Empty => {
            builder.push("NULLIF(");
            column(builder, catalog, field)?;
            builder.push(",'') IS NULL");
        }
        Operator::Contains if matches!(kind, Kind::Text | Kind::Url) => {
            let term = text(value)?;
            builder.push("instr(lower(");
            column(builder, catalog, field)?;
            builder.push("),lower(").push_bind(term).push("))>0");
        }
        Operator::WithinDays | Operator::OlderThanDays
            if matches!(
                field,
                "last_contact" | "updated" | "mail_last_sent" | "mail_last_reply"
            ) =>
        {
            let days = value
                .and_then(Value::as_u64)
                .filter(|days| (1..=3650).contains(days))
                .ok_or(Error::Invalid("query_value"))?;
            column(builder, catalog, field)?;
            builder
                .push(if matches!(op, Operator::WithinDays) {
                    ">="
                } else {
                    "<"
                })
                .push("(unixepoch()-")
                .push_bind(days as i64 * 86_400)
                .push(")");
        }
        Operator::Eq | Operator::NotEq | Operator::Gte | Operator::Lte => {
            let comparison = match op {
                Operator::Eq => "=",
                Operator::NotEq => "<>",
                Operator::Gte if matches!(kind, Kind::Number | Kind::Date) => ">=",
                Operator::Lte if matches!(kind, Kind::Number | Kind::Date) => "<=",
                _ => return Err(Error::Invalid("query_operator")),
            };
            column(builder, catalog, field)?;
            builder.push(comparison);
            match kind {
                Kind::Number => {
                    builder.push_bind(
                        value
                            .and_then(Value::as_f64)
                            .filter(|n| n.is_finite())
                            .ok_or(Error::Invalid("query_value"))?,
                    );
                }
                Kind::Checkbox => {
                    builder.push_bind(
                        value
                            .and_then(Value::as_bool)
                            .ok_or(Error::Invalid("query_value"))?,
                    );
                }
                Kind::Date => {
                    let date = text(value)?;
                    if !super::fields::valid_date(&date) {
                        return Err(Error::Invalid("query_value"));
                    }
                    builder.push_bind(date);
                }
                _ => {
                    builder.push_bind(text(value)?).push(" COLLATE NOCASE");
                }
            }
        }
        _ => return Err(Error::Invalid("query_operator")),
    }
    Ok(())
}

fn text(value: Option<&Value>) -> Result<String> {
    let value = value
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 300 && !s.contains('\0'))
        .ok_or(Error::Invalid("query_value"))?;
    Ok(value.to_owned())
}
