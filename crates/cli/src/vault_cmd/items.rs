//! `vault add` and `vault edit`: parse the arguments into a new item or a change, then hand
//! it to the guard, which checks descriptions and confirms weakening before storing.

use super::{
    StatusArg, Toggle,
    input::{FieldInput, collect_fields, field_name, item_name, origins},
};
use crate::{
    compose::App, description_args::DescriptionArgs, expiry_args::ExpiryArgs,
    tty_confirm::TtyConfirm,
};
use anyhow::{Context, Result, anyhow};
use keyharbor_core::{ItemChange, ItemDetails, ItemSource, ItemStatus, NewItem};
use keyharbor_domain::{tag::Tag, vault::ItemKind};
use keyharbor_guard::items;

pub struct AddArgs {
    pub name: String,
    pub kind: ItemKind,
    pub input: FieldInput,
    pub origins: Vec<String>,
    pub no_origin_binding: bool,
    pub run_allowed: Option<Toggle>,
    pub expiry: ExpiryArgs,
    pub about: DescriptionArgs,
    pub tags: Vec<String>,
    pub plain: Vec<String>,
    pub draft: bool,
}

pub struct EditArgs {
    pub name: String,
    pub input: FieldInput,
    pub unset: Vec<String>,
    pub origins: Vec<String>,
    pub clear_origins: bool,
    pub origin_binding: Option<Toggle>,
    pub run_allowed: Option<Toggle>,
    pub expiry: ExpiryArgs,
    pub about: DescriptionArgs,
    pub tags: Vec<String>,
    pub untag: Vec<String>,
    pub plain: Vec<String>,
    pub secret: Vec<String>,
    pub status: Option<StatusArg>,
}

fn tags(values: &[String]) -> Result<Vec<Tag>> {
    values
        .iter()
        .map(|t| Tag::parse(t).map_err(|e| anyhow!("{e}")))
        .collect()
}

pub fn add(app: &App, args: &AddArgs) -> Result<()> {
    let (kind, name) = (args.kind, item_name(&args.name)?);
    let (description, field_descriptions) = (args.about.item()?, args.about.new_fields(kind)?);
    let fields = collect_fields(kind, &args.input, true)?;
    let vault = app.vault()?;
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
            plain_fields: args
                .plain
                .iter()
                .map(|f| field_name(kind, f))
                .collect::<Result<_>>()?,
        },
    };
    items::add(&vault, item, &args.about.texts())?;
    println!("added\t{}", name.as_str());
    Ok(())
}

pub fn edit(app: &App, args: &EditArgs) -> Result<()> {
    let name = item_name(&args.name)?;
    let vault = app.vault()?;
    let kind = vault
        .list()?
        .into_iter()
        .find(|i| i.name == name.as_str())
        .context("item not found")?
        .kind;
    let new_origins = if args.clear_origins {
        Some(Vec::new())
    } else if args.origins.is_empty() {
        None
    } else {
        Some(origins(&args.origins)?)
    };
    let change = ItemChange {
        set_fields: collect_fields(kind, &args.input, false)?,
        remove_fields: args
            .unset
            .iter()
            .map(|f| field_name(kind, f))
            .collect::<Result<_>>()?,
        origins: new_origins,
        origin_binding: args.origin_binding.map(Toggle::on),
        run_allowed: args.run_allowed.map(Toggle::on),
        expiry: args.expiry.for_change(app.clock())?,
        description: args.about.item()?,
        field_descriptions: args.about.fields(kind)?,
        status: args.status.map(ItemStatus::from),
        add_tags: tags(&args.tags)?,
        remove_tags: tags(&args.untag)?,
        make_plain: args
            .plain
            .iter()
            .map(|f| field_name(kind, f))
            .collect::<Result<_>>()?,
        make_secret: args
            .secret
            .iter()
            .map(|f| field_name(kind, f))
            .collect::<Result<_>>()?,
    };
    items::edit(&vault, &TtyConfirm, &name, change, &args.about.texts())?;
    println!("updated\t{}", name.as_str());
    Ok(())
}
