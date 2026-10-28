//! Reading and changing settings. Which settings exist, their defaults, formats and what
//! weakens a protection is the domain registry (`keyharbor_domain::settings::SETTINGS`);
//! a weakening change needs a person (R-35, `weaken`).

use crate::weaken;
use anyhow::{Result, anyhow};
use keyharbor_domain::{
    ports::{Confirm, SettingsStore},
    settings::{SETTINGS, SettingSpec},
};

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

/// Checks and stores a value, asking a person first if it weakens a protection.
pub fn set(store: &dyn SettingsStore, confirm: &dyn Confirm, key: &str, value: &str) -> Result<()> {
    spec(key)?.check(value).map_err(|e| anyhow!("{key} {e}"))?;
    weaken::require(confirm, weaken::setting(key, value).as_slice())?;
    store.setting_set(key, value)
}
