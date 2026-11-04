//! Arguments several commands share: descriptions, expiry, the browser choice, and value
//! parsers that read their choices from a registry instead of an enum of their own.

mod browser;
mod choice;
mod description;
mod expiry;

pub use browser::parse_browser;
pub use choice::choice;
pub use description::{DescriptionArgs, parse_description};
pub use expiry::ExpiryArgs;
