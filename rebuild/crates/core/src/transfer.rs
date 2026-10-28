//! Encrypted export/import in the age format (docs/design/07-cryptography-design.md §7.7).

use crate::{ItemSource, ItemStatus, NewItem, PlaintextAccess, Vault};
use age::secrecy::SecretString;
use anyhow::{Context, Result, anyhow, bail};
use keyharbor_domain::vault::{FieldName, ItemKind, ItemName, Origin, PlaceholderRef};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Write, path::Path};
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
    status: String,
    fields: BTreeMap<String, String>,
}

impl Vault {
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

    /// Writes every item, values included, to a new age file (mode 0600). Never overwrites.
    pub fn export_age(&self, passphrase: SecretString, out: &Path) -> Result<()> {
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
            exported_at: crate::now_rfc3339(),
            items: summaries
                .iter()
                .zip(&values)
                .map(|(s, fields)| ItemOut {
                    name: &s.name,
                    kind: s.kind.as_str(),
                    origins: &s.origins,
                    origin_binding: s.origin_binding,
                    run_allowed: s.run_allowed,
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
        let encrypted = age::encrypt(&age::scrypt::Recipient::new(passphrase), &plaintext)
            .map_err(|e| anyhow!("age encryption failed: {e}"))?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options
            .open(out)
            .with_context(|| format!("cannot create {} (it must not exist yet)", out.display()))?;
        file.write_all(&encrypted)?;
        file.sync_all()?;
        Ok(())
    }

    /// Adds every item whose name is free; existing names are left untouched and reported.
    pub fn import_age(&self, passphrase: SecretString, input: &Path) -> Result<ImportReport> {
        let encrypted =
            std::fs::read(input).with_context(|| format!("cannot read {}", input.display()))?;
        let plaintext = Zeroizing::new(
            age::decrypt(&age::scrypt::Identity::new(passphrase), &encrypted).map_err(|_| {
                anyhow!("could not decrypt export (wrong passphrase or not a KeyHarbor export)")
            })?,
        );
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
    use age::secrecy::SecretString;
    use keyharbor_domain::vault::{ItemKind, ItemName};

    fn pass(p: &str) -> SecretString {
        SecretString::from(p.to_owned())
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
        let out = a.data.parent().unwrap().join("export.age");
        va.export_age(pass("correct horse"), &out).unwrap();
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
        let out = e.data.parent().unwrap().join("export.age");
        v.export_age(pass("pw"), &out).unwrap();
        let bytes = std::fs::read(&out).unwrap();
        assert!(bytes.starts_with(b"age-encryption.org/v1\n"));
        assert!(!contains(&bytes, secret.as_bytes()));
        let plain = age::decrypt(&age::scrypt::Identity::new(pass("pw")), &bytes).unwrap();
        assert!(contains(&plain, b"\"format\":\"keyharbor-export/v1\""));
        assert!(contains(&plain, secret.as_bytes()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&out).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn wrong_passphrase_and_existing_target_fail() {
        let e = env();
        let v = init(&e);
        v.add(login("github", "pw")).unwrap();
        let out = e.data.parent().unwrap().join("export.age");
        v.export_age(pass("right"), &out).unwrap();
        let before = std::fs::read(&out).unwrap();
        assert!(v.export_age(pass("right"), &out).is_err());
        assert_eq!(std::fs::read(&out).unwrap(), before);
        let other = env();
        let w = init(&other);
        assert!(w.import_age(pass("wrong"), &out).is_err());
        assert!(w.list().unwrap().is_empty());
    }
}
