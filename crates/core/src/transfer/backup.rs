//! Encrypted export/import in the age format (docs/design/07-cryptography-design.md §7.7).
//! These work on bytes; the CLI reads and writes the file (0600, never overwriting).

use crate::{
    ExpiryChoice, ItemDetails, ItemSource, ItemStatus, ItemSummary, NewItem, PlaintextAccess, Vault,
};
use anyhow::{Context, Result, anyhow, bail};
use keyharbor_crypto::export::{Passphrase, decrypt_age, encrypt_age};
use keyharbor_domain::{
    date::Expiry,
    tag::Tag,
    vault::{Description, FieldName, ItemKind, ItemName, Origin, PlaceholderRef},
};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use std::io;
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
    otp_allowed: bool,
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
    /// Absent in exports from before #206: on, as it was then.
    #[serde(default)]
    otp_allowed: Option<bool>,
    status: String,
    /// Wiped on drop from the moment it is parsed, so no early return frees a value unwiped
    /// (#154, J-190).
    #[serde(deserialize_with = "wiped_values")]
    fields: BTreeMap<String, Zeroizing<String>>,
}

/// One field value, moved into `Zeroizing` as soon as serde hands it over: a map that fails
/// halfway drops the values it already holds wiped.
struct WipedValue(Zeroizing<String>);

impl<'de> Deserialize<'de> for WipedValue {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d).map(|v| Self(Zeroizing::new(v)))
    }
}

fn wiped_values<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<BTreeMap<String, Zeroizing<String>>, D::Error> {
    // Moving a `String` moves its heap buffer; nothing is copied.
    BTreeMap::<String, WipedValue>::deserialize(d)
        .map(|m| m.into_iter().map(|(k, WipedValue(v))| (k, v)).collect())
}

/// The serialized export while it is built. A growing `Vec` frees its old buffer without
/// wiping it, so this moves to a bigger `Zeroizing` buffer itself and the old one is wiped
/// as it drops (J-190).
struct WipingBuffer(Zeroizing<Vec<u8>>);

/// First allocation; most exports are a few KiB.
const WIPING_BUFFER_START: usize = 4096;

impl WipingBuffer {
    fn new() -> Self {
        Self(Zeroizing::new(Vec::with_capacity(WIPING_BUFFER_START)))
    }
}

impl io::Write for WipingBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let needed = self.0.len() + buf.len();
        if needed > self.0.capacity() {
            let mut bigger = Zeroizing::new(Vec::with_capacity(needed.max(self.0.capacity() * 2)));
            bigger.extend_from_slice(&self.0);
            self.0 = bigger;
        }
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> ItemOut<'a> {
    /// The export record of `s` with its `fields` values. Field descriptions equal to the
    /// kind's default are left out, so an import keeps following the default.
    fn from_summary(s: &'a ItemSummary, fields: &'a [(String, Zeroizing<String>)]) -> Self {
        ItemOut {
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
            otp_allowed: s.otp_allowed,
            status: s.status.as_str(),
            source: s.source.as_str(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect(),
        }
    }
}

impl ItemIn {
    /// The item this export record describes, checked as any new item is; it is marked
    /// imported whatever its source was.
    fn into_new_item(self) -> Result<NewItem> {
        let kind = ItemKind::parse(&self.kind).context("unknown item kind in export")?;
        let fields = self
            .fields
            .into_iter()
            .map(|(k, v)| Ok((FieldName::parse(&k)?, v)))
            .collect::<Result<Vec<_>>>()?;
        Ok(NewItem {
            name: ItemName::parse(&self.name)?,
            kind,
            fields,
            origins: self
                .origins
                .iter()
                .map(|o| Origin::parse(o))
                .collect::<Result<_, _>>()?,
            origin_binding: Some(self.origin_binding),
            status: ItemStatus::parse(&self.status).context("unknown status in export")?,
            source: ItemSource::Import,
            details: ItemDetails {
                run_allowed: self.run_allowed,
                expiry: match self.expires_at.as_deref() {
                    Some(date) => ExpiryChoice::On(Expiry::parse(date)?),
                    None => ExpiryChoice::Never,
                },
                description: self
                    .description
                    .as_deref()
                    .map(Description::parse)
                    .transpose()?,
                field_descriptions: self
                    .field_descriptions
                    .iter()
                    .map(|(f, d)| Ok((FieldName::parse(f)?, Description::parse(d)?)))
                    .collect::<Result<_>>()?,
                tags: self
                    .tags
                    .iter()
                    .map(|t| Tag::parse(t))
                    .collect::<Result<_, _>>()?,
                plain_fields: self
                    .plain_fields
                    .iter()
                    .map(|f| FieldName::parse(f))
                    .collect::<Result<_, _>>()?,
                otp_allowed: self.otp_allowed,
            },
        })
    }
}

impl Vault {
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
            let item = ItemName::parse(&summary.name)?;
            let mut fields = Vec::new();
            for field in &summary.fields {
                let reference = PlaceholderRef {
                    item: item.clone(),
                    field: FieldName::parse(field)?,
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
                .map(|(s, fields)| ItemOut::from_summary(s, fields))
                .collect(),
        };
        let mut plaintext = WipingBuffer::new();
        serde_json::to_writer(&mut plaintext, &doc)?;
        encrypt_age(passphrase, &plaintext.0)
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
            self.add(item.into_new_item()?)?;
            report.added += 1;
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::{ItemIn, WipingBuffer};
    use crate::tests::{contains, env, field, init, login, pref, reopen, uuid_like};
    use crate::{ItemSource, ItemStatus, NewItem};
    use keyharbor_crypto::export::{Passphrase, decrypt_age, encrypt_age};
    use keyharbor_domain::vault::{ItemKind, ItemName};
    use std::collections::BTreeMap;
    use std::io::Write;
    use zeroize::Zeroizing;

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

    /// An export holding one item whose `fields` is `fields_json`.
    fn export_with_fields(fields_json: &str) -> Vec<u8> {
        let doc = format!(
            r#"{{"format":"keyharbor-export/v1","exported_at":"2026-01-01T00:00:00Z","items":[{{"name":"github","kind":"login","origins":[],"origin_binding":false,"status":"active","source":"user","fields":{fields_json}}}]}}"#
        );
        encrypt_age(pass("pw"), doc.as_bytes()).unwrap()
    }

    #[test]
    fn imported_field_values_are_wiped_types_from_parsing_on() {
        let item: ItemIn = serde_json::from_str(
            r#"{"name":"a","kind":"login","origins":[],"origin_binding":false,"status":"active","fields":{"password":"pw!"}}"#,
        )
        .unwrap();
        // Compile-time check (#154): changing the field type back to `String` breaks this.
        let fields: &BTreeMap<String, Zeroizing<String>> = &item.fields;
        assert_eq!(fields["password"].as_str(), "pw!");
    }

    #[test]
    fn malformed_field_name_returns_early_without_adding() {
        let e = env();
        let v = init(&e);
        let out = export_with_fields(r#"{"password":"pw","bad name":"KH-SYNTH-x"}"#);
        let err = v.import_age(pass("pw"), &out).unwrap_err();
        assert!(!format!("{err:#}").contains("KH-SYNTH"));
        assert!(v.list().unwrap().is_empty());
    }

    #[test]
    fn non_string_field_value_fails_parsing_without_echoing_values() {
        let e = env();
        let v = init(&e);
        let out = export_with_fields(r#"{"password":"KH-SYNTH-y","otp":7}"#);
        let err = v.import_age(pass("pw"), &out).unwrap_err();
        assert_eq!(format!("{err:#}"), "export content is not valid");
        assert!(v.list().unwrap().is_empty());
    }

    #[test]
    fn wiping_buffer_keeps_every_byte_across_growth() {
        let mut buf = WipingBuffer::new();
        let chunk = [7u8; 1000];
        for _ in 0..50 {
            buf.write_all(&chunk).unwrap();
        }
        assert_eq!(buf.0.len(), 50_000);
        assert!(buf.0.iter().all(|&b| b == 7));
    }
}
