//! Settings: which keys exist, their defaults and valid values, and changing them. Turning a
//! protection down needs a person (R-35, `weaken`).

use crate::{DEFAULT_ITEM_TTL, NO_TTL, weaken};
use anyhow::{Result, anyhow, bail};
use keyharbor_domain::{
    date::Ttl,
    ports::{AUDIT_ENABLED, Confirm, SettingsStore},
};

/// One setting: its default and how a value is checked.
pub struct Setting {
    pub key: &'static str,
    pub default: &'static str,
    valid: fn(&str) -> Result<()>,
}

fn boolean(value: &str) -> Result<()> {
    if !matches!(value, "true" | "false") {
        bail!("must be true or false");
    }
    Ok(())
}

fn ttl(value: &str) -> Result<()> {
    if value != NO_TTL {
        Ttl::parse(value).map_err(|e| anyhow!("{e}, or none"))?;
    }
    Ok(())
}

/// Every setting.
pub const SETTINGS: &[Setting] = &[
    Setting {
        key: AUDIT_ENABLED,
        default: "true",
        valid: boolean,
    },
    Setting {
        key: weaken::CAPTURE_ENABLED,
        default: "false",
        valid: boolean,
    },
    Setting {
        key: DEFAULT_ITEM_TTL,
        default: NO_TTL,
        valid: ttl,
    },
];

fn setting(key: &str) -> Result<&'static Setting> {
    SETTINGS.iter().find(|s| s.key == key).ok_or_else(|| {
        let known: Vec<&str> = SETTINGS.iter().map(|s| s.key).collect();
        anyhow!("unknown setting; known settings: {}", known.join(", "))
    })
}

/// The setting's value, or its default when it was never set.
pub fn get(store: &dyn SettingsStore, key: &str) -> Result<String> {
    let setting = setting(key)?;
    Ok(store
        .setting_get(key)?
        .unwrap_or_else(|| setting.default.to_owned()))
}

/// Checks and stores a value. Turning the usage record off or changing capture needs a
/// person (R-35).
pub fn set(store: &dyn SettingsStore, confirm: &dyn Confirm, key: &str, value: &str) -> Result<()> {
    (setting(key)?.valid)(value).map_err(|e| anyhow!("{key} {e}"))?;
    weaken::require(confirm, weaken::setting(key, value).as_slice())?;
    store.setting_set(key, value)
}
