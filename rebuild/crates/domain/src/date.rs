//! Calendar dates (UTC) for item expiry (R-37). Pure arithmetic; the current time comes from
//! the caller.

use crate::vault::InvalidValue;
use std::fmt;

/// A day in the proleptic Gregorian calendar, as days since 1970-01-01.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(i64);

/// Days from 1970-01-01 to `y-m-d` (Howard Hinnant's days_from_civil).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `(year, month, day)` of a day count (civil_from_days).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

impl Date {
    /// `YYYY-MM-DD`, a real calendar day between years 1970 and 9999.
    pub fn parse(value: &str) -> Result<Self, InvalidValue> {
        const BAD: InvalidValue = InvalidValue("date must be YYYY-MM-DD");
        let parts: Vec<&str> = value.split('-').collect();
        let [y, m, d] = parts.as_slice() else {
            return Err(BAD);
        };
        if y.len() != 4 || m.len() != 2 || d.len() != 2 {
            return Err(BAD);
        }
        let number = |s: &str| s.parse::<i64>().map_err(|_| BAD);
        let (y, m, d) = (number(y)?, number(m)?, number(d)?);
        let date = Self(days_from_civil(y, m, d));
        if !(1970..=9999).contains(&y) || date.parts() != (y, m, d) {
            return Err(BAD);
        }
        Ok(date)
    }

    /// The UTC day of a Unix time.
    pub fn from_unix_seconds(seconds: u64) -> Self {
        Self(i64::try_from(seconds / 86_400).unwrap_or(i64::MAX / 2))
    }

    pub fn plus_days(self, days: u32) -> Self {
        Self(self.0 + i64::from(days))
    }

    fn parts(self) -> (i64, i64, i64) {
        civil_from_days(self.0)
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (y, m, d) = self.parts();
        write!(f, "{y:04}-{m:02}-{d:02}")
    }
}

/// The last day agents may use an item (R-37). After it the item is kept but refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Expiry(Date);

impl Expiry {
    pub fn parse(value: &str) -> Result<Self, InvalidValue> {
        Date::parse(value).map(Self)
    }
    pub fn on(date: Date) -> Self {
        Self(date)
    }
    pub fn date(self) -> Date {
        self.0
    }
    /// Expired once `today` is past the last day.
    pub fn is_expired(self, today: Date) -> bool {
        today > self.0
    }
}

impl fmt::Display for Expiry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// How long new items last by default (`default_item_ttl`, e.g. `90d`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ttl(u32);

impl Ttl {
    /// `<N>d`, 1 to 36 500 days.
    pub fn parse(value: &str) -> Result<Self, InvalidValue> {
        value
            .strip_suffix('d')
            .and_then(|n| n.parse::<u32>().ok())
            .filter(|n| (1..=36_500).contains(n))
            .map(Self)
            .ok_or(InvalidValue("a duration is <N>d with N from 1 to 36500"))
    }
    /// The expiry of something created on `today`.
    pub fn from(self, today: Date) -> Expiry {
        Expiry(today.plus_days(self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_parse_print_and_count_days() {
        for value in ["1970-01-01", "2024-02-29", "2027-01-31", "9999-12-31"] {
            assert_eq!(Date::parse(value).unwrap().to_string(), value);
        }
        for bad in [
            "2027-02-29",
            "2027-13-01",
            "2027-1-01",
            "27-01-01",
            "1969-12-31",
            "x",
            "",
        ] {
            assert!(Date::parse(bad).is_err(), "{bad}");
        }
        let day = Date::from_unix_seconds(1_793_203_129);
        assert_eq!(day.to_string(), "2026-10-28");
        assert_eq!(day.plus_days(4).to_string(), "2026-11-01");
    }

    #[test]
    fn expiry_includes_its_last_day() {
        let last = Expiry::parse("2027-01-31").unwrap();
        assert!(!last.is_expired(Date::parse("2027-01-31").unwrap()));
        assert!(last.is_expired(Date::parse("2027-02-01").unwrap()));
        let ttl = Ttl::parse("90d").unwrap();
        assert_eq!(
            ttl.from(Date::parse("2026-10-29").unwrap()).to_string(),
            "2027-01-27"
        );
        for bad in ["90", "0d", "d", "-1d", "36501d", "1w"] {
            assert!(Ttl::parse(bad).is_err(), "{bad}");
        }
    }
}
