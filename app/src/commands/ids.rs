//! Ids and names as the window sends them, parsed in one place so every command refuses a bad
//! one with the same words. The domain's own message passes through as it is.

use anyhow::{Context, Result};
use keyharbor_domain::{
    RequestId, SessionId,
    vault::{FieldName, ItemName},
};

pub(crate) fn request_id(id: &str) -> Result<RequestId> {
    RequestId::parse(id).context("invalid request id")
}

pub(crate) fn session_id(id: &str) -> Result<SessionId> {
    SessionId::parse(id).context("invalid session id")
}

/// An item that exists (or existed): any valid name.
pub(crate) fn item_name(name: &str) -> Result<ItemName> {
    Ok(ItemName::parse(name)?)
}

/// A name for a new item, or a new name for one (`ItemName::parse_new`).
pub(crate) fn new_item_name(name: &str) -> Result<ItemName> {
    Ok(ItemName::parse_new(name)?)
}

pub(crate) fn field_name(field: &str) -> Result<FieldName> {
    Ok(FieldName::parse(field)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bad_id_is_refused_in_the_same_words_everywhere() {
        assert_eq!(
            request_id("x").unwrap_err().to_string(),
            "invalid request id"
        );
        assert_eq!(
            session_id("x").unwrap_err().to_string(),
            "invalid session id"
        );
        assert!(item_name("github").is_ok() && item_name("").is_err());
        assert!(field_name("password").is_ok());
    }
}
