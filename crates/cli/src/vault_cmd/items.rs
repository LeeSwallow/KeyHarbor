//! `vault add`, `edit`, `revert`, `mv` and `rm`: parse the arguments into a new item or a
//! change, then hand it to the guard (which checks descriptions and confirms weakening before
//! storing) or to core.

use super::{
    Toggle,
    input::{FieldInput, Fill, collect_fields, field_name, item_name, origins},
    kind_parser,
    output::warn_common_values,
    status_parser, templates,
};
use crate::{
    args::{DescriptionArgs, ExpiryArgs},
    compose::App,
    tty_confirm::TtyConfirm,
};
use anyhow::{Context, Result, anyhow, bail};
use clap::Args;
use keyharbor_core::{
    ItemChange, ItemDetails, ItemSource, ItemStatus, ItemSummary, NewItem, Vault,
};
use keyharbor_domain::{
    tag::Tag,
    vault::{FieldName, ItemKind, ItemName, placeholder},
};
use keyharbor_guard::items;

#[derive(Args)]
pub struct AddArgs {
    name: String,
    #[arg(long, value_parser = kind_parser())]
    kind: ItemKind,
    #[command(flatten)]
    input: FieldInput,
    #[arg(long = "origin")]
    origins: Vec<String>,
    #[arg(long)]
    no_origin_binding: bool,
    /// Whether `keyharbor run` may inject the item (default: on for secret and env)
    #[arg(long, value_enum)]
    run_allowed: Option<Toggle>,
    /// Whether agents may type the login's one-time codes while agent_otp is on (default: on)
    #[arg(long, value_enum)]
    otp_allowed: Option<Toggle>,
    #[command(flatten)]
    expiry: ExpiryArgs,
    #[command(flatten)]
    about: DescriptionArgs,
    /// A label to find the item by, e.g. env:prod (repeat for more)
    #[arg(long = "tag")]
    tags: Vec<String>,
    /// A field that is not secret (PORT): its value shows in listings, unmasked
    #[arg(long = "plain", value_name = "FIELD")]
    plain: Vec<String>,
    /// Start a credential from a template: its fields are asked for, its plain ones marked
    #[arg(long, value_name = "NAME")]
    template: Option<String>,
    #[arg(long)]
    draft: bool,
}

#[derive(Args)]
pub struct EditArgs {
    name: String,
    #[command(flatten)]
    input: FieldInput,
    #[arg(long = "unset")]
    unset: Vec<String>,
    #[arg(long = "origin")]
    origins: Vec<String>,
    #[arg(long)]
    clear_origins: bool,
    #[arg(long, value_enum)]
    origin_binding: Option<Toggle>,
    /// Whether `keyharbor run` may inject the item
    #[arg(long, value_enum)]
    run_allowed: Option<Toggle>,
    /// Whether agents may type the login's one-time codes while agent_otp is on (on asks first)
    #[arg(long, value_enum)]
    otp_allowed: Option<Toggle>,
    #[command(flatten)]
    expiry: ExpiryArgs,
    #[command(flatten)]
    about: DescriptionArgs,
    /// Add a tag (repeat for more)
    #[arg(long = "tag")]
    tags: Vec<String>,
    /// Remove a tag
    #[arg(long = "untag")]
    untag: Vec<String>,
    /// Mark a field not secret: its value shows in listings, unmasked (asks first)
    #[arg(long = "plain", value_name = "FIELD")]
    plain: Vec<String>,
    /// Mark a field secret again
    #[arg(long = "secret", value_name = "FIELD")]
    secret: Vec<String>,
    #[arg(long, value_parser = status_parser())]
    status: Option<ItemStatus>,
}

fn field_names(kind: ItemKind, values: &[String]) -> Result<Vec<FieldName>> {
    values.iter().map(|f| field_name(kind, f)).collect()
}

fn tags(values: &[String]) -> Result<Vec<Tag>> {
    values
        .iter()
        .map(|t| Tag::parse(t).map_err(|e| anyhow!("{e}")))
        .collect()
}

/// The fields a template gives a new item, and which of them are plain; none without one.
fn template_fields(
    app: &App,
    kind: ItemKind,
    template: Option<&str>,
) -> Result<(Vec<FieldName>, Vec<FieldName>)> {
    let Some(name) = template else {
        return Ok(Default::default());
    };
    let template = templates::for_new_item(app, kind, name)?;
    let fields = template.fields().iter().map(|f| f.name.clone()).collect();
    Ok((fields, template.plain_fields()))
}

pub fn add(app: &App, args: &AddArgs) -> Result<()> {
    let (kind, name) = (args.kind, item_name(&args.name)?);
    let (description, field_descriptions) = (args.about.item()?, args.about.new_fields(kind)?);
    let (template_fields, template_plain) = template_fields(app, kind, args.template.as_deref())?;
    let mut plain = field_names(kind, &args.plain)?;
    plain.extend(template_plain.iter().cloned());
    let fill = Fill {
        kind_secrets: true,
        template: &template_fields,
        plain: &plain,
    };
    let fields = collect_fields(kind, &args.input, fill)?;
    let vault = app.vault()?;
    // A template field left empty is not stored, so the template cannot mark it plain.
    plain.retain(|p| !template_plain.contains(p) || fields.iter().any(|(f, _)| f == p));
    let item = NewItem {
        name: name.clone(),
        kind,
        fields,
        origins: origins(&args.origins)?,
        origin_binding: args.no_origin_binding.then_some(false),
        status: if args.draft {
            ItemStatus::Draft
        } else {
            ItemStatus::Active
        },
        source: ItemSource::User,
        details: ItemDetails {
            run_allowed: args.run_allowed.map(Toggle::on),
            expiry: args.expiry.for_new_item(app.clock())?,
            description: description.flatten(),
            field_descriptions,
            tags: tags(&args.tags)?,
            plain_fields: plain,
            otp_allowed: args.otp_allowed.map(Toggle::on),
        },
    };
    let warnings = items::add(&vault, item, &args.about.texts())?;
    println!("added\t{}", name.as_str());
    warn_common_values(&warnings);
    Ok(())
}

pub fn edit(app: &App, args: &EditArgs) -> Result<()> {
    let name = item_name(&args.name)?;
    let vault = app.vault()?;
    let stored = stored_item(&vault, &name)?;
    let kind = stored.kind;
    let (make_plain, make_secret) = (
        field_names(kind, &args.plain)?,
        field_names(kind, &args.secret)?,
    );
    // Plain after the change: a field the person names may then come from `--set` (R-48).
    let plain: Vec<FieldName> = stored
        .plain_values
        .keys()
        .filter_map(|f| FieldName::parse(f).ok())
        .chain(make_plain.iter().cloned())
        .filter(|f| !make_secret.contains(f))
        .collect();
    let fill = Fill {
        plain: &plain,
        ..Fill::default()
    };
    let new_origins = if args.clear_origins {
        Some(Vec::new())
    } else if args.origins.is_empty() {
        None
    } else {
        Some(origins(&args.origins)?)
    };
    let change = ItemChange {
        set_fields: collect_fields(kind, &args.input, fill)?,
        remove_fields: field_names(kind, &args.unset)?,
        origins: new_origins,
        origin_binding: args.origin_binding.map(Toggle::on),
        run_allowed: args.run_allowed.map(Toggle::on),
        expiry: args.expiry.for_change(app.clock())?,
        description: args.about.item()?,
        field_descriptions: args.about.fields(kind)?,
        status: args.status,
        add_tags: tags(&args.tags)?,
        remove_tags: tags(&args.untag)?,
        make_plain,
        make_secret,
        otp_allowed: args.otp_allowed.map(Toggle::on),
    };
    let warnings = items::edit(&vault, &TtyConfirm, &name, change, &args.about.texts())?;
    println!("updated\t{}", name.as_str());
    warn_common_values(&warnings);
    Ok(())
}

/// A stored item without values, to read its kind and plain fields.
fn stored_item(vault: &Vault, name: &ItemName) -> Result<ItemSummary> {
    vault
        .list()?
        .into_iter()
        .find(|i| i.name == name.as_str())
        .context("item not found")
}

pub fn revert(app: &App, name: &str, fields: &[String]) -> Result<()> {
    let name = item_name(name)?;
    let vault = app.vault()?;
    let fields = field_names(stored_item(&vault, &name)?.kind, fields)?;
    for field in vault.revert(&name, &fields)? {
        println!("reverted\t{}\t{field}", name.as_str());
    }
    Ok(())
}

pub fn rename(app: &App, from: &str, to: &str) -> Result<()> {
    let (from, to) = (item_name(from)?, item_name(to)?);
    keyharbor_core::rename::rename_item(&*app.store()?, &app.vault()?, &from, &to)?;
    println!("renamed\t{}\t{}", from.as_str(), to.as_str());
    println!(
        "placeholders now read {}; update agents that quote the old name",
        placeholder(to.as_str(), "<field>")
    );
    Ok(())
}

pub fn remove(app: &App, name: &str) -> Result<()> {
    let name = item_name(name)?;
    if !app.vault()?.remove(&name)? {
        bail!("item not found");
    }
    println!("removed\t{}", name.as_str());
    Ok(())
}
