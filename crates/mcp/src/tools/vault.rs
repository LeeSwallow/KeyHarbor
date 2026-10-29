//! Vault tools: metadata and placeholders only, never a value. Agents add items only as
//! drafts that never replace an existing item (08 §8.2–§8.3).

use crate::server::{Server, fail, outcome};
use anyhow::{Context, Result, anyhow, bail};
use keyharbor_core::{CatalogEntry, DraftItem, GeneratedLogin, ItemSource, ItemSummary, catalog};
use keyharbor_domain::{
    password::{DEFAULT_LENGTH, PasswordRule},
    vault::{FieldName, InvalidValue, ItemKind, ItemName, Origin, placeholder},
};
use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

#[derive(Deserialize, schemars::JsonSchema)]
struct PasswordRuleArgs {
    /// 8 to 128; default 20.
    length: Option<usize>,
    /// Include symbols (!#$%&*+-=?@^_~); default true.
    symbols: Option<bool>,
    /// Characters the site does not accept.
    #[serde(default)]
    forbid: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct Generate {
    /// Item name (1–64 letters, digits, - or _). A taken name gets a draft named <name>~draft-N.
    name: String,
    /// Site origin without a path, e.g. https://shop.example.
    origin: String,
    /// Username or email to store with the password. Not a secret.
    username: Option<String>,
    password_rule: Option<PasswordRuleArgs>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct Put {
    /// Item name (1–64 letters, digits, - or _). A taken name gets a draft named <name>~draft-N.
    name: String,
    /// login, secret, env, card, identity or address.
    kind: String,
    /// Field name → value, e.g. {"api_key": "..."} for a key you just created.
    fields: BTreeMap<String, String>,
    /// Origins the item is for, e.g. ["https://github.com"].
    #[serde(default)]
    origins: Vec<String>,
}

/// What a new draft looks like to the agent: its name and placeholders, never a value.
fn draft_json(name: &ItemName, fields: impl Iterator<Item = String>) -> Value {
    let placeholders: Vec<String> = fields.map(|f| placeholder(name.as_str(), &f)).collect();
    json!({"name": name.as_str(), "draft": true, "placeholders": placeholders})
}

fn invalid(error: InvalidValue) -> anyhow::Error {
    anyhow!("{error}")
}

fn rule(args: Option<PasswordRuleArgs>) -> Result<PasswordRule> {
    let Some(args) = args else {
        return Ok(PasswordRule::default());
    };
    PasswordRule::new(
        args.length.unwrap_or(DEFAULT_LENGTH),
        args.symbols.unwrap_or(true),
        &args.forbid,
    )
    .map_err(invalid)
}

fn draft_item(put: Put) -> Result<DraftItem> {
    let kind = ItemKind::parse(&put.kind).context("unknown kind")?;
    let fields = put
        .fields
        .into_iter()
        .map(|(field, value)| {
            Ok((
                FieldName::parse(&field).map_err(invalid)?,
                Zeroizing::new(value),
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    if fields.is_empty() {
        bail!("an item needs at least one field");
    }
    Ok(DraftItem {
        name: ItemName::parse_new(&put.name).map_err(invalid)?,
        kind,
        fields,
        origins: put
            .origins
            .iter()
            .map(|o| Origin::parse(o).map_err(invalid))
            .collect::<Result<_>>()?,
        source: ItemSource::Agent,
    })
}
/// A verified item as agents see it.
fn summary_json(item: &ItemSummary) -> Value {
    let placeholders: Vec<String> = item
        .fields
        .iter()
        .map(|f| placeholder(&item.name, f))
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

    #[tool(
        description = "Create a login with a new random password for a sign-up or password change form, without seeing it. Saved as a draft (a taken name gets <name>~draft-N; the existing item is not changed). Type it with the returned placeholders; the draft is confirmed when you record site_state_set logged_in or the user confirms it."
    )]
    async fn vault_generate(&self, Parameters(args): Parameters<Generate>) -> CallToolResult {
        let login = match (|| {
            Ok::<_, anyhow::Error>(GeneratedLogin {
                name: ItemName::parse_new(&args.name).map_err(invalid)?,
                origin: Origin::parse(&args.origin).map_err(invalid)?,
                username: args.username.map(Zeroizing::new),
                rule: rule(args.password_rule)?,
            })
        })() {
            Ok(login) => login,
            Err(error) => return fail(error.to_string()),
        };
        let fields = ["username", "password"]
            .into_iter()
            .filter(|f| *f == "password" || login.username.is_some())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        outcome(
            self.blocking(move |inner| {
                let vault = (inner.open_vault)()?;
                let name = vault.generate_login(login, inner.agent())?;
                Ok(draft_json(&name, fields.into_iter()))
            })
            .await,
        )
    }

    #[tool(
        description = "Save values you already have (e.g. an API key a site just showed you) as a draft item. A taken name gets <name>~draft-N; the existing item is not changed. Returns placeholders, never the values. The user confirms or discards drafts."
    )]
    async fn vault_put(&self, Parameters(args): Parameters<Put>) -> CallToolResult {
        let item = match draft_item(args) {
            Ok(item) => item,
            Err(error) => return fail(error.to_string()),
        };
        let fields: Vec<String> = item
            .fields
            .iter()
            .map(|(f, _)| f.as_str().to_owned())
            .collect();
        outcome(
            self.blocking(move |inner| {
                let vault = (inner.open_vault)()?;
                let name = vault.add_draft(item, inner.agent())?;
                Ok(draft_json(&name, fields.into_iter()))
            })
            .await,
        )
    }
}
