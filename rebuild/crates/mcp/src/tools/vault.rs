//! Vault tools: metadata and placeholders only, never a value.

use crate::server::{Server, outcome};
use keyharbor_domain::{
    ports::VaultStore,
    vault::{Domain, ItemKind},
};
use keyharbor_sqlite::SqliteStore;
use rmcp::{model::CallToolResult, tool, tool_router};
use serde_json::{Value, json};

#[tool_router(router = vault_tools, vis = "pub(crate)")]
impl Server {
    #[tool(
        description = "List vault items: name, kind, domain, field names, origins, status and the placeholder for each field (use it in browser_fill or browser_type). Values are never returned."
    )]
    async fn vault_list(&self) -> CallToolResult {
        outcome(
            self.blocking(|inner| {
                let store = SqliteStore::open(&inner.data)?;
                let mut items = Vec::new();
                for row in store.item_list()? {
                    let kind = ItemKind::parse(&row.kind);
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
                        "fields": fields,
                        "placeholders": placeholders,
                        "origins": row.origins,
                        "origin_binding": row.origin_binding,
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
