//! How items are printed: a tab-separated line or a JSON object, never a value.

use keyharbor_core::ItemSummary;

pub fn summary_line(item: &ItemSummary) -> String {
    format!(
        "{}\t{}\t{}\t{}{}\t{}\t{}",
        item.name,
        item.kind.as_str(),
        item.domain.as_str(),
        item.status.as_str(),
        if item.tampered { " (tampered)" } else { "" },
        item.origins.join(","),
        item.fields.join(","),
    ) + &expiry_column(item)
        + &tags_column(item)
}

/// `\ttags a,b`, or nothing for an untagged item.
fn tags_column(item: &ItemSummary) -> String {
    if item.tags.is_empty() {
        String::new()
    } else {
        format!("\ttags {}", item.tags.join(","))
    }
}

/// `\texpires 2027-01-31`, `\texpired`, or nothing for an item that never expires.
fn expiry_column(item: &ItemSummary) -> String {
    match item.expires_at {
        Some(_) if item.expired => "\texpired".to_owned(),
        Some(date) => format!("\texpires {date}"),
        None => String::new(),
    }
}

pub fn summary_json(item: &ItemSummary) -> serde_json::Value {
    serde_json::json!({
        "name": item.name,
        "kind": item.kind.as_str(),
        "domain": item.domain.as_str(),
        "fields": item.fields,
        "origins": item.origins,
        "origin_binding": item.origin_binding,
        "run_allowed": item.run_allowed,
        "expires_at": item.expires_at.map(|e| e.to_string()),
        "description": item.description,
        "field_descriptions": item.field_descriptions,
        "tags": item.tags,
        "plain_values": item.plain_values,
        "expired": item.expired,
        "status": item.status.as_str(),
        "source": item.source.as_str(),
        "updated_at": item.updated_at,
        "tampered": item.tampered,
    })
}
