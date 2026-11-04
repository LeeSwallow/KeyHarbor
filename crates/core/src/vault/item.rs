//! The words the vault use cases speak: an item's status and source, what a new item or a
//! change carries, and the value-free summary callers may show.

use keyharbor_domain::{
    date::Expiry,
    tag::Tag,
    vault::{Description, Domain, FieldName, ItemKind, ItemName, Origin},
};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemStatus {
    Draft,
    Active,
    Discarded,
}
impl ItemStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Discarded => "discarded",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        [Self::Draft, Self::Active, Self::Discarded]
            .into_iter()
            .find(|s| s.as_str() == value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemSource {
    User,
    Import,
    Generated,
    Captured,
    Requested,
    Agent,
}
impl ItemSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Import => "import",
            Self::Generated => "generated",
            Self::Captured => "captured",
            Self::Requested => "requested",
            Self::Agent => "agent",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        [
            Self::User,
            Self::Import,
            Self::Generated,
            Self::Captured,
            Self::Requested,
            Self::Agent,
        ]
        .into_iter()
        .find(|s| s.as_str() == value)
    }
}

pub struct NewItem {
    pub name: ItemName,
    pub kind: ItemKind,
    pub fields: Vec<(FieldName, Zeroizing<String>)>,
    pub origins: Vec<Origin>,
    /// `None` uses the kind's default (on for login and card).
    pub origin_binding: Option<bool>,
    pub status: ItemStatus,
    pub source: ItemSource,
    pub details: ItemDetails,
}

/// Attributes a new item may set beyond its fields and origins; each defaults sensibly.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemDetails {
    /// `None` uses the kind's default (secret and env items yes, the others no; R-36).
    pub run_allowed: Option<bool>,
    pub expiry: ExpiryChoice,
    pub description: Option<Description>,
    /// Overrides of the kind's field descriptions; each names a field the item has.
    pub field_descriptions: Vec<(FieldName, Description)>,
    /// Labels to find the item by (developer secrets spec §2).
    pub tags: Vec<Tag>,
    /// Fields that are not secret: shown in listings, not masked (developer secrets spec §2).
    pub plain_fields: Vec<FieldName>,
    /// `None` allows agents to type the item's one-time codes while `agent_otp` is on (#206).
    pub otp_allowed: Option<bool>,
}

/// The expiry a new item gets (R-37).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExpiryChoice {
    /// Today plus `default_item_ttl` if that is set, otherwise never.
    #[default]
    Default,
    Never,
    On(Expiry),
}

#[derive(Default)]
pub struct ItemChange {
    pub set_fields: Vec<(FieldName, Zeroizing<String>)>,
    pub remove_fields: Vec<FieldName>,
    pub origins: Option<Vec<Origin>>,
    pub origin_binding: Option<bool>,
    pub run_allowed: Option<bool>,
    /// `Some(None)` removes the expiry.
    pub expiry: Option<Option<Expiry>>,
    /// `Some(None)` removes the description.
    pub description: Option<Option<Description>>,
    /// Sets (`Some`) or removes (`None`, back to the kind's default) a field's description.
    pub field_descriptions: Vec<(FieldName, Option<Description>)>,
    pub status: Option<ItemStatus>,
    pub add_tags: Vec<Tag>,
    pub remove_tags: Vec<Tag>,
    /// Mark fields not secret (weakens: a person confirms).
    pub make_plain: Vec<FieldName>,
    /// Mark fields secret again.
    pub make_secret: Vec<FieldName>,
    /// Let agents type the item's one-time codes (on weakens: a person confirms) or not.
    pub otp_allowed: Option<bool>,
}

/// What callers may show: never a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemSummary {
    pub name: String,
    pub kind: ItemKind,
    pub domain: Domain,
    pub fields: Vec<String>,
    pub origins: Vec<String>,
    pub origin_binding: bool,
    pub run_allowed: bool,
    /// Agents may type the item's one-time codes while `agent_otp` is on (#206).
    pub otp_allowed: bool,
    pub expires_at: Option<Expiry>,
    /// The expiry date has passed: agents may not use the item.
    pub expired: bool,
    pub description: Option<String>,
    /// Every field's description: the item's own or the kind's default.
    pub field_descriptions: BTreeMap<String, String>,
    /// Sorted.
    pub tags: Vec<String>,
    /// The values of fields marked not secret (empty for a tampered item).
    pub plain_values: BTreeMap<String, String>,
    pub status: ItemStatus,
    pub source: ItemSource,
    pub updated_at: String,
    /// Metadata MAC failed: the item is not used for input until re-signed.
    pub tampered: bool,
}
