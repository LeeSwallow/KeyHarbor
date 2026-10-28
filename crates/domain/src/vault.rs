use std::{error::Error, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidValue(pub &'static str);
impl fmt::Display for InvalidValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl Error for InvalidValue {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum Domain {
    /// Site sign-ins (`login`): typed only into the sites they belong to.
    Credentials,
    Payment,
    Personal,
    /// API keys and environment variables (`secret`, `env`): injected into processes and
    /// tools (R-40).
    Developer,
}
impl Domain {
    pub const ALL: [Domain; 4] = [
        Self::Credentials,
        Self::Payment,
        Self::Personal,
        Self::Developer,
    ];
    pub fn as_byte(self) -> u8 {
        match self {
            Self::Credentials => 1,
            Self::Payment => 2,
            Self::Personal => 3,
            Self::Developer => 4,
        }
    }
    pub fn from_byte(value: u8) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|domain| domain.as_byte() == value)
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Credentials => "credentials",
            Self::Payment => "payment",
            Self::Personal => "personal",
            Self::Developer => "developer",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|domain| domain.as_str() == value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ItemKind {
    Login,
    Secret,
    Env,
    Card,
    Identity,
    Address,
}
impl ItemKind {
    pub const ALL: [ItemKind; 6] = [
        Self::Login,
        Self::Secret,
        Self::Env,
        Self::Card,
        Self::Identity,
        Self::Address,
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
    pub fn default_origin_binding(self) -> bool {
        self.spec().default_origin_binding
    }
    /// Whether `keyharbor run` may inject a new item of this kind (R-35, R-36).
    pub fn default_run_allowed(self) -> bool {
        self.spec().default_run_allowed
    }
    /// How `field` is treated in an item of this kind: its declared spec, the free-field spec
    /// for kinds with free-form names, or `None` if the kind has no such field.
    pub fn field(self, field: &str) -> Option<&'static FieldSpec> {
        let spec = self.spec();
        spec.fields
            .iter()
            .find(|f| f.name == field)
            .or(spec.free_fields.then_some(&FREE_FIELD))
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
            None => KINDS
                .iter()
                .flat_map(|k| k.fields)
                .any(|f| f.name == field && f.secret),
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
    pub default_origin_binding: bool,
    /// `keyharbor run` may inject the values unless the user turns it off: true for API keys
    /// and environment items, false for anything a site signs in or pays with (R-36).
    pub default_run_allowed: bool,
}

/// How a free-form field (an `env` variable) is treated: secret and masked.
pub const FREE_FIELD: FieldSpec = FieldSpec {
    name: "*",
    secret: true,
    maskable: true,
    input: InputKind::Text,
    description: "Environment variable",
};

const fn plain(name: &'static str, description: &'static str) -> FieldSpec {
    FieldSpec {
        name,
        secret: false,
        maskable: true,
        input: InputKind::Text,
        description,
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
            secret("totp_seed", "One-time code seed (stored only)"),
            secret("note", "Free-form note"),
        ],
        free_fields: false,
        default_origin_binding: true,
        default_run_allowed: false,
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
        default_origin_binding: false,
        default_run_allowed: true,
    },
    KindSpec {
        kind: ItemKind::Env,
        name: "env",
        domain: Domain::Developer,
        fields: &[],
        free_fields: true,
        default_origin_binding: false,
        default_run_allowed: true,
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
            secret("cvc", "Security code"),
            plain("billing_address", "Billing address item"),
            secret("note", "Free-form note"),
        ],
        free_fields: false,
        default_origin_binding: true,
        default_run_allowed: false,
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
            secret("note", "Free-form note"),
        ],
        free_fields: false,
        default_origin_binding: false,
        default_run_allowed: false,
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
        default_origin_binding: false,
        default_run_allowed: false,
    },
];

#[derive(Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct ItemName(String);
impl ItemName {
    pub fn parse(value: &str) -> Result<Self, InvalidValue> {
        let count = value.chars().count();
        if count == 0
            || count > 64
            || !value
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(InvalidValue(
                "item name must be 1–64 letters, digits, '-' or '_'",
            ));
        }
        Ok(Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct FieldName(String);
impl FieldName {
    pub fn parse(value: &str) -> Result<Self, InvalidValue> {
        if value.is_empty()
            || value.len() > 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(InvalidValue(
                "field name must be 1–64 ASCII letters, digits or '_'",
            ));
        }
        Ok(Self(value.to_owned()))
    }
    /// Environment variable names: uppercase letters, digits, '_', not starting with a digit.
    pub fn parse_env(value: &str) -> Result<Self, InvalidValue> {
        let name = Self::parse(value)?;
        let valid = value
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            && !value.as_bytes()[0].is_ascii_digit();
        if !valid {
            return Err(InvalidValue(
                "environment variable names use A–Z, 0–9 and '_' and must not start with a digit",
            ));
        }
        Ok(name)
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct Origin(String);
impl Origin {
    pub fn parse(value: &str) -> Result<Self, InvalidValue> {
        let url = url::Url::parse(value).map_err(|_| InvalidValue("invalid origin"))?;
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
        {
            return Err(InvalidValue(
                "origin must not include a path, query, fragment, or user information",
            ));
        }
        let local = match url.host() {
            Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        };
        if url.host().is_none() || !(url.scheme() == "https" || (url.scheme() == "http" && local)) {
            return Err(InvalidValue("origin must use HTTPS or HTTP on localhost"));
        }
        Ok(Self(url.origin().ascii_serialization()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What an item, a field or a session is for, so an agent can tell when to use it (R-38).
/// 1–500 characters, no control characters but newlines. Not a secret: it is stored in
/// plaintext (signed in the metadata MAC for items) and shown to agents.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Description(String);

impl Description {
    pub const MAX_CHARS: usize = 500;

    pub fn parse(value: &str) -> Result<Self, InvalidValue> {
        let value = value.trim();
        let count = value.chars().count();
        if count == 0 || count > Self::MAX_CHARS {
            return Err(InvalidValue("a description is 1–500 characters"));
        }
        if value.chars().any(|c| c.is_control() && c != '\n') {
            return Err(InvalidValue("a description has no control characters"));
        }
        Ok(Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaceholderRef {
    pub item: ItemName,
    pub field: FieldName,
}

/// Where a placeholder starts and ends: `{{secret:<item>.<field>}}`. Every other crate
/// writes and finds placeholders through these and `Display`, never its own literal.
pub const PLACEHOLDER_OPEN: &str = "{{secret:";
pub const PLACEHOLDER_CLOSE: &str = "}}";

/// The placeholder text for a field of an item, e.g. `{{secret:github.password}}`.
pub fn placeholder(item: &str, field: &str) -> String {
    format!("{PLACEHOLDER_OPEN}{item}.{field}{PLACEHOLDER_CLOSE}")
}

/// Whether `text` holds anything that starts a placeholder.
pub fn has_placeholder(text: &str) -> bool {
    text.contains(PLACEHOLDER_OPEN)
}

impl fmt::Display for PlaceholderRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&placeholder(self.item.as_str(), self.field.as_str()))
    }
}

/// Finds every `{{secret:<item>.<field>}}` reference. A malformed reference fails the whole text.
pub fn parse_placeholders(text: &str) -> Result<Vec<PlaceholderRef>, InvalidValue> {
    let mut refs = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(PLACEHOLDER_OPEN) {
        let after = &rest[start + PLACEHOLDER_OPEN.len()..];
        let end = after
            .find(PLACEHOLDER_CLOSE)
            .ok_or(InvalidValue("unterminated placeholder"))?;
        let (item, field) = after[..end]
            .split_once('.')
            .ok_or(InvalidValue("placeholder must be {{secret:item.field}}"))?;
        refs.push(PlaceholderRef {
            item: ItemName::parse(item)?,
            field: FieldName::parse(field)?,
        });
        rest = &after[end + PLACEHOLDER_CLOSE.len()..];
    }
    Ok(refs)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn placeholders_display_as_they_parse() {
        let text = "u={{secret:github.username}}";
        let refs = parse_placeholders(text).unwrap();
        assert_eq!(refs[0].to_string(), "{{secret:github.username}}");
        assert_eq!(placeholder("github", "username"), refs[0].to_string());
        assert!(has_placeholder(text) && !has_placeholder("{secret:x.y}"));
    }
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
    fn item_names_reject_placeholder_syntax() {
        let long = "a".repeat(65);
        for bad in [
            "",
            " ",
            "my.github",
            "a}b",
            "a{b",
            "a b",
            "x\n",
            long.as_str(),
        ] {
            assert!(ItemName::parse(bad).is_err(), "{bad:?}");
        }
        let max = "a".repeat(64);
        for good in ["github", "my-visa", "회사_메일", max.as_str()] {
            assert!(ItemName::parse(good).is_ok(), "{good:?}");
        }
    }
    #[test]
    fn descriptions_are_short_plain_text() {
        assert_eq!(
            Description::parse("  Work GitHub, for CI  ")
                .unwrap()
                .as_str(),
            "Work GitHub, for CI"
        );
        assert!(Description::parse("two\nlines").is_ok());
        assert!(Description::parse(&"가".repeat(500)).is_ok());
        for bad in ["", "   ", "tab\there", "\u{1b}[31m"] {
            assert!(Description::parse(bad).is_err(), "{bad:?}");
        }
        assert!(Description::parse(&"x".repeat(501)).is_err());
        assert!(
            KINDS
                .iter()
                .flat_map(|k| k.fields)
                .all(|f| !f.description.is_empty())
        );
    }

    #[test]
    fn origins_are_normalized() {
        assert_eq!(
            Origin::parse("HTTPS://GitHub.com:443").unwrap().as_str(),
            "https://github.com"
        );
        assert_eq!(
            Origin::parse("http://localhost:8080").unwrap().as_str(),
            "http://localhost:8080"
        );
        assert_eq!(
            Origin::parse("https://한국.kr").unwrap().as_str(),
            "https://xn--3e0b707e.kr"
        );
        for bad in [
            "github.com",
            "https://github.com/login",
            "https://u:p@github.com",
            "ftp://x.com",
            "http://example.com",
        ] {
            assert!(Origin::parse(bad).is_err(), "{bad:?}");
        }
    }
    #[test]
    fn placeholders_parse_and_reject_malformed() {
        let refs =
            parse_placeholders("id={{secret:github.username}} pw={{secret:github.password}}")
                .unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[1].item.as_str(), "github");
        assert_eq!(refs[1].field.as_str(), "password");
        assert!(parse_placeholders("plain text").unwrap().is_empty());
        for bad in [
            "{{secret:github}}",
            "{{secret:.x}}",
            "{{secret:a.b",
            "{{secret:a b.c}}",
        ] {
            assert!(parse_placeholders(bad).is_err(), "{bad:?}");
        }
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
        assert!(!ItemKind::Login.is_secret_field("username"));
        assert!(
            ItemKind::Login.is_secret_field("cvc"),
            "secret in another kind"
        );
        assert!(ItemKind::Env.is_secret_field("API_KEY"));
        assert!(ItemKind::Card.field("number").unwrap().maskable);
        assert!(ItemKind::Card.field("password").is_none());
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
        ] {
            assert!(kind.schema().unwrap().contains(&"note"));
            assert_eq!(ItemKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(Domain::parse("payment"), Some(Domain::Payment));
    }
}
