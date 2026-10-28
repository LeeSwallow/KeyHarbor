//! Vault tools: metadata and placeholders only, never a value.

use crate::server::{Server, outcome};
use keyharbor_core::{CatalogEntry, ItemSummary, catalog};
use rmcp::{model::CallToolResult, tool, tool_router};
use serde_json::{Value, json};

/// A verified item as agents see it.
fn summary_json(item: &ItemSummary) -> Value {
    let placeholders: Vec<String> = item
        .fields
        .iter()
        .map(|f| format!("{{{{secret:{}.{f}}}}}", item.name))
        .collect();
    json!({
        "name": item.name,
        "kind": item.kind.as_str(),
        "domain": item.domain.as_str(),
        "field_descriptions": item.field_descriptions,
        "fields": item.fields,
        "placeholders": placeholders,
        "origins": item.origins,
        "origin_binding": item.origin_binding,
        "description": item.description,
        "run_allowed": item.run_allowed,
        "expires_at": item.expires_at.map(|e| e.to_string()),
        "expired": item.expired,
        "status": item.status.as_str(),
        "source": item.source.as_str(),
        "tampered": false,
    })
}

fn entry_json(entry: &CatalogEntry) -> Value {
    match entry {
        CatalogEntry::Verified(item) => summary_json(item),
        CatalogEntry::Tampered { name } => json!({"name": name, "tampered": true}),
    }
}

#[tool_router(router = vault_tools, vis = "pub(crate)")]
impl Server {
    #[tool(
        description = "List vault items: name, kind, domain, field names, descriptions (what the item and each field is for), origins, run permission, expiry, status and the placeholder for each field (use it in browser_fill or browser_type). An item whose stored metadata fails verification shows only its name and tampered: true; it cannot be used until the user fixes it. Values are never returned."
    )]
    async fn vault_list(&self) -> CallToolResult {
        outcome(
            self.blocking(|inner| {
                let entries = catalog(&**inner.store()?, || (inner.open_vault)())?;
                Ok(Value::Array(entries.iter().map(entry_json).collect()))
            })
            .await,
        )
    }
}
