//! The item kind registry (`KINDS`): each kind's domain, fields and defaults (02 §2.7.2).

use super::{Domain, FieldName, InvalidValue};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ItemKind {
    Login,
    Secret,
    Env,
    Card,
    Identity,
    Address,
    Credential,
}
impl ItemKind {
    pub const ALL: [ItemKind; 7] = [
        Self::Login,
        Self::Secret,
        Self::Env,
        Self::Card,
        Self::Identity,
        Self::Address,
        Self::Credential,
    ];
    /// This kind's entry in [`KINDS`].
    pub fn spec(self) -> &'static KindSpec {
        KINDS
            .iter()
            .find(|spec| spec.kind == self)
            .expect("every kind is registered")
    }
    pub fn domain(self) -> Domain {
        self.spec().domain
    }
    pub fn as_str(self) -> &'static str {
        self.spec().name
    }
    pub fn parse(value: &str) -> Option<Self> {
        KINDS
            .iter()
            .find(|spec| spec.name == value)
            .map(|spec| spec.kind)
    }
    /// Fixed field names; `None` means free-form environment variable names.
    pub fn schema(self) -> Option<Vec<&'static str>> {
        let spec = self.spec();
        (!spec.free_fields).then(|| spec.fields.iter().map(|f| f.name).collect())
    }
    /// The field that holds the seed of the item's one-time codes (`{{otp:item}}`, #206), if
    /// the kind has one.
    pub fn otp_field(self) -> Option<&'static str> {
        self.spec()
            .fields
            .iter()
            .find(|f| f.otp_seed)
            .map(|f| f.name)
    }
    /// The field whose value can be generated (see [`KindSpec::generated`]).
    pub fn generated_field(self) -> Option<&'static str> {
        self.spec().generated
    }
    pub fn default_origin_binding(self) -> bool {
        self.spec().default_origin_binding
    }
    /// Whether `keyharbor run` may inject a new item of this kind (R-35, R-36).
    pub fn default_run_allowed(self) -> bool {
        self.spec().default_run_allowed
    }
    /// Whether a browser may receive this kind's values only in frames of its listed origins,
    /// and nowhere while it lists none (R-36), whatever its origin binding says.
    pub fn needs_origin_for_browser(self) -> bool {
        self.spec().browser_needs_origin
    }
    /// Whether `field` would be a field the person names on an item of this kind (R-48): the
    /// kind takes custom fields and does not declare one by that name.
    pub fn is_custom_field(self, field: &str) -> bool {
        let spec = self.spec();
        spec.custom_fields && !spec.fields.iter().any(|f| f.name == field)
    }
    /// Whether any kind declares `field` secret (`password`, `cvc`): such a name stays off the
    /// command line even as a custom field marked plain (R-48).
    pub fn is_declared_secret_anywhere(field: &str) -> bool {
        KINDS
            .iter()
            .flat_map(|k| k.fields)
            .any(|f| f.name == field && f.secret)
    }
    /// How `field` is treated in an item of this kind: its declared spec, the free-field spec
    /// for kinds with free-form names, the custom-field spec for kinds that take named fields, or `None` if the kind has no such field.
    pub fn field(self, field: &str) -> Option<&'static FieldSpec> {
        let spec = self.spec();
        spec.fields
            .iter()
            .find(|f| f.name == field)
            .or(spec.free_fields.then_some(&FREE_FIELD))
            .or(spec.custom_fields.then_some(&CUSTOM_FIELD))
    }
    /// A field name for this kind: free-form kinds take environment variable names.
    pub fn field_name(self, field: &str) -> Result<FieldName, InvalidValue> {
        if self.spec().free_fields {
            FieldName::parse_env(field)
        } else {
            FieldName::parse(field)
        }
    }
    /// Whether `field` may hold a secret, so it must never arrive on the command line. A name
    /// this kind does not declare counts as secret if any kind declares it secret.
    pub fn is_secret_field(self, field: &str) -> bool {
        match self.field(field) {
            Some(spec) => spec.secret,
            None => Self::is_declared_secret_anywhere(field),
        }
    }
}

/// Which elements a field's value may be typed into (03 §3.8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    /// Any element that takes text.
    Text,
    /// Only password inputs (`type=password`, or `autocomplete` current-/new-password).
    Password,
}

/// One field of an item kind.
#[derive(Debug, PartialEq, Eq)]
pub struct FieldSpec {
    pub name: &'static str,
    /// May hold a secret: never accepted as a command-line argument (`--set`), only from a
    /// hidden prompt or stdin.
    pub secret: bool,
    /// Replaced by its placeholder in responses (03 §3.7). Values under 4 characters never are.
    pub maskable: bool,
    /// Where a browser may receive the value.
    pub input: InputKind,
    /// What the field holds, for agents choosing a placeholder; an item may override it.
    pub description: &'static str,
    /// Shorter than `MIN_MASKED_CHARS` by nature (a card's security code): never masked, and
    /// storing one does not warn that it is too short to mask, since every value would (J-232).
    pub short: bool,
    /// Holds a one-time code seed (base32 or an `otpauth://` URI): checked when stored, and
    /// the source of `{{otp:item}}` (#206).
    pub otp_seed: bool,
}

/// One item kind: its stored name, domain, fields and defaults (02 §2.7.2). Adding a kind is
/// one entry here plus its `ItemKind` variant.
#[derive(Debug, PartialEq, Eq)]
pub struct KindSpec {
    pub kind: ItemKind,
    pub name: &'static str,
    pub domain: Domain,
    pub fields: &'static [FieldSpec],
    /// Field names are free-form (environment variable names), each treated as [`FREE_FIELD`].
    pub free_fields: bool,
    /// Besides its declared fields the kind takes fields the person names (`school`, `gpa`,
    /// a bank's second password), each treated as [`CUSTOM_FIELD`] unless the item marks it
    /// plain (J-111, R-48).
    pub custom_fields: bool,
    /// The field whose value KeyHarbor can make up instead of taking one (a login's password,
    /// `generate_password`); `None` when the kind has none. Always one of `fields`.
    pub generated: Option<&'static str>,
    pub default_origin_binding: bool,
    /// `keyharbor run` may inject the values unless the user turns it off: true for API keys
    /// and environment items, false for anything a site signs in or pays with (R-36).
    pub default_run_allowed: bool,
    /// A browser receives the values only in frames of the item's listed origins, and nowhere
    /// while it lists none (R-36): API keys, environment items and credentials (J-280).
    pub browser_needs_origin: bool,
}

/// How a field the person named is treated: secret and masked, like any value whose meaning
/// KeyHarbor cannot know. The item's plain mark makes one plain (R-48).
pub const CUSTOM_FIELD: FieldSpec = FieldSpec {
    name: "*",
    secret: true,
    maskable: true,
    input: InputKind::Text,
    description: "Custom field",
    short: false,
    otp_seed: false,
};

/// How a free-form field (an `env` variable) is treated: secret and masked.
pub const FREE_FIELD: FieldSpec = FieldSpec {
    name: "*",
    secret: true,
    maskable: true,
    input: InputKind::Text,
    description: "Environment variable",
    short: false,
    otp_seed: false,
};

const fn plain(name: &'static str, description: &'static str) -> FieldSpec {
    FieldSpec {
        name,
        secret: false,
        maskable: true,
        input: InputKind::Text,
        description,
        short: false,
        otp_seed: false,
    }
}
const fn short(spec: FieldSpec) -> FieldSpec {
    FieldSpec {
        short: true,
        ..spec
    }
}
const fn otp_seed(spec: FieldSpec) -> FieldSpec {
    FieldSpec {
        otp_seed: true,
        ..spec
    }
}
const fn secret(name: &'static str, description: &'static str) -> FieldSpec {
    FieldSpec {
        secret: true,
        ..plain(name, description)
    }
}
const fn password(name: &'static str, description: &'static str) -> FieldSpec {
    FieldSpec {
        input: InputKind::Password,
        ..secret(name, description)
    }
}

/// Every item kind. The single source for schemas, domains, origin-binding defaults, which
/// fields are secret and which are masked.
pub static KINDS: &[KindSpec] = &[
    KindSpec {
        kind: ItemKind::Login,
        name: "login",
        domain: Domain::Credentials,
        fields: &[
            plain("username", "Sign-in name or email"),
            password("password", "Sign-in password"),
            otp_seed(secret(
                "totp_seed",
                "One-time code seed: base32 or an otpauth:// URI",
            )),
            secret("note", "Free-form note"),
        ],
        free_fields: false,
        custom_fields: true,
        generated: Some("password"),
        default_origin_binding: true,
        default_run_allowed: false,
        browser_needs_origin: false,
    },
    KindSpec {
        kind: ItemKind::Secret,
        name: "secret",
        domain: Domain::Developer,
        fields: &[
            secret("value", "The secret value, e.g. an API key or token"),
            secret("note", "Free-form note"),
        ],
        free_fields: false,
        custom_fields: true,
        generated: None,
        default_origin_binding: false,
        default_run_allowed: true,
        browser_needs_origin: true,
    },
    KindSpec {
        kind: ItemKind::Env,
        name: "env",
        domain: Domain::Developer,
        fields: &[],
        free_fields: true,
        custom_fields: false,
        generated: None,
        default_origin_binding: false,
        default_run_allowed: true,
        browser_needs_origin: true,
    },
    KindSpec {
        kind: ItemKind::Card,
        name: "card",
        domain: Domain::Payment,
        fields: &[
            plain("holder", "Name on the card"),
            secret("number", "Card number"),
            plain("exp_month", "Expiry month"),
            plain("exp_year", "Expiry year"),
            short(secret("cvc", "Security code")),
            plain("billing_address", "Billing address item"),
            secret("note", "Free-form note"),
        ],
        free_fields: false,
        custom_fields: true,
        generated: None,
        default_origin_binding: true,
        default_run_allowed: false,
        browser_needs_origin: false,
    },
    KindSpec {
        kind: ItemKind::Identity,
        name: "identity",
        domain: Domain::Personal,
        fields: &[
            plain("full_name", "Full name"),
            plain("given_name", "Given name"),
            plain("family_name", "Family name"),
            plain("email", "Email address"),
            plain("phone", "Phone number"),
            plain("birth_date", "Date of birth"),
            plain("english_name", "Name in English letters, as on a passport"),
            plain("hanja_name", "Name in Chinese characters (hanja)"),
            plain("gender", "Gender as the application form asks it"),
            plain("nationality", "Nationality"),
            plain("emergency_phone", "Emergency contact phone number"),
            secret("military_service", "Military service status (sensitive)"),
            secret("disability", "Disability status (sensitive)"),
            secret("veterans", "Veterans or national merit status (sensitive)"),
            secret("note", "Free-form note"),
        ],
        free_fields: false,
        custom_fields: true,
        generated: None,
        default_origin_binding: false,
        default_run_allowed: false,
        browser_needs_origin: false,
    },
    KindSpec {
        kind: ItemKind::Address,
        name: "address",
        domain: Domain::Personal,
        fields: &[
            plain("recipient", "Recipient name"),
            plain("line1", "Street address"),
            plain("line2", "Apartment, suite, etc."),
            plain("city", "City"),
            plain("region", "State, province or region"),
            plain("postal_code", "Postal code"),
            plain("country", "Country"),
            plain("phone", "Phone number"),
            secret("note", "Free-form note"),
        ],
        free_fields: false,
        custom_fields: true,
        generated: None,
        default_origin_binding: false,
        default_run_allowed: false,
        browser_needs_origin: false,
    },
    // A credential with no fixed shape (R-48): a bank's second password, a member number, a
    // certificate password. Its fields are the ones the person names, often from a template.
    KindSpec {
        kind: ItemKind::Credential,
        name: "credential",
        domain: Domain::Credentials,
        fields: &[secret("note", "Free-form note")],
        free_fields: false,
        custom_fields: true,
        generated: None,
        default_origin_binding: false,
        default_run_allowed: false,
        browser_needs_origin: true,
    },
];

#[cfg(test)]
mod tests {
    use crate::vault::*;
    #[test]
    fn kinds_map_to_domains() {
        assert_eq!(ItemKind::Login.domain(), Domain::Credentials);
        assert_eq!(ItemKind::Env.domain(), Domain::Developer);
        assert_eq!(ItemKind::Secret.domain(), Domain::Developer);
        assert_eq!(Domain::Developer.as_byte(), 4);
        assert_eq!(Domain::from_byte(4), Some(Domain::Developer));
        assert_eq!(Domain::parse("developer"), Some(Domain::Developer));
        // The existing domains keep their stored numbers.
        assert_eq!(
            Domain::ALL[..3]
                .iter()
                .map(|d| d.as_byte())
                .collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(ItemKind::Card.domain(), Domain::Payment);
        assert_eq!(ItemKind::Address.domain(), Domain::Personal);
        assert_eq!(Domain::Payment.as_byte(), 2);
        assert!(ItemKind::Card.default_origin_binding());
        assert!(!ItemKind::Identity.default_origin_binding());
        assert!(ItemKind::Env.default_run_allowed() && ItemKind::Secret.default_run_allowed());
        assert!(!ItemKind::Login.default_run_allowed() && !ItemKind::Card.default_run_allowed());
    }
    #[test]
    fn kind_registry_is_complete_and_marks_secrets() {
        assert_eq!(KINDS.len(), ItemKind::ALL.len());
        for kind in ItemKind::ALL {
            assert_eq!(kind.spec().kind, kind);
        }
        let secrets: Vec<&str> = KINDS
            .iter()
            .flat_map(|k| k.fields)
            .filter(|f| f.secret)
            .map(|f| f.name)
            .collect();
        for name in ["password", "totp_seed", "value", "number", "cvc", "note"] {
            assert!(secrets.contains(&name), "{name}");
        }
        assert_eq!(ItemKind::Login.otp_field(), Some("totp_seed"));
        assert!(ItemKind::ALL[1..].iter().all(|k| k.otp_field().is_none()));
        assert!(!ItemKind::Login.is_secret_field("username"));
        assert!(
            ItemKind::Login.is_secret_field("cvc"),
            "secret in another kind"
        );
        assert!(ItemKind::Env.is_secret_field("API_KEY"));
        assert!(ItemKind::Card.field("number").unwrap().maskable);
        // A card takes fields the person names (R-48): `password` is one of those there.
        assert_eq!(ItemKind::Card.field("password"), Some(&CUSTOM_FIELD));
        assert_eq!(
            ItemKind::Login.field("password").unwrap().input,
            InputKind::Password
        );
        assert_eq!(
            ItemKind::Login.field("username").unwrap().input,
            InputKind::Text
        );
        assert_eq!(
            ItemKind::Env.field("API_KEY").unwrap().input,
            InputKind::Text
        );
    }
    #[test]
    fn identity_has_korean_application_fields() {
        for plain in [
            "english_name",
            "hanja_name",
            "gender",
            "nationality",
            "emergency_phone",
        ] {
            let spec = ItemKind::Identity.field(plain).unwrap();
            assert!(
                spec.name == plain && !spec.secret && spec.maskable,
                "{plain}"
            );
        }
        for secret in ["military_service", "disability", "veterans"] {
            let spec = ItemKind::Identity.field(secret).unwrap();
            assert!(
                spec.name == secret && spec.secret && spec.maskable,
                "{secret}"
            );
        }
    }
    #[test]
    fn custom_field_is_secret_and_masked() {
        // Every kind with named fields takes custom fields (R-48); env names its own.
        for kind in ItemKind::ALL.into_iter().filter(|k| *k != ItemKind::Env) {
            let spec = kind.field("school").unwrap();
            assert_eq!(spec, &CUSTOM_FIELD);
            assert!(spec.secret && spec.maskable);
            assert_eq!(spec.input, InputKind::Text);
            assert!(kind.is_secret_field("school"));
            assert!(kind.is_custom_field("school"), "{kind:?}");
            assert!(!kind.is_custom_field("note"), "{kind:?} declares note");
        }
        assert!(!ItemKind::Env.is_custom_field("SCHOOL"));
    }
    #[test]
    fn a_credential_is_custom_fields_that_need_an_origin_in_a_browser() {
        let spec = ItemKind::Credential.spec();
        assert_eq!(spec.domain, Domain::Credentials);
        assert_eq!(ItemKind::Credential.schema().unwrap(), ["note"]);
        assert!(spec.custom_fields && !spec.free_fields);
        assert!(!ItemKind::Credential.default_origin_binding());
        assert!(!ItemKind::Credential.default_run_allowed());
        assert!(ItemKind::Credential.needs_origin_for_browser());
        assert_eq!(ItemKind::parse("credential"), Some(ItemKind::Credential));
        // The R-36 rule as it was: developer items need an origin, the others follow binding.
        for kind in ItemKind::ALL {
            let developer = kind.domain() == Domain::Developer;
            if kind != ItemKind::Credential {
                assert_eq!(kind.needs_origin_for_browser(), developer, "{kind:?}");
            }
        }
    }
    #[test]
    fn custom_field_cannot_shadow_a_declared_secret() {
        // A custom field spelled like a declared field is that field; one spelled like another
        // kind's secret is still secret.
        assert!(!ItemKind::Identity.field("email").unwrap().secret);
        assert!(ItemKind::Identity.is_secret_field("password"));
        assert!(ItemKind::Identity.is_secret_field("cvc"));
    }
    #[test]
    fn only_a_login_generates_and_only_a_declared_secret_field() {
        assert_eq!(ItemKind::Login.generated_field(), Some("password"));
        for spec in KINDS {
            if let Some(field) = spec.generated {
                let declared = spec.fields.iter().find(|f| f.name == field);
                assert!(declared.is_some_and(|f| f.secret), "{}", spec.name);
            } else {
                assert_ne!(spec.kind, ItemKind::Login);
            }
        }
    }
    #[test]
    fn schemas_cover_every_kind() {
        assert!(ItemKind::Login.schema().unwrap().contains(&"password"));
        assert!(ItemKind::Card.schema().unwrap().contains(&"cvc"));
        assert!(ItemKind::Env.schema().is_none());
        for kind in [
            ItemKind::Login,
            ItemKind::Secret,
            ItemKind::Card,
            ItemKind::Identity,
            ItemKind::Address,
            ItemKind::Credential,
        ] {
            assert!(kind.schema().unwrap().contains(&"note"));
            assert_eq!(ItemKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(Domain::parse("payment"), Some(Domain::Payment));
    }
}
