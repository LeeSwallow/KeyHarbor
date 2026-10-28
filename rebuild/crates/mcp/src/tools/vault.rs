//! Vault tools: metadata and placeholders only, never a value.

use crate::server::{Server, outcome};
use keyharbor_core::effective_descriptions;
use keyharbor_domain::{
    date::Expiry,
    ports::VaultStore,
    vault::{Domain, ItemKind},
};
use keyharbor_sqlite::SqliteStore;
use rmcp::{model::CallToolResult, tool, tool_router};
use serde_json::{Value, json};

/// Whether a stored expiry date has passed; an unreadable one counts as expired.
fn expired(expires_at: Option<&str>) -> bool {
    expires_at.is_some_and(|date| {
        Expiry::parse(date).map_or(true, |e| e.is_expired(keyharbor_core::today()))
    })
}

#[tool_router(router = vault_tools, vis = "pub(crate)")]
impl Server {
    #[tool(
        description = "List vault items: name, kind, domain, field names, descriptions (what the item and each field is for), origins, run permission, expiry, status and the placeholder for each field (use it in browser_fill or browser_type). Values are never returned."
    )]
    async fn vault_list(&self) -> CallToolResult {
        outcome(
            self.blocking(|inner| {
                let store = SqliteStore::open(&inner.data)?;
                let mut items = Vec::new();
                for row in store.item_list()? {
                    let kind = ItemKind::parse(&row.kind);
                    let described = |fields: &[String]| {
                        kind.map(|k| effective_descriptions(k, fields, &row.field_descriptions))
                    };
                    let fields: Vec<String> = store
                        .item_get(&row.name)?
                        .map(|(_, fields)| fields.into_iter().map(|f| f.field).collect())
                        .unwrap_or_default();
                    let placeholders: Vec<String> = fields
                        .iter()
                        .map(|f| format!("{{{{secret:{}.{f}}}}}", row.name))
                        .collect();
                    items.push(json!({
                        "name": row.name,
                        "kind": kind.map(ItemKind::as_str),
                        "domain": kind.map(ItemKind::domain).map(Domain::as_str),
                        "field_descriptions": described(&fields),
                        "fields": fields,
                        "placeholders": placeholders,
                        "origins": row.origins,
                        "origin_binding": row.origin_binding,
                        "description": row.description,
                        "run_allowed": row.run_allowed,
                        "expires_at": row.expires_at,
                        "expired": expired(row.expires_at.as_deref()),
                        "status": row.status,
                        "source": row.source,
                    }));
                }
                Ok(Value::Array(items))
            })
            .await,
        )
    }
}
