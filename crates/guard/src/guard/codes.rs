//! One-time codes the session typed, kept for a short while so responses mask them (#206).
//! The vault's masker cannot hold them: it is built once per vault version, and a code
//! changes every period. Each typed code brings the codes of the steps before and after it,
//! and all three are masked as `{{otp:item}}` until two periods after the typing (J-292).

use crate::pep::mask::Masker;
use anyhow::Result;
use keyharbor_core::OneTimeCode;
use keyharbor_domain::vault::{ItemName, Placeholder};
use zeroize::Zeroizing;

/// How many periods a typed code stays masked: the step it was typed in may end at once, and a
/// page may echo it during the next step (a login that answers slowly).
const MASKED_PERIODS: u64 = 2;

struct Recent {
    item: ItemName,
    code: Zeroizing<String>,
    until: u64,
}

#[derive(Default)]
pub(crate) struct RecentCodes {
    codes: Vec<Recent>,
}

impl RecentCodes {
    /// Masks `code` (and its neighbours) from `now` until [`MASKED_PERIODS`] periods later.
    pub(crate) fn remember(&mut self, item: &ItemName, code: OneTimeCode, now: u64) {
        self.forget_old(now);
        let until = now.saturating_add(code.period.saturating_mul(MASKED_PERIODS));
        let OneTimeCode {
            code, neighbours, ..
        } = code;
        for code in std::iter::once(code).chain(neighbours) {
            self.codes.push(Recent {
                item: item.clone(),
                code,
                until,
            });
        }
    }

    /// A masker for the codes still masked at `now`, or `None` when there are none.
    pub(crate) fn masker(&mut self, now: u64) -> Result<Option<Masker>> {
        self.forget_old(now);
        if self.codes.is_empty() {
            return Ok(None);
        }
        let values = self
            .codes
            .iter()
            .map(|r| (Placeholder::Otp(r.item.clone()), r.code.clone()));
        Masker::build(values).map(Some)
    }

    fn forget_old(&mut self, now: u64) {
        self.codes.retain(|r| r.until > now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(now: &str, before: &str, after: &str) -> OneTimeCode {
        OneTimeCode {
            code: Zeroizing::new(now.into()),
            neighbours: [Zeroizing::new(before.into()), Zeroizing::new(after.into())],
            period: 30,
        }
    }

    #[test]
    fn typed_codes_and_their_neighbours_are_masked_for_two_periods() {
        let mut recent = RecentCodes::default();
        assert!(recent.masker(100).unwrap().is_none());
        let bank = ItemName::parse("bank").unwrap();
        recent.remember(&bank, code("287082", "111111", "222222"), 100);
        let masker = recent.masker(159).unwrap().unwrap();
        assert_eq!(
            masker.redact("code 287082, then 222222 or 111111"),
            "code {{otp:bank}}, then {{otp:bank}} or {{otp:bank}}"
        );
        assert!(recent.masker(160).unwrap().is_none(), "two periods later");
    }
}
