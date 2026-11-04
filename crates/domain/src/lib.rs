//! KeyHarbor's vocabulary, rules and ports (ADR-0014). One module per context — `vault`,
//! `session`, `site`, `access` — plus what every context shares (`audit`, `date`, `secret`,
//! `settings`, `request`) and the ports the outside implements.

/// A UUID newtype: built from 16 random bytes in UUID v4 layout, parsed from and shown as
/// the hyphenated form. New ids come from `keyharbor_crypto::random::id()` (ADR-0016).
macro_rules! uuid_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
        pub struct $name(::uuid::Uuid);
        impl $name {
            /// From 16 random bytes in UUID v4 layout; new ids come from
            /// `keyharbor_crypto::random::id()` (ADR-0016).
            pub fn from_bytes(bytes: [u8; 16]) -> Self {
                Self(::uuid::Uuid::from_bytes(bytes))
            }
            pub fn parse(value: &str) -> Result<Self, ::uuid::Error> {
                Ok(Self(::uuid::Uuid::parse_str(value)?))
            }
        }
        impl From<[u8; 16]> for $name {
            fn from(bytes: [u8; 16]) -> Self {
                Self::from_bytes(bytes)
            }
        }
        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

/// A number constant and a text built around the same literal, so an error message or a
/// stored default cannot drift from the number it names (`concat!` takes only literals; J-171).
macro_rules! number_text {
    (
        $(#[$meta:meta])* $vis:vis $name:ident: $ty:ty = $n:literal,
        $text:ident = $before:literal, $after:literal
    ) => {
        $(#[$meta])*
        $vis const $name: $ty = $n;
        const $text: &'static str = concat!($before, $n, $after);
    };
}

pub mod access;
pub mod audit;
pub mod date;
pub mod ports;
pub mod request;
pub mod secret;
pub mod session;
pub mod settings;
pub mod site;
pub mod vault;

// The paths these had before the contexts became folders (#163).
pub use access::{denial, grant, policy};
pub use session::{
    Generation, HandoffStatus, InvalidName, RequestId, Session, SessionId, SessionName, State,
    browser_choice, upload, validate_navigation_url, validate_start_url,
};
pub use site::handoff_site;
pub use vault::{item_pattern, password, tag};
