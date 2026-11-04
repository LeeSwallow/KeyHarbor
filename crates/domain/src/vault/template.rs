//! Credential templates (R-48): a name and the fields a new `credential` item starts with,
//! each secret or plain. A template holds no value, so adding or removing one weakens nothing;
//! an item made from it keeps its fields when the template goes (J-281).

use super::{FieldName, InvalidValue, ItemKind};

/// What a person calls a template: `은행 2차 비밀번호`, `school-portal`.
#[derive(Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct TemplateName(String);

impl TemplateName {
    number_text!(
        /// The longest name, in characters.
        pub MAX_CHARS: usize = 40,
        RULE = "template name must be 1–", " letters, digits, spaces, '-' or '_'"
    );

    pub fn parse(value: &str) -> Result<Self, InvalidValue> {
        let value = value.trim();
        let count = value.chars().count();
        let allowed = |c: char| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_');
        if count == 0 || count > Self::MAX_CHARS || !value.chars().all(allowed) {
            return Err(InvalidValue(Self::RULE));
        }
        Ok(Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One field a template gives a new item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplateField {
    pub name: FieldName,
    /// Secret fields are masked and come only from a hidden prompt; plain ones show in
    /// listings (the item's plain mark).
    pub secret: bool,
}

/// A template: a name and its fields, in the order a form shows them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CredentialTemplate {
    name: TemplateName,
    fields: Vec<TemplateField>,
}

impl CredentialTemplate {
    number_text!(
        /// The most fields one template gives.
        pub MAX_FIELDS: usize = 32,
        TOO_MANY = "a template takes at most ", " fields"
    );

    /// A template of `fields`: at least one, each named once, none named like a field every
    /// credential already has (`note`): an item cannot hold two fields of one name.
    pub fn new(name: TemplateName, fields: Vec<TemplateField>) -> Result<Self, InvalidValue> {
        if fields.is_empty() {
            return Err(InvalidValue("a template needs at least one field"));
        }
        if fields.len() > Self::MAX_FIELDS {
            return Err(InvalidValue(Self::TOO_MANY));
        }
        for (i, field) in fields.iter().enumerate() {
            if !ItemKind::Credential.is_custom_field(field.name.as_str()) {
                return Err(InvalidValue(
                    "a template field cannot be named like a field every credential has",
                ));
            }
            if fields[..i].iter().any(|f| f.name == field.name) {
                return Err(InvalidValue("a template names each field once"));
            }
        }
        Ok(Self { name, fields })
    }
    pub fn name(&self) -> &TemplateName {
        &self.name
    }
    pub fn fields(&self) -> &[TemplateField] {
        &self.fields
    }
    /// The fields a new item marks plain.
    pub fn plain_fields(&self) -> Vec<FieldName> {
        self.fields
            .iter()
            .filter(|f| !f.secret)
            .map(|f| f.name.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, secret: bool) -> TemplateField {
        TemplateField {
            name: FieldName::parse(name).unwrap(),
            secret,
        }
    }
    fn name(text: &str) -> TemplateName {
        TemplateName::parse(text).unwrap()
    }

    #[test]
    fn template_names_are_short_words() {
        assert_eq!(name(" 은행 2차 비밀번호 ").as_str(), "은행 2차 비밀번호");
        assert!(TemplateName::parse("school-portal_1").is_ok());
        for bad in ["", "   ", "a/b", "x<y", &"a".repeat(41)] {
            assert!(TemplateName::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_template_keeps_its_order_and_marks_plain_fields() {
        let t = CredentialTemplate::new(
            name("bank"),
            vec![field("member_no", false), field("second_password", true)],
        )
        .unwrap();
        assert_eq!(t.fields()[0].name.as_str(), "member_no");
        assert_eq!(t.plain_fields(), [FieldName::parse("member_no").unwrap()]);
    }

    #[test]
    fn a_template_refuses_no_fields_twice_a_field_and_a_declared_name() {
        assert!(CredentialTemplate::new(name("x"), vec![]).is_err());
        let twice = vec![field("pin", true), field("pin", false)];
        assert!(CredentialTemplate::new(name("x"), twice).is_err());
        assert!(CredentialTemplate::new(name("x"), vec![field("note", true)]).is_err());
        let many = (0..=CredentialTemplate::MAX_FIELDS)
            .map(|i| field(&format!("f{i}"), true))
            .collect();
        assert!(CredentialTemplate::new(name("x"), many).is_err());
    }
}
