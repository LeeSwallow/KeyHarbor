//! The plaintext paths. Both demand a [`PlaintextAccess`], which only the guard (after its
//! checks) and core's export obtain; values only ever land in `Zeroizing` buffers.

use super::{MaskValues, Vault, vault_err};
use crate::PlaintextAccess;
use anyhow::{Context, Result, anyhow};
use keyharbor_crypto::otp::OtpSeed;
use keyharbor_domain::vault::{
    Domain, FieldName, ItemKind, ItemName, Placeholder, PlaceholderRef, parse_secret_placeholders,
    placeholder_spans,
};
use keyharbor_vault::record::{FieldAd, open_field};
use zeroize::Zeroizing;

impl Vault {
    /// Decrypts one field after verifying the item's metadata. Only the guard (and export)
    /// holds the capability this needs.
    pub fn resolve(
        &self,
        _access: &PlaintextAccess,
        reference: &PlaceholderRef,
    ) -> Result<Zeroizing<String>> {
        let (row, fields) = self.load_item_row(&reference.item)?;
        let (kind, _, _) = self.verified(&row)?;
        let field = fields
            .iter()
            .find(|f| f.field == reference.field.as_str())
            .context("field not found")?;
        let domain = kind.domain();
        let ad = FieldAd {
            vault: &self.vault_id,
            domain,
            item: &row.id,
            field: &field.field,
        };
        let bytes =
            open_field(&self.domain_key(domain)?.1, &ad, &field.sealed).map_err(vault_err)?;
        let text = std::str::from_utf8(&bytes).context("stored field is not UTF-8")?;
        Ok(Zeroizing::new(text.to_owned()))
    }

    /// The item's one-time code at `unix_seconds` (#206), with the codes of the steps just
    /// before and after it for the guard's masker. Like [`Self::resolve`], only the guard holds
    /// the capability; the seed never leaves this function.
    pub fn one_time_code(
        &self,
        access: &PlaintextAccess,
        item: &ItemName,
        unix_seconds: u64,
    ) -> Result<OneTimeCode> {
        let (row, _) = self.load_item_row(item)?;
        let (kind, _, _) = self.verified(&row)?;
        let field = kind
            .otp_field()
            .context("the item's kind has no one-time code seed")?;
        let reference = PlaceholderRef {
            item: item.clone(),
            field: FieldName::parse(field)?,
        };
        let seed = OtpSeed::parse(&self.resolve(access, &reference)?)
            .map_err(|e| anyhow!("{field}: {e}"))?;
        let period = seed.period();
        Ok(OneTimeCode {
            code: seed.code_at(unix_seconds),
            neighbours: [
                seed.code_at(unix_seconds.saturating_sub(period)),
                seed.code_at(unix_seconds.saturating_add(period)),
            ],
            period,
        })
    }

    /// Every field value in the vault, whatever its status or policy (an item an agent may not
    /// use must not leak either), with the version they were read at — for building the
    /// guard's masker. Fails if any value cannot be decrypted, so callers can withhold
    /// responses rather than send them unmasked.
    pub fn mask_values(&self, _access: &PlaintextAccess) -> Result<MaskValues> {
        let version = self.version()?;
        let mut values = Vec::new();
        for row in self.store.item_list()? {
            let item = ItemName::parse(&row.name)?;
            let domain = Domain::from_byte(row.domain).context("unknown domain in database")?;
            let (_, fields) = self
                .store
                .item_get(&row.name)?
                .context("item changed while masking")?;
            // An unknown kind is masked in full: masking errs on the side of hiding.
            let kind = ItemKind::parse(&row.kind);
            for field in fields {
                // A field marked not secret is shown on purpose; masking it would blank out
                // common short values. The mark is signed (MAC v4); a tampered row keeps it
                // masked.
                if row.plain_fields.contains(&field.field) && self.verified(&row).is_ok() {
                    continue;
                }
                if kind
                    .and_then(|k| k.field(&field.field))
                    .is_some_and(|spec| !spec.maskable)
                {
                    continue;
                }
                let ad = FieldAd {
                    vault: &self.vault_id,
                    domain,
                    item: &row.id,
                    field: &field.field,
                };
                let reference = PlaceholderRef {
                    item: item.clone(),
                    field: FieldName::parse(&field.field)?,
                };
                let dk = &self.domain_key(domain)?.1;
                // The value before the last change may still be valid somewhere: mask it too.
                for record in std::iter::once(&field.sealed).chain(field.previous.as_ref()) {
                    let bytes = open_field(dk, &ad, record)
                        .map_err(vault_err)
                        .context("could not decrypt a value for masking")?;
                    let text = Zeroizing::new(
                        std::str::from_utf8(&bytes)
                            .context("stored field is not UTF-8")?
                            .to_owned(),
                    );
                    values.push((reference.clone(), text));
                }
            }
        }
        Ok(MaskValues { version, values })
    }
}

/// An item's one-time code at one moment. Every code is wiped when dropped.
pub struct OneTimeCode {
    pub code: Zeroizing<String>,
    /// The codes of the step before and the step after: a page that shows or echoes a code a
    /// little late or early must not show it unmasked either (J-292).
    pub neighbours: [Zeroizing<String>; 2],
    /// Seconds one code lasts.
    pub period: u64,
}

/// Replaces every `{{secret:item.field}}` in `template` with `resolve(reference)`. A
/// `{{otp:item}}` fails the whole template: this is the path of `keyharbor run`, which never
/// takes a one-time code.
pub fn fill_template(
    template: &str,
    mut resolve: impl FnMut(&PlaceholderRef) -> Result<Zeroizing<String>>,
) -> Result<Zeroizing<String>> {
    parse_secret_placeholders(template)?;
    fill_placeholders(template, |found| match found {
        Placeholder::Secret(reference) => resolve(reference),
        Placeholder::Otp(_) => unreachable!("refused above"),
    })
}

/// Replaces every placeholder of either namespace in `template` with `resolve(placeholder)`.
/// The result is allocated once at its final size, so no partial copy is left behind.
pub fn fill_placeholders(
    template: &str,
    mut resolve: impl FnMut(&Placeholder) -> Result<Zeroizing<String>>,
) -> Result<Zeroizing<String>> {
    let spans = placeholder_spans(template)?;
    let values = spans
        .iter()
        .map(|(_, found)| resolve(found))
        .collect::<Result<Vec<_>>>()?;
    let covered: usize = spans.iter().map(|(range, _)| range.len()).sum();
    let size = template.len() - covered + values.iter().map(|v| v.len()).sum::<usize>();
    let mut out = Zeroizing::new(String::with_capacity(size));
    let mut at = 0;
    for ((range, _), value) in spans.iter().zip(&values) {
        out.push_str(&template[at..range.start]);
        out.push_str(value);
        at = range.end;
    }
    out.push_str(&template[at..]);
    debug_assert_eq!(out.len(), size);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{env, init, login, pref};

    #[test]
    fn templates_substitute_in_order() {
        let e = env();
        let v = init(&e);
        v.add(login("github", "KH-SYNTH-pw")).unwrap();
        let out = fill_template(
            "u={{secret:github.username}}&p={{secret:github.password}}!",
            |r| v.resolve(r),
        )
        .unwrap();
        assert_eq!(&*out, "u=me@x.com&p=KH-SYNTH-pw!");
        assert_eq!(&*fill_template("plain", |r| v.resolve(r)).unwrap(), "plain");
        let mut seen = Vec::new();
        fill_template("{{secret:a.b}}{{secret:c.d}}", |r| {
            seen.push(r.clone());
            Ok(Zeroizing::new(String::new()))
        })
        .unwrap();
        assert_eq!(seen, vec![pref("a", "b"), pref("c", "d")]);
        assert!(fill_template("{{secret:github.note}}", |r| v.resolve(r)).is_err());
        assert!(fill_template("{{secret:github}}", |r| v.resolve(r)).is_err());
        // `keyharbor run`'s path never takes a one-time code.
        assert!(fill_template("{{otp:github}}", |r| v.resolve(r)).is_err());
    }

    /// RFC 6238's SHA-1 seed, in base32.
    const RFC_SEED: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    fn with_seed(seed: &str) -> crate::NewItem {
        let mut item = login("bank", "KH-SYNTH-pw");
        item.fields.push(crate::tests::field("totp_seed", seed));
        item
    }

    #[test]
    fn a_login_seed_gives_the_code_of_the_moment_and_its_neighbours() {
        let e = env();
        let v = init(&e);
        v.add(with_seed(RFC_SEED)).unwrap();
        let bank = ItemName::parse("bank").unwrap();
        // RFC 6238 Appendix B, SHA-1 at 59 s: 94287082 (six digits: the last six).
        let code = v.one_time_code(&bank, 59).unwrap();
        assert_eq!(*code.code, "287082");
        assert_eq!(code.period, 30);
        assert_eq!(
            *code.neighbours[0],
            *v.one_time_code(&bank, 29).unwrap().code
        );
        assert_eq!(
            *code.neighbours[1],
            *v.one_time_code(&bank, 89).unwrap().code
        );
        // Placeholders of both namespaces fill in order.
        let out = fill_placeholders(
            "{{secret:bank.username}} {{otp:bank}}.",
            |found| match found {
                Placeholder::Secret(r) => v.resolve(r),
                Placeholder::Otp(item) => Ok(v.one_time_code(item, 59)?.code),
            },
        )
        .unwrap();
        assert_eq!(&*out, "me@x.com 287082.");
        // An item without a seed has no code.
        v.add(login("github", "KH-SYNTH-pw")).unwrap();
        assert!(
            v.one_time_code(&ItemName::parse("github").unwrap(), 59)
                .is_err()
        );
    }

    #[test]
    fn a_seed_that_is_not_one_is_refused_without_its_value() {
        let e = env();
        let v = init(&e);
        let error = v
            .add(with_seed("KH-SYNTH-not a seed!"))
            .unwrap_err()
            .to_string();
        assert!(
            error.starts_with("field totp_seed: not a one-time code seed"),
            "{error}"
        );
        assert!(!error.contains("KH-SYNTH"), "{error}");
        let uri = format!("otpauth://totp/Bank:me?secret={RFC_SEED}&issuer=Bank");
        v.add(with_seed(&uri)).unwrap();
        let change = crate::ItemChange {
            set_fields: vec![crate::tests::field("totp_seed", "KH-SYNTH-1!")],
            ..Default::default()
        };
        let bank = ItemName::parse("bank").unwrap();
        let error = v.edit(&bank, change).unwrap_err().to_string();
        assert!(!error.contains("KH-SYNTH"), "{error}");
        // An import keeps what an earlier release stored (it could not check seeds).
        let mut imported = with_seed("see my phone");
        imported.name = ItemName::parse("old").unwrap();
        imported.source = crate::ItemSource::Import;
        v.add(imported).unwrap();
    }
}
