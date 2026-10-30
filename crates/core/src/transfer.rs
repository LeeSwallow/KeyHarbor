//! Encrypted export/import in the age format (docs/design/07-cryptography-design.md §7.7).
//! These work on bytes; the CLI reads and writes the file (0600, never overwriting).

use crate::{ItemSource, ItemStatus, NewItem, PlaintextAccess, Vault};
use anyhow::{Context, Result, anyhow, bail};
use keyharbor_crypto::export::{Passphrase, decrypt_age, encrypt_age};
use keyharbor_domain::vault::{Description, FieldName, ItemKind, ItemName, Origin, PlaceholderRef};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

pub const EXPORT_FORMAT: &str = "keyharbor-export/v1";

#[derive(Debug, PartialEq, Eq)]
pub struct ImportReport {
    pub added: usize,
    pub skipped_existing: Vec<String>,
}

#[derive(Serialize)]
struct ExportOut<'a> {
    format: &'static str,
    exported_at: String,
    items: Vec<ItemOut<'a>>,
}
#[derive(Serialize)]
struct ItemOut<'a> {
    name: &'a str,
    kind: &'static str,
    origins: &'a [String],
    origin_binding: bool,
    run_allowed: bool,
    expires_at: Option<String>,
    description: Option<&'a str>,
    field_descriptions: BTreeMap<&'a str, &'a str>,
    tags: &'a [String],
    /// Fields that are not secret.
    plain_fields: Vec<&'a str>,
    status: &'static str,
    source: &'static str,
    fields: BTreeMap<&'a str, &'a str>,
}
#[derive(Deserialize)]
struct ExportIn {
    format: String,
    items: Vec<ItemIn>,
}
#[derive(Deserialize)]
struct ItemIn {
    name: String,
    kind: String,
    origins: Vec<String>,
    origin_binding: bool,
    /// Absent in exports from before phase 3: the kind's default.
    #[serde(default)]
    run_allowed: Option<bool>,
    #[serde(default)]
    expires_at: Option<String>,
    #[serde(default)]
    description: Option<String>,
    /// Only the item's own descriptions (not the kind's defaults).
    #[serde(default)]
    field_descriptions: BTreeMap<String, String>,
    /// Absent in exports from before tags.
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    plain_fields: Vec<String>,
    status: String,
    fields: BTreeMap<String, String>,
}

/// What `import_chrome_csv` did. Counts only: names and values stay out of reports.
#[derive(Debug, PartialEq, Eq)]
pub struct ChromeImportReport {
    pub added: usize,
    /// Rows whose name an item already had; that item is left as it was.
    pub skipped_existing: usize,
    /// Rows for no web address (`android://…`, plain HTTP): an origin cannot hold them.
    pub skipped_not_web: usize,
}

/// Items from a Chrome export are named `chrome/<host>`, dots as `-`: `chrome/github-com`.
const CHROME_PREFIX: &str = "chrome/";
/// Room left for a `-<n>` suffix within the 64-character name limit.
const CHROME_HOST_CHARS: usize = 52;

/// `chrome/<host>`, or `chrome/<host>-<n>` for the n-th account on that site in this file.
fn chrome_item_name(origin: &Origin, seen: &mut BTreeMap<String, usize>) -> Result<ItemName> {
    let host = origin.as_str().split_once("://").map_or("", |(_, h)| h);
    let host: String = host
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .take(CHROME_HOST_CHARS)
        .collect();
    let count = seen.entry(host.clone()).or_default();
    *count += 1;
    let name = match *count {
        1 => format!("{CHROME_PREFIX}{host}"),
        n => format!("{CHROME_PREFIX}{host}-{n}"),
    };
    ItemName::parse(&name).map_err(|e| anyhow!("{e}"))
}

impl Vault {
    /// Adds a login for each row of a Chrome password export, bound to the row's site, with
    /// the note in the login's secret `note` field. Existing names are not overwritten.
    pub fn import_chrome_csv(&self, text: &str) -> Result<ChromeImportReport> {
        let rows = crate::chrome_csv::parse_chrome_csv(text)?;
        let existing: Vec<String> = self.list()?.into_iter().map(|i| i.name).collect();
        let mut report = ChromeImportReport {
            added: 0,
            skipped_existing: 0,
            skipped_not_web: 0,
        };
        let mut seen = BTreeMap::new();
        for row in rows {
            let Ok(origin) = Origin::of_page(&row.url) else {
                report.skipped_not_web += 1;
                continue;
            };
            let name = chrome_item_name(&origin, &mut seen)?;
            if existing.iter().any(|e| e == name.as_str()) {
                report.skipped_existing += 1;
                continue;
            }
            let mut fields = Vec::new();
            for (field, value) in [
                ("username", row.username),
                ("password", row.password),
                ("note", row.note),
            ] {
                if !value.is_empty() {
                    fields.push((FieldName::parse(field).map_err(|e| anyhow!("{e}"))?, value));
                }
            }
            self.add(NewItem {
                name,
                kind: ItemKind::Login,
                fields,
                origins: vec![origin],
                origin_binding: None,
                status: ItemStatus::Active,
                source: ItemSource::Import,
                details: Default::default(),
            })
            .with_context(|| format!("line {}", row.line))?;
            report.added += 1;
        }
        Ok(report)
    }

    pub fn import_env(&self, name: ItemName, text: &str) -> Result<usize> {
        let fields = crate::dotenv::parse_dotenv(text)?;
        let count = fields.len();
        self.add(NewItem {
            name,
            kind: ItemKind::Env,
            fields,
            origins: Vec::new(),
            origin_binding: Some(false),
            status: ItemStatus::Active,
            source: ItemSource::Import,
            details: Default::default(),
        })?;
        Ok(count)
    }

    /// Every item, values included, as an age file's bytes (encrypted to `passphrase`).
    pub fn export_age(&self, passphrase: Passphrase) -> Result<Vec<u8>> {
        let summaries = self.list()?;
        if let Some(bad) = summaries.iter().find(|s| s.tampered) {
            bail!("item {} failed verification; refusing to export", bad.name);
        }
        // The user asked for a backup of every value: export is the one plaintext path that
        // does not go through the guard (it never reaches an agent, only the age file).
        let access = PlaintextAccess::issue();
        let mut values: Vec<Vec<(String, Zeroizing<String>)>> = Vec::new();
        for summary in &summaries {
            let item = ItemName::parse(&summary.name).map_err(|e| anyhow!(e.to_string()))?;
            let mut fields = Vec::new();
            for field in &summary.fields {
                let reference = PlaceholderRef {
                    item: item.clone(),
                    field: FieldName::parse(field).map_err(|e| anyhow!(e.to_string()))?,
                };
                fields.push((field.clone(), self.resolve(&access, &reference)?));
            }
            values.push(fields);
        }
        let doc = ExportOut {
            format: EXPORT_FORMAT,
            exported_at: self.clock.now_rfc3339(),
            items: summaries
                .iter()
                .zip(&values)
                .map(|(s, fields)| ItemOut {
                    name: &s.name,
                    kind: s.kind.as_str(),
                    origins: &s.origins,
                    origin_binding: s.origin_binding,
                    run_allowed: s.run_allowed,
                    expires_at: s.expires_at.map(|e| e.to_string()),
                    description: s.description.as_deref(),
                    field_descriptions: s
                        .field_descriptions
                        .iter()
                        .filter(|(f, d)| {
                            s.kind
                                .field(f)
                                .is_none_or(|spec| spec.description != d.as_str())
                        })
                        .map(|(f, d)| (f.as_str(), d.as_str()))
                        .collect(),
                    tags: &s.tags,
                    plain_fields: s.plain_values.keys().map(String::as_str).collect(),
                    status: s.status.as_str(),
                    source: s.source.as_str(),
                    fields: fields
                        .iter()
                        .map(|(k, v)| (k.as_str(), v.as_str()))
                        .collect(),
                })
                .collect(),
        };
        let plaintext = Zeroizing::new(serde_json::to_vec(&doc)?);
        encrypt_age(passphrase, &plaintext)
    }

    /// Adds every item of the age file `encrypted` whose name is free; existing names are left
    /// untouched and reported.
    pub fn import_age(&self, passphrase: Passphrase, encrypted: &[u8]) -> Result<ImportReport> {
        let plaintext = decrypt_age(passphrase, encrypted).map_err(|_| {
            anyhow!("could not decrypt export (wrong passphrase or not a KeyHarbor export)")
        })?;
        let doc: ExportIn = serde_json::from_slice(&plaintext)
            .map_err(|_| anyhow!("export content is not valid"))?;
        if doc.format != EXPORT_FORMAT {
            bail!("unsupported export format");
        }
        let existing: Vec<String> = self.list()?.into_iter().map(|s| s.name).collect();
        let mut report = ImportReport {
            added: 0,
            skipped_existing: Vec::new(),
        };
        for item in doc.items {
            if existing.contains(&item.name) {
                report.skipped_existing.push(item.name);
                continue;
            }
            let kind = ItemKind::parse(&item.kind).context("unknown item kind in export")?;
            let fields = item
                .fields
                .into_iter()
                .map(|(k, v)| {
                    Ok((
                        FieldName::parse(&k).map_err(|e| anyhow!(e.to_string()))?,
                        Zeroizing::new(v),
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            self.add(NewItem {
                name: ItemName::parse(&item.name).map_err(|e| anyhow!(e.to_string()))?,
                kind,
                fields,
                origins: item
                    .origins
                    .iter()
                    .map(|o| Origin::parse(o).map_err(|e| anyhow!(e.to_string())))
                    .collect::<Result<_>>()?,
                origin_binding: Some(item.origin_binding),
                status: ItemStatus::parse(&item.status).context("unknown status in export")?,
                source: ItemSource::Import,
                details: crate::ItemDetails {
                    run_allowed: item.run_allowed,
                    expiry: match item.expires_at.as_deref() {
                        Some(date) => crate::ExpiryChoice::On(
                            keyharbor_domain::date::Expiry::parse(date)
                                .map_err(|e| anyhow!("{e}"))?,
                        ),
                        None => crate::ExpiryChoice::Never,
                    },
                    description: item
                        .description
                        .as_deref()
                        .map(Description::parse)
                        .transpose()
                        .map_err(|e| anyhow!("{e}"))?,
                    field_descriptions: item
                        .field_descriptions
                        .iter()
                        .map(|(f, d)| {
                            Ok((
                                FieldName::parse(f).map_err(|e| anyhow!("{e}"))?,
                                Description::parse(d).map_err(|e| anyhow!("{e}"))?,
                            ))
                        })
                        .collect::<Result<_>>()?,
                    tags: item
                        .tags
                        .iter()
                        .map(|t| keyharbor_domain::tag::Tag::parse(t).map_err(|e| anyhow!("{e}")))
                        .collect::<Result<_>>()?,
                    plain_fields: item
                        .plain_fields
                        .iter()
                        .map(|f| FieldName::parse(f).map_err(|e| anyhow!("{e}")))
                        .collect::<Result<_>>()?,
                },
            })?;
            report.added += 1;
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::{contains, env, field, init, login, pref, reopen, uuid_like};
    use crate::{ItemSource, ItemStatus, NewItem};
    use keyharbor_crypto::export::{Passphrase, decrypt_age};
    use keyharbor_domain::vault::{ItemKind, ItemName};

    fn pass(p: &str) -> Passphrase {
        Passphrase::new(p.to_owned())
    }
    fn item(name: &str, kind: ItemKind, fields: &[(&str, &str)]) -> NewItem {
        NewItem {
            name: ItemName::parse(name).unwrap(),
            kind,
            fields: fields.iter().map(|(k, v)| field(k, v)).collect(),
            origins: vec![],
            origin_binding: None,
            status: ItemStatus::Active,
            source: ItemSource::User,
            details: Default::default(),
        }
    }

    #[test]
    fn a_chrome_export_becomes_one_login_per_row_bound_to_its_site() {
        let e = env();
        let v = init(&e);
        v.add(login("chrome/bank-example", "kept")).unwrap();
        let csv = "name,url,username,password,note\n\
            github.com,https://github.com/login,amy,pw-1,\"recovery\ncodes\"\n\
            github.com,https://github.com/,bob,pw-2,\n\
            bank,https://bank.example/,c,pw-3,\n\
            app,android://x@com.example/,d,pw-4,\n";
        let report = v.import_chrome_csv(csv).unwrap();
        assert_eq!(
            report,
            super::ChromeImportReport {
                added: 2,
                skipped_existing: 1,
                skipped_not_web: 1
            }
        );
        assert_eq!(
            &*v.resolve(&pref("chrome/github-com", "password")).unwrap(),
            "pw-1"
        );
        assert_eq!(
            &*v.resolve(&pref("chrome/github-com", "note")).unwrap(),
            "recovery\ncodes"
        );
        assert_eq!(
            &*v.resolve(&pref("chrome/github-com-2", "username")).unwrap(),
            "bob"
        );
        assert_eq!(
            &*v.resolve(&pref("chrome/bank-example", "password")).unwrap(),
            "kept",
            "not overwritten"
        );
        let github = v
            .list()
            .unwrap()
            .into_iter()
            .find(|i| i.name == "chrome/github-com")
            .unwrap();
        assert_eq!(
            (github.kind, github.source),
            (ItemKind::Login, ItemSource::Import)
        );
        assert_eq!(github.origins, ["https://github.com"]);
        assert!(
            github.description.is_none(),
            "the note is a secret field, not a plaintext description"
        );
    }

    #[test]
    fn import_env_creates_env_item() {
        let e = env();
        let v = init(&e);
        let n = v
            .import_env(
                ItemName::parse("app").unwrap(),
                "API_KEY=abc\nexport DB_URL=\"postgres://x\"\n",
            )
            .unwrap();
        assert_eq!(n, 2);
        let list = v.list().unwrap();
        assert_eq!(list[0].kind, ItemKind::Env);
        assert_eq!(list[0].source, ItemSource::Import);
        assert_eq!(&*v.resolve(&pref("app", "DB_URL")).unwrap(), "postgres://x");
    }

    #[test]
    fn export_import_roundtrip_into_other_vault() {
        let (a, b) = (env(), env());
        let va = init(&a);
        va.add(login("github", "pw")).unwrap();
        va.add(item(
            "visa",
            ItemKind::Card,
            &[("number", "4242"), ("cvc", "123")],
        ))
        .unwrap();
        va.add(item("app", ItemKind::Env, &[("API_KEY", "k")]))
            .unwrap();
        let out = va.export_age(pass("correct horse")).unwrap();
        let vb = init(&b);
        vb.add(login("github", "keep-me")).unwrap();
        let report = vb.import_age(pass("correct horse"), &out).unwrap();
        assert_eq!(report.added, 2);
        assert_eq!(report.skipped_existing, vec!["github".to_string()]);
        assert_eq!(
            &*vb.resolve(&pref("github", "password")).unwrap(),
            "keep-me"
        );
        assert_eq!(&*vb.resolve(&pref("visa", "cvc")).unwrap(), "123");
        assert_eq!(
            &*reopen(&b)
                .unwrap()
                .resolve(&pref("app", "API_KEY"))
                .unwrap(),
            "k"
        );
        let visa = vb
            .list()
            .unwrap()
            .into_iter()
            .find(|i| i.name == "visa")
            .unwrap();
        assert!(visa.origin_binding);
    }

    #[test]
    fn export_is_age_and_contains_no_plaintext() {
        let e = env();
        let v = init(&e);
        let secret = format!("KH-SYNTH-{}", uuid_like());
        v.add(login("github", &secret)).unwrap();
        let bytes = v.export_age(pass("pw")).unwrap();
        assert!(bytes.starts_with(b"age-encryption.org/v1\n"));
        assert!(!contains(&bytes, secret.as_bytes()));
        let plain = decrypt_age(pass("pw"), &bytes).unwrap();
        assert!(contains(&plain, b"\"format\":\"keyharbor-export/v1\""));
        assert!(contains(&plain, secret.as_bytes()));
        // The file mode (0600) is the CLI's: vault_cmd::files::tests.
    }

    #[test]
    fn wrong_passphrase_fails() {
        let e = env();
        let v = init(&e);
        v.add(login("github", "pw")).unwrap();
        let out = v.export_age(pass("right")).unwrap();
        // Refusing an existing target is the CLI's: vault_cmd::files::tests.
        let other = env();
        let w = init(&other);
        assert!(w.import_age(pass("wrong"), &out).is_err());
        assert!(w.list().unwrap().is_empty());
    }
}
