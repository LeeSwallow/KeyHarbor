//! Per-session site sign-in states (02 §2.7). One record per (session, origin).

use crate::{SessionId, site::SiteRecord, vault::Origin};
use anyhow::Result;

pub trait SiteStore {
    /// The record for `origin` in `session`, if one was set.
    fn site_get(&self, session: SessionId, origin: &Origin) -> Result<Option<SiteRecord>>;
    /// Adds the record or replaces the one with the same session and origin.
    fn site_set(&self, record: &SiteRecord) -> Result<()>;
    /// Every record of `session`, ordered by origin.
    fn site_list(&self, session: SessionId) -> Result<Vec<SiteRecord>>;
}
