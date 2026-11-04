//! Reading and changing settings. Which settings exist, their defaults, formats and what
//! weakens a protection is the domain registry (`keyharbor_domain::settings::SETTINGS`);
//! a weakening change needs a person (R-35, `weaken`).

use crate::weaken;
use anyhow::{Result, anyhow};
use keyharbor_domain::{
    ports::{Confirm, SettingsStore},
    settings::{SETTINGS, SettingSpec},
};
use std::time::Duration;

fn spec(key: &str) -> Result<&'static SettingSpec> {
    SettingSpec::find(key).ok_or_else(|| {
        let known: Vec<&str> = SETTINGS.iter().map(|s| s.key).collect();
        anyhow!("unknown setting; known settings: {}", known.join(", "))
    })
}

/// The setting's value, or its default when it was never set.
pub fn get(store: &dyn SettingsStore, key: &str) -> Result<String> {
    let spec = spec(key)?;
    Ok(store
        .setting_get(key)?
        .unwrap_or_else(|| spec.default.to_owned()))
}

/// A number setting's value (or default). A stored value out of its range is an error naming
/// the setting, never the value.
pub fn number(store: &dyn SettingsStore, key: &str) -> Result<u64> {
    spec(key)?
        .number(store.setting_get(key)?.as_deref())
        .map_err(|e| anyhow!("invalid {key}: {e}"))
}

/// A setting counted in seconds, as a wait.
pub fn seconds(store: &dyn SettingsStore, key: &str) -> Result<Duration> {
    number(store, key).map(Duration::from_secs)
}

/// Whether agents may type one-time codes (`agent_otp`, #206): off unless set to `true`.
pub fn agent_otp(store: &dyn SettingsStore) -> Result<bool> {
    Ok(keyharbor_domain::settings::agent_otp_enabled(
        store
            .setting_get(keyharbor_domain::settings::AGENT_OTP)?
            .as_deref(),
    ))
}

/// Checks and stores a value, asking a person first if it weakens a protection.
pub fn set(store: &dyn SettingsStore, confirm: &dyn Confirm, key: &str, value: &str) -> Result<()> {
    spec(key)?.check(value).map_err(|e| anyhow!("{key} {e}"))?;
    weaken::require(confirm, weaken::setting(key, value).as_slice())?;
    store.setting_set(key, value)
}
