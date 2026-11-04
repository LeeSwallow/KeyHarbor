//! Every setting in one table (`SETTINGS`): its name, default, format, and which change
//! weakens a protection (R-35). The store keeps strings; meaning is read from here only.

use crate::date::Ttl;
use std::{fmt, ops::RangeInclusive};

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
const CLEAR_SECONDS_RANGE: RangeInclusive<u64> = 5..=300;
number_text!(CLEAR_SECONDS_DEFAULT: u64 = 30, CLEAR_SECONDS_DEFAULT_TEXT = "", "");
/// macOS notifications from the admin app, per kind (09 §9.3). On unless `false`. Turning one
/// off hides nothing: what waits stays on the home screen and the menu bar.
pub const NOTIFY_REQUESTS: &str = "notify_requests";
pub const NOTIFY_HANDOFFS: &str = "notify_handoffs";
pub const NOTIFY_DRAFTS: &str = "notify_drafts";
pub const NOTIFY_DENIALS: &str = "notify_denials";
/// A session stopped substituting (`substitution_locked`, #190).
pub const NOTIFY_LOCKS: &str = "notify_locks";
/// Folders `browser_upload` may take files from, separated by `:` (03 §3.14). Empty (the
/// default): uploads are refused. Any change needs a person: it decides what may leave.
pub const UPLOAD_DIRS: &str = "upload_dirs";
/// The most files one `browser_upload` may put into a page. More weakens.
pub const UPLOAD_MAX_FILES: &str = "upload_max_files";
const UPLOAD_MAX_FILES_RANGE: RangeInclusive<u64> = 1..=50;

/// Which browser a session opens in when nobody says: `auto` (ego lite if it answers, else
/// Chrome), `chrome`, `ego-lite` or an absolute path (#120, R-47(1)). Not a weakening: the
/// person chose `auto` as the default; a policy can deny ego lite.
pub const BROWSER: &str = "browser";
/// How long `auto` waits for ego lite to answer before it uses Chrome.
pub const EGO_PROBE_SECONDS: &str = "ego_probe_seconds";
const EGO_PROBE_RANGE: RangeInclusive<u64> = 1..=30;

/// How often (ms) the ego lite relay takes page events (ADR-0018). Shorter reacts faster,
/// longer costs less; ego offers no push.
pub const EGO_POLL_MS: &str = "ego_poll_ms";
const EGO_POLL_RANGE: RangeInclusive<u64> = 5..=1000;

/// Seconds `open` waits for a new session's supervisor to report the browser running (#201).
/// A slow machine's first Chrome start needs more than the default (#183). Waiting longer
/// protects nothing less, so no change is a weakening (J-270).
pub const SESSION_START_SECONDS: &str = "session_start_seconds";
const SESSION_START_RANGE: RangeInclusive<u64> = 5..=300;
/// Seconds `stop` waits for the browser to exit. Above the supervisor's close grace (10 s,
/// then it kills the browser), or `stop` would report a failure while the supervisor is
/// still closing on schedule (J-271).
pub const SESSION_STOP_SECONDS: &str = "session_stop_seconds";
const SESSION_STOP_RANGE: RangeInclusive<u64> = 15..=300;
/// Seconds the raw CDP endpoint waits for a `--cdp` browser to write `DevToolsActivePort`.
pub const BROWSER_ENDPOINT_SECONDS: &str = "browser_endpoint_seconds";
const BROWSER_ENDPOINT_RANGE: RangeInclusive<u64> = 5..=120;
/// Seconds one CDP call (and ego lite's relay connection) may take before it fails. The top
/// stays under the control answer wait (120 s) so a stuck call fails with its own reason
/// before the caller gives up on the supervisor (J-272).
pub const BROWSER_CALL_SECONDS: &str = "browser_call_seconds";
const BROWSER_CALL_RANGE: RangeInclusive<u64> = 5..=90;
/// Hours an agent's information request stays answerable (08 §8.2 ③). Longer leaves an old
/// prompt open to a mistaken answer, so above the default weakens (J-273).
pub const REQUEST_TTL_HOURS: &str = "request_ttl_hours";
const REQUEST_TTL_RANGE: RangeInclusive<u64> = 1..=168;
/// Drafts of one name an agent may pile up before a person sorts them out (08 §8.3). More
/// lets an agent leave more for a person to review, so above the default weakens (J-273).
pub const MAX_DRAFTS: &str = "max_drafts";
const MAX_DRAFTS_RANGE: RangeInclusive<u64> = 1..=100;

/// The `default_item_ttl` value meaning "never expire".
pub const NO_TTL: &str = "none";

/// What values a setting takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingFormat {
    /// `true` or `false`.
    Boolean,
    /// `<N>d` or `none`.
    Ttl,
    /// A whole number within `range`, counted in `unit`.
    Range {
        range: &'static RangeInclusive<u64>,
        unit: RangeUnit,
    },
    /// Folders separated by `:`, each absolute or starting with `~/`; empty for none.
    Folders,
    /// `auto`, `chrome`, `ego-lite` or an absolute path.
    Browser,
}

/// What a [`SettingFormat::Range`] counts, as its error message names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeUnit {
    Count,
    Seconds,
    Millis,
    Hours,
}
impl RangeUnit {
    fn words(self) -> &'static str {
        match self {
            Self::Count => "a whole number",
            Self::Seconds => "whole seconds",
            Self::Millis => "whole milliseconds",
            Self::Hours => "whole hours",
        }
    }
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

/// A whole number of `unit` within `range`.
const fn number(
    key: &'static str,
    default: &'static str,
    (range, unit): (&'static RangeInclusive<u64>, RangeUnit),
    weakening: SettingWeakening,
) -> SettingSpec {
    SettingSpec {
        key,
        default,
        format: SettingFormat::Range { range, unit },
        weakening,
    }
}

/// A notice switch: on by default, and off is a preference, not a weakening.
const fn notice(key: &'static str) -> SettingSpec {
    SettingSpec {
        key,
        default: "true",
        format: SettingFormat::Boolean,
        weakening: SettingWeakening::Never,
    }
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
        default: CLEAR_SECONDS_DEFAULT_TEXT,
        format: SettingFormat::Range {
            range: &CLEAR_SECONDS_RANGE,
            unit: RangeUnit::Seconds,
        },
        weakening: SettingWeakening::AboveDefault,
    },
    SettingSpec {
        key: UPLOAD_DIRS,
        default: "",
        format: SettingFormat::Folders,
        weakening: SettingWeakening::AnyChange,
    },
    SettingSpec {
        key: UPLOAD_MAX_FILES,
        default: "10",
        format: SettingFormat::Range {
            range: &UPLOAD_MAX_FILES_RANGE,
            unit: RangeUnit::Count,
        },
        weakening: SettingWeakening::AboveDefault,
    },
    SettingSpec {
        key: BROWSER,
        default: "auto",
        format: SettingFormat::Browser,
        weakening: SettingWeakening::Never,
    },
    SettingSpec {
        key: EGO_PROBE_SECONDS,
        default: "5",
        format: SettingFormat::Range {
            range: &EGO_PROBE_RANGE,
            unit: RangeUnit::Seconds,
        },
        weakening: SettingWeakening::Never,
    },
    SettingSpec {
        key: EGO_POLL_MS,
        default: "20",
        format: SettingFormat::Range {
            range: &EGO_POLL_RANGE,
            unit: RangeUnit::Millis,
        },
        weakening: SettingWeakening::Never,
    },
    number(
        SESSION_START_SECONDS,
        "15",
        (&SESSION_START_RANGE, RangeUnit::Seconds),
        SettingWeakening::Never,
    ),
    number(
        SESSION_STOP_SECONDS,
        "15",
        (&SESSION_STOP_RANGE, RangeUnit::Seconds),
        SettingWeakening::Never,
    ),
    number(
        BROWSER_ENDPOINT_SECONDS,
        "15",
        (&BROWSER_ENDPOINT_RANGE, RangeUnit::Seconds),
        SettingWeakening::Never,
    ),
    number(
        BROWSER_CALL_SECONDS,
        "30",
        (&BROWSER_CALL_RANGE, RangeUnit::Seconds),
        SettingWeakening::Never,
    ),
    number(
        REQUEST_TTL_HOURS,
        "24",
        (&REQUEST_TTL_RANGE, RangeUnit::Hours),
        SettingWeakening::AboveDefault,
    ),
    number(
        MAX_DRAFTS,
        "20",
        (&MAX_DRAFTS_RANGE, RangeUnit::Count),
        SettingWeakening::AboveDefault,
    ),
    notice(NOTIFY_REQUESTS),
    notice(NOTIFY_HANDOFFS),
    notice(NOTIFY_DRAFTS),
    notice(NOTIFY_DENIALS),
    notice(NOTIFY_LOCKS),
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
            SettingFormat::Folders => crate::upload::parse_dirs(value, std::path::Path::new("/"))
                .map(|_| ())
                .map_err(InvalidSetting),
            SettingFormat::Browser => crate::browser_choice::BrowserChoice::parse(value)
                .map(|_| ())
                .map_err(|e| InvalidSetting(e.to_string())),
            SettingFormat::Range { range, unit } => match in_range(value, range) {
                Some(_) => Ok(()),
                None => Err(InvalidSetting(format!(
                    "must be {} from {} to {}",
                    unit.words(),
                    range.start(),
                    range.end()
                ))),
            },
        }
    }

    /// The range a [`SettingFormat::Range`] setting takes, for checks that relate a setting to
    /// a fixed wait elsewhere (`ANSWER_TIMEOUT`, `CLOSE_GRACE`).
    pub fn range(&self) -> Option<&'static RangeInclusive<u64>> {
        match self.format {
            SettingFormat::Range { range, .. } => Some(range),
            _ => None,
        }
    }

    /// The number a [`SettingFormat::Range`] setting holds: `stored`, or the default when it
    /// was never set. A stored value it does not take (only a tampered store holds one) is an
    /// error, never used.
    pub fn number(&self, stored: Option<&str>) -> Result<u64, InvalidSetting> {
        let range = self
            .range()
            .ok_or_else(|| InvalidSetting("is not a number".into()))?;
        let value = stored.unwrap_or(self.default);
        self.check(value)?;
        in_range(value, range).ok_or_else(|| InvalidSetting("is out of range".into()))
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

/// `value` as a number, if it is a whole number in `range`.
fn in_range(value: &str, range: &RangeInclusive<u64>) -> Option<u64> {
    let n = value.parse::<u64>().ok()?;
    range.contains(&n).then_some(n)
}

/// How long a copied value stays for a stored `clipboard_clear_seconds`. A value the setting
/// does not take (only a tampered store holds one) falls back to the default.
pub fn clipboard_clear_after(stored: Option<&str>) -> std::time::Duration {
    let seconds = stored.and_then(|value| in_range(value, &CLEAR_SECONDS_RANGE));
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

/// Whether a `notify_*` notice is on for its stored value (`None` when never set): on unless
/// set to `false`, like [`audit_enabled`]. Off is a preference, so an odd value means on.
pub fn notify_enabled(stored: Option<&str>) -> bool {
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

    /// The waits and limits #201 moved out of constants: default (the old constant), range and
    /// its message, and whether raising it asks a person.
    #[test]
    fn the_moved_waits_and_limits_keep_their_old_values_as_defaults() {
        let cases = [
            (
                SESSION_START_SECONDS,
                "15",
                "whole seconds from 5 to 300",
                false,
            ),
            (
                SESSION_STOP_SECONDS,
                "15",
                "whole seconds from 15 to 300",
                false,
            ),
            (
                BROWSER_ENDPOINT_SECONDS,
                "15",
                "whole seconds from 5 to 120",
                false,
            ),
            (
                BROWSER_CALL_SECONDS,
                "30",
                "whole seconds from 5 to 90",
                false,
            ),
            (REQUEST_TTL_HOURS, "24", "whole hours from 1 to 168", true),
            (MAX_DRAFTS, "20", "a whole number from 1 to 100", true),
        ];
        for (key, default, words, raising_weakens) in cases {
            let spec = SettingSpec::find(key).unwrap();
            assert_eq!(spec.default, default, "{key}");
            let range = spec.range().unwrap();
            assert!(range.contains(&default.parse().unwrap()), "{key}");
            for ok in [range.start(), range.end()] {
                assert!(spec.check(&ok.to_string()).is_ok(), "{key} {ok}");
            }
            for bad in [range.start() - 1, range.end() + 1] {
                assert_eq!(
                    spec.check(&bad.to_string()).unwrap_err().to_string(),
                    format!("must be {words}"),
                    "{key} {bad}"
                );
            }
            let above = (range.end()).to_string();
            assert_eq!(spec.weakens(&above), raising_weakens, "{key}");
            assert!(!spec.weakens(range.start().to_string().as_str()), "{key}");
        }
    }

    #[test]
    fn a_number_setting_reads_its_default_or_refuses_an_odd_stored_value() {
        let start = SettingSpec::find(SESSION_START_SECONDS).unwrap();
        assert_eq!(start.number(None), Ok(15));
        assert_eq!(start.number(Some("60")), Ok(60));
        assert!(start.number(Some("1")).is_err());
        assert!(start.number(Some("soon")).is_err());
        assert!(
            SettingSpec::find(AUDIT_ENABLED)
                .unwrap()
                .number(None)
                .is_err()
        );
    }

    #[test]
    fn notices_are_on_by_default_and_turning_them_off_does_not_weaken() {
        for key in [
            NOTIFY_REQUESTS,
            NOTIFY_HANDOFFS,
            NOTIFY_DRAFTS,
            NOTIFY_DENIALS,
            NOTIFY_LOCKS,
        ] {
            let spec = SettingSpec::find(key).unwrap();
            assert_eq!(
                (spec.default, spec.format),
                ("true", SettingFormat::Boolean)
            );
            assert!(!spec.weakens("false"), "{key}");
        }
    }

    #[test]
    fn only_false_turns_a_notice_off() {
        assert!(notify_enabled(None));
        assert!(notify_enabled(Some("true")));
        assert!(notify_enabled(Some("yes")));
        assert!(!notify_enabled(Some("false")));
    }

    #[test]
    fn only_false_turns_the_usage_record_off() {
        assert!(audit_enabled(None));
        assert!(audit_enabled(Some("true")));
        assert!(audit_enabled(Some("TRUE")));
        assert!(!audit_enabled(Some("false")));
    }
}
