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
    Credentials,
    Payment,
    Personal,
}
impl Domain {
    pub const ALL: [Domain; 3] = [Self::Credentials, Self::Payment, Self::Personal];
    pub fn as_byte(self) -> u8 {
        match self {
            Self::Credentials => 1,
            Self::Payment => 2,
            Self::Personal => 3,
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
    pub fn domain(self) -> Domain {
        match self {
            Self::Login | Self::Secret | Self::Env => Domain::Credentials,
            Self::Card => Domain::Payment,
            Self::Identity | Self::Address => Domain::Personal,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Secret => "secret",
            Self::Env => "env",
            Self::Card => "card",
            Self::Identity => "identity",
            Self::Address => "address",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }
    /// Fixed field names; `None` means free-form environment variable names.
    pub fn schema(self) -> Option<&'static [&'static str]> {
        match self {
            Self::Login => Some(&["username", "password", "totp_seed", "note"]),
            Self::Secret => Some(&["value", "note"]),
            Self::Env => None,
            Self::Card => Some(&[
                "holder",
                "number",
                "exp_month",
                "exp_year",
                "cvc",
                "billing_address",
                "note",
            ]),
            Self::Identity => Some(&[
                "full_name",
                "given_name",
                "family_name",
                "email",
                "phone",
                "birth_date",
                "note",
            ]),
            Self::Address => Some(&[
                "recipient",
                "line1",
                "line2",
                "city",
                "region",
                "postal_code",
                "country",
                "phone",
                "note",
            ]),
        }
    }
    pub fn default_origin_binding(self) -> bool {
        matches!(self, Self::Login | Self::Card)
    }
}

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaceholderRef {
    pub item: ItemName,
    pub field: FieldName,
}

const PLACEHOLDER_OPEN: &str = "{{secret:";

/// Finds every `{{secret:<item>.<field>}}` reference. A malformed reference fails the whole text.
pub fn parse_placeholders(text: &str) -> Result<Vec<PlaceholderRef>, InvalidValue> {
    let mut refs = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(PLACEHOLDER_OPEN) {
        let after = &rest[start + PLACEHOLDER_OPEN.len()..];
        let end = after
            .find("}}")
            .ok_or(InvalidValue("unterminated placeholder"))?;
        let (item, field) = after[..end]
            .split_once('.')
            .ok_or(InvalidValue("placeholder must be {{secret:item.field}}"))?;
        refs.push(PlaceholderRef {
            item: ItemName::parse(item)?,
            field: FieldName::parse(field)?,
        });
        rest = &after[end + 2..];
    }
    Ok(refs)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kinds_map_to_domains() {
        assert_eq!(ItemKind::Login.domain(), Domain::Credentials);
        assert_eq!(ItemKind::Env.domain(), Domain::Credentials);
        assert_eq!(ItemKind::Card.domain(), Domain::Payment);
        assert_eq!(ItemKind::Address.domain(), Domain::Personal);
        assert_eq!(Domain::Payment.as_byte(), 2);
        assert!(ItemKind::Card.default_origin_binding());
        assert!(!ItemKind::Identity.default_origin_binding());
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
