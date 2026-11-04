//! The vault's vocabulary: item kinds, names, status and source, origins, descriptions,
//! placeholders, tags, run patterns, generated passwords and secret values masking handles
//! badly. Eleven files, three past the folder line: each is one concept the issues (#163,
//! #191, #200) asked to keep apart, and none groups with another (J-170, J-230, J-260).

mod common_value;
mod description;
mod item;
pub mod item_pattern;
mod kind;
mod name;
mod origin;
pub mod password;
mod placeholder;
pub mod tag;

pub use common_value::{
    CommonValueReason, CommonValueWarning, MIN_MASKED_CHARS, common_value_warnings,
};
pub use description::Description;
pub use item::{ItemSource, ItemStatus};
pub use kind::{CUSTOM_FIELD, FREE_FIELD, FieldSpec, InputKind, ItemKind, KINDS, KindSpec};
pub use name::{DRAFT_MARK, FieldName, ItemName};
pub use origin::Origin;
pub(crate) use origin::is_https_or_local_http;
pub use placeholder::{
    PLACEHOLDER_CLOSE, PLACEHOLDER_OPEN, PlaceholderRef, has_placeholder, parse_placeholders,
    placeholder,
};

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
