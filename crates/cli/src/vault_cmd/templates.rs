//! `vault template add|ls|rm`: credential templates (R-48), the fields a new `credential`
//! item starts with. A template holds no value, so none of these asks a person (J-281). The
//! rules are core's (`keyharbor_core::template`).

use super::input::field_name;
use crate::compose::App;
use anyhow::{Result, anyhow, bail};
use clap::Subcommand;
use keyharbor_core::template;
use keyharbor_domain::vault::{CredentialTemplate, ItemKind, TemplateField, TemplateName};

#[derive(Subcommand)]
pub enum TemplateCmd {
    /// Add a template: each --field is secret unless also named by --plain
    Add {
        name: String,
        /// A field the template gives, in order (repeat for more)
        #[arg(long = "field", value_name = "FIELD", required = true)]
        fields: Vec<String>,
        /// One of the fields that is not secret: its value shows in listings, unmasked
        #[arg(long = "plain", value_name = "FIELD")]
        plain: Vec<String>,
    },
    /// List templates and their fields
    #[command(visible_alias = "list")]
    Ls {
        #[arg(long)]
        json: bool,
    },
    /// Remove a template; items made from it keep their fields
    Rm { name: String },
}

pub fn template_name(value: &str) -> Result<TemplateName> {
    TemplateName::parse(value).map_err(|e| anyhow!("{e}"))
}

/// The template `name` asks for, for a new item of `kind`.
pub fn for_new_item(app: &App, kind: ItemKind, name: &str) -> Result<CredentialTemplate> {
    if kind != ItemKind::Credential {
        bail!("a template is for credential items (--kind credential)");
    }
    template::find(&*app.store()?, &template_name(name)?)
}

fn build(name: &str, fields: &[String], plain: &[String]) -> Result<CredentialTemplate> {
    let kind = ItemKind::Credential;
    if let Some(stray) = plain.iter().find(|p| !fields.contains(p)) {
        bail!("--plain {stray} is not one of the template's --field names");
    }
    let fields = fields
        .iter()
        .map(|f| {
            Ok(TemplateField {
                name: field_name(kind, f)?,
                secret: !plain.contains(f),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    CredentialTemplate::new(template_name(name)?, fields).map_err(|e| anyhow!("{e}"))
}

/// `name  field,field  plain field` (the last column only when one is plain).
fn line(t: &CredentialTemplate) -> String {
    let names: Vec<&str> = t.fields().iter().map(|f| f.name.as_str()).collect();
    let plain: Vec<String> = t.plain_fields().iter().map(|f| f.as_str().into()).collect();
    let mut out = format!("{}\t{}", t.name().as_str(), names.join(","));
    if !plain.is_empty() {
        out += &format!("\tplain {}", plain.join(","));
    }
    out
}

fn json(t: &CredentialTemplate) -> serde_json::Value {
    let fields: Vec<_> = t
        .fields()
        .iter()
        .map(|f| serde_json::json!({"name": f.name.as_str(), "secret": f.secret}))
        .collect();
    serde_json::json!({"name": t.name().as_str(), "fields": fields})
}

pub fn run(app: &App, command: TemplateCmd) -> Result<()> {
    let store = app.store()?;
    match command {
        TemplateCmd::Add {
            name,
            fields,
            plain,
        } => {
            let built = build(&name, &fields, &plain)?;
            template::add(&*store, &built)?;
            println!("added\t{}", built.name().as_str());
        }
        TemplateCmd::Ls { json: as_json } => {
            let all = template::list(&*store)?;
            if as_json {
                println!(
                    "{}",
                    serde_json::Value::Array(all.iter().map(json).collect())
                );
            } else {
                for t in &all {
                    println!("{}", line(t));
                }
            }
        }
        TemplateCmd::Rm { name } => {
            let name = template_name(&name)?;
            template::remove(&*store, &name)?;
            println!("removed\t{}", name.as_str());
        }
    }
    Ok(())
}
