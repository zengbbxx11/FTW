use super::{Error, Result};
use ftw_ipc::crm::{ActivityInput, ContactInput, LeadInput};
use std::collections::HashSet;
use url::{Host, Url};

pub const STAGES: &[&str] = &[
    "prospect",
    "contacted",
    "replied",
    "requirements",
    "quote_sample",
    "negotiation",
    "won",
    "paused",
    "excluded",
];

pub const ACTIVITY_TYPES: &[&str] = &["note", "call", "meeting", "chat", "sample", "other"];

pub fn text(value: &mut String, name: &'static str, max: usize, required: bool) -> Result<()> {
    let trimmed = value.trim();
    if (required && trimmed.is_empty()) || trimmed.len() > max || trimmed.contains('\0') {
        return Err(Error::Invalid(name));
    }
    if trimmed.len() != value.len() {
        *value = trimmed.to_owned();
    }
    Ok(())
}

pub fn choice(value: &str, name: &'static str, valid: &[&str]) -> Result<()> {
    if valid.contains(&value) {
        Ok(())
    } else {
        Err(Error::Invalid(name))
    }
}

pub fn email(value: &str) -> Result<String> {
    let (local, domain) = value.split_once('@').ok_or(Error::Invalid("email"))?;
    if local.is_empty()
        || local.len() > 64
        || local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
        || !local
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".!#$%&'*+-/=?^_`{|}~".contains(&c))
    {
        return Err(Error::Invalid("email"));
    }
    match Host::parse(domain) {
        Ok(Host::Domain(domain))
            if domain.contains('.')
                && domain.split('.').all(|part| {
                    !part.is_empty()
                        && part.len() <= 63
                        && !part.starts_with('-')
                        && !part.ends_with('-')
                        && part.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
                }) =>
        {
            Ok(format!(
                "{}@{}",
                local.to_ascii_lowercase(),
                domain.to_ascii_lowercase()
            ))
        }
        _ => Err(Error::Invalid("email")),
    }
}

pub fn website(value: &str, field: &'static str) -> Result<String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    let parsed = Url::parse(value).map_err(|_| Error::Invalid(field))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(Error::Invalid(field));
    }
    let host = parsed
        .host_str()
        .ok_or(Error::Invalid(field))?
        .trim_end_matches('.');
    Ok(host.strip_prefix("www.").unwrap_or(host).to_lowercase())
}

fn timestamp(value: i64, field: &'static str) -> Result<()> {
    if !(-2208988800..=253402300799).contains(&value) {
        return Err(Error::Invalid(field));
    }
    Ok(())
}

pub fn contact(input: &mut ContactInput) -> Result<(String, Vec<String>)> {
    text(&mut input.display_name, "display_name", 300, true)?;
    for (field, value) in [
        ("company_name", &mut input.company_name),
        ("country_region", &mut input.country_region),
        ("customer_type", &mut input.customer_type),
        ("customer_source", &mut input.customer_source),
        ("customer_level", &mut input.customer_level),
        ("first_contact_channel", &mut input.first_contact_channel),
        ("primary_language", &mut input.primary_language),
        ("sample_status", &mut input.sample_status),
        ("bulk_order_status", &mut input.bulk_order_status),
        ("alibaba_buyer_id", &mut input.alibaba_buyer_id),
    ] {
        text(value, field, 300, false)?;
    }
    text(&mut input.main_need, "main_need", 4000, false)?;
    text(&mut input.notes, "notes", 4000, false)?;
    if input.stage.is_empty() {
        input.stage = "prospect".to_owned();
    }
    choice(&input.stage, "stage", STAGES)?;
    if let Some(first) = input.first_contact_at {
        timestamp(first, "first_contact_at")?;
    }
    if let Some(last) = input.last_contact_at {
        timestamp(last, "last_contact_at")?;
    }
    if matches!((input.first_contact_at, input.last_contact_at), (Some(first), Some(last)) if first > last)
    {
        return Err(Error::Invalid("last_contact_at"));
    }
    text(&mut input.website_url, "website_url", 1000, false)?;
    let domain = website(&input.website_url, "website_url")?;

    if input.methods.len() > 12 || input.interests.len() > 20 {
        return Err(Error::Invalid("collection_limit"));
    }
    let mut seen = HashSet::new();
    let mut primary = HashSet::new();
    let mut normalized = Vec::with_capacity(input.methods.len());
    for method in &mut input.methods {
        choice(
            &method.kind,
            "method_kind",
            &["email", "phone", "whatsapp", "wechat", "other"],
        )?;
        text(&mut method.value, "method_value", 300, true)?;
        text(&mut method.label, "method_label", 200, false)?;
        let value = if method.kind == "email" {
            email(&method.value)?
        } else {
            method.value.to_lowercase()
        };
        if !seen.insert((method.kind.clone(), value.clone())) {
            return Err(Error::Invalid("duplicate_method"));
        }
        if method.is_primary && !primary.insert(method.kind.clone()) {
            return Err(Error::Invalid("primary_method"));
        }
        normalized.push(value);
    }
    for interest in &mut input.interests {
        text(&mut interest.product_text, "product_text", 300, true)?;
        text(&mut interest.requirement, "requirement", 2000, false)?;
        if interest
            .intent_quantity
            .is_some_and(|qty| !(0..=9_000_000_000_000_000).contains(&qty))
        {
            return Err(Error::Invalid("intent_quantity"));
        }
    }
    Ok((domain, normalized))
}

pub fn lead(input: &mut LeadInput) -> Result<(String, String)> {
    text(&mut input.display_name, "display_name", 300, true)?;
    text(&mut input.company_name, "company_name", 300, false)?;
    text(&mut input.country_region, "country_region", 300, false)?;
    text(&mut input.email, "email", 300, false)?;
    text(&mut input.website_url, "website_url", 1000, false)?;
    text(&mut input.business_summary, "business_summary", 3000, false)?;
    text(&mut input.research_notes, "research_notes", 4000, false)?;
    if input.confidence.is_some_and(|value| value > 100) {
        return Err(Error::Invalid("confidence"));
    }
    if input.source_urls.len() > 12 {
        return Err(Error::Invalid("source_urls"));
    }
    for link in &mut input.source_urls {
        text(link, "source_urls", 1000, true)?;
        website(link, "source_urls")?;
    }
    let email = if input.email.is_empty() {
        String::new()
    } else {
        email(&input.email)?
    };
    Ok((website(&input.website_url, "website_url")?, email))
}

pub fn activity(input: &mut ActivityInput) -> Result<()> {
    choice(&input.event_type, "event_type", ACTIVITY_TYPES)?;
    text(&mut input.summary, "summary", 500, true)?;
    text(&mut input.details, "details", 4000, false)?;
    timestamp(input.occurred_at, "occurred_at")
}
