//! Every setting in one table (`SETTINGS`): its name, default, format, and which change
//! weakens a protection (R-35). The store keeps strings; meaning is read from here only.

use crate::date::Ttl;
use std::fmt;

/// Turns the usage record off (`false`) or on (anything else, and the default; J-23, J-73).
pub const AUDIT_ENABLED: &str = "audit_enabled";
/// Turns credential capture (phase 4) on or off.
pub const CAPTURE_ENABLED: &str = "capture_enabled";
/// Gives new items an expiry, e.g. `90d` (R-37). `none` (the default): they never expire.
pub const DEFAULT_ITEM_TTL: &str = "default_item_ttl";
/// How long an unconfirmed draft is kept before `prune-drafts` removes it (08 §8.3). `none`:
/// until a person confirms or discards it.
pub const DRAFT_TTL: &str = "draft_ttl";
/// Seconds a value copied in the admin app stays on the clipboard (09 §9.4.7). Longer weakens.
pub const CLIPBOARD_CLEAR_SECONDS: &str = "clipboard_clear_seconds";
/// The range `clipboard_clear_seconds` takes: long enough to paste, short enough to matter.
const CLEAR_SECONDS_RANGE: std::ops::RangeInclusive<u64> = 5..=300;
const CLEAR_SECONDS_DEFAULT: u64 = 30;
/// The `default_item_ttl` value meaning "never expire".
pub const NO_TTL: &str = "none";

/// What values a setting takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingFormat {
    /// `true` or `false`.
    Boolean,
    /// `<N>d` or `none`.
    Ttl,
    /// Whole seconds within [`CLEAR_SECONDS_RANGE`].
    ClearSeconds,
}

/// Which change to a setting weakens a protection and needs a person.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingWeakening {
    /// Setting this value turns the protection down.
    WhenSetTo(&'static str),
    /// A number above the default turns the protection down.
    AboveDefault,
    /// Any change, either way (capture: on collects more, off is still a change of mode).
    AnyChange,
    Never,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettingSpec {
    pub key: &'static str,
    /// The value while it was never set.
    pub default: &'static str,
    pub format: SettingFormat,
    pub weakening: SettingWeakening,
}

/// Every setting.
pub const SETTINGS: &[SettingSpec] = &[
    SettingSpec {
        key: AUDIT_ENABLED,
        default: "true",
        format: SettingFormat::Boolean,
        weakening: SettingWeakening::WhenSetTo("false"),
    },
    SettingSpec {
        key: CAPTURE_ENABLED,
        default: "false",
        format: SettingFormat::Boolean,
        weakening: SettingWeakening::AnyChange,
    },
    SettingSpec {
        key: DEFAULT_ITEM_TTL,
        default: NO_TTL,
        format: SettingFormat::Ttl,
        weakening: SettingWeakening::Never,
    },
    SettingSpec {
        key: DRAFT_TTL,
        default: "7d",
        format: SettingFormat::Ttl,
        weakening: SettingWeakening::Never,
    },
    SettingSpec {
        key: CLIPBOARD_CLEAR_SECONDS,
        default: "30", // CLEAR_SECONDS_DEFAULT
        format: SettingFormat::ClearSeconds,
        weakening: SettingWeakening::AboveDefault,
    },
];

/// A value a setting does not take. The message names the format, never the value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidSetting(String);
impl fmt::Display for InvalidSetting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for InvalidSetting {}

impl SettingSpec {
    pub fn find(key: &str) -> Option<&'static SettingSpec> {
        SETTINGS.iter().find(|s| s.key == key)
    }

    /// `Ok` if `value` has this setting's format.
    pub fn check(&self, value: &str) -> Result<(), InvalidSetting> {
        match self.format {
            SettingFormat::Boolean if matches!(value, "true" | "false") => Ok(()),
            SettingFormat::Boolean => Err(InvalidSetting("must be true or false".into())),
            SettingFormat::Ttl if value == NO_TTL => Ok(()),
            SettingFormat::Ttl => Ttl::parse(value)
                .map(|_| ())
                .map_err(|e| InvalidSetting(format!("{e}, or none"))),
            SettingFormat::ClearSeconds => match clear_seconds(value) {
                Some(_) => Ok(()),
                None => Err(InvalidSetting(format!(
                    "must be whole seconds from {} to {}",
                    CLEAR_SECONDS_RANGE.start(),
                    CLEAR_SECONDS_RANGE.end()
                ))),
            },
        }
    }

    /// Whether setting `value` turns this protection down.
    pub fn weakens(&self, value: &str) -> bool {
        match self.weakening {
            SettingWeakening::WhenSetTo(off) => value == off,
            SettingWeakening::AboveDefault => {
                value.parse::<u64>().ok() > self.default.parse::<u64>().ok()
            }
            SettingWeakening::AnyChange => true,
            SettingWeakening::Never => false,
        }
    }
}

/// `value` as seconds, if it is a whole number in range.
fn clear_seconds(value: &str) -> Option<u64> {
    let seconds = value.parse::<u64>().ok()?;
    CLEAR_SECONDS_RANGE.contains(&seconds).then_some(seconds)
}

/// How long a copied value stays for a stored `clipboard_clear_seconds`. A value the setting
/// does not take (only a tampered store holds one) falls back to the default.
pub fn clipboard_clear_after(stored: Option<&str>) -> std::time::Duration {
    let seconds = stored.and_then(clear_seconds);
    std::time::Duration::from_secs(seconds.unwrap_or(CLEAR_SECONDS_DEFAULT))
}

/// Whether capture is on for a stored `capture_enabled`: only an explicit `true` (off by
/// default, R-36).
pub fn capture_enabled(stored: Option<&str>) -> bool {
    stored == Some("true")
}

/// Whether the usage record is on for a stored `audit_enabled` (`None` when never set): only
/// an explicit `false` turns it off, so an odd stored value fails safe (J-73).
pub fn audit_enabled(stored: Option<&str>) -> bool {
    stored != Some("false")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_checks_values_and_names_weakenings() {
        let audit = SettingSpec::find(AUDIT_ENABLED).unwrap();
        assert_eq!(audit.default, "true");
        assert!(audit.check("false").is_ok());
        assert_eq!(
            audit.check("off").unwrap_err().to_string(),
            "must be true or false"
        );
        assert!(audit.weakens("false") && !audit.weakens("true"));
        let capture = SettingSpec::find(CAPTURE_ENABLED).unwrap();
        assert!(capture.weakens("false") && capture.weakens("true"));
        let ttl = SettingSpec::find(DEFAULT_ITEM_TTL).unwrap();
        assert!(ttl.check("90d").is_ok() && ttl.check(NO_TTL).is_ok());
        assert!(
            ttl.check("90")
                .unwrap_err()
                .to_string()
                .ends_with(", or none")
        );
        assert!(!ttl.weakens("1d"));
        assert!(SettingSpec::find("nope").is_none());
    }

    #[test]
    fn the_clipboard_time_is_bounded_and_raising_it_weakens() {
        let clear = SettingSpec::find(CLIPBOARD_CLEAR_SECONDS).unwrap();
        assert_eq!(clear.default, "30");
        assert_eq!(clear.default, CLEAR_SECONDS_DEFAULT.to_string());
        for ok in ["5", "30", "300"] {
            assert!(clear.check(ok).is_ok(), "{ok}");
        }
        for bad in ["4", "301", "30s", "-1", ""] {
            assert_eq!(
                clear.check(bad).unwrap_err().to_string(),
                "must be whole seconds from 5 to 300",
                "{bad}"
            );
        }
        assert!(clear.weakens("31") && !clear.weakens("30") && !clear.weakens("10"));
        assert_eq!(clipboard_clear_after(None).as_secs(), 30);
        assert_eq!(clipboard_clear_after(Some("10")).as_secs(), 10);
        assert_eq!(clipboard_clear_after(Some("9999")).as_secs(), 30);
    }

    #[test]
    fn only_false_turns_the_usage_record_off() {
        assert!(audit_enabled(None));
        assert!(audit_enabled(Some("true")));
        assert!(audit_enabled(Some("TRUE")));
        assert!(!audit_enabled(Some("false")));
    }
}
