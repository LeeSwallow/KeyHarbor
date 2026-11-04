//! Setting strings. What they mean is `crate::settings`.

use anyhow::Result;

pub use crate::settings::AUDIT_ENABLED;

/// Stores setting strings; what they mean is `crate::settings`.
pub trait SettingsStore {
    fn setting_get(&self, key: &str) -> Result<Option<String>>;
    fn setting_set(&self, key: &str, value: &str) -> Result<()>;
}
