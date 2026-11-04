//! The per-second cap on recorded refusals: a flood of refused requests must not flood the
//! usage record, yet every refusal is still counted.

use super::Guard;
use keyharbor_domain::audit::{AuditEvent, AuditKind};

/// Refusals a session records per second; the rest become one counter event. Fixed: it only
/// bounds the record's growth, every refusal is still counted.
pub const MAX_DENIALS_PER_SECOND: u32 = 10;

/// Refusals recorded in the current second, and those left out.
#[derive(Default)]
pub(super) struct Denials {
    second: u64,
    recorded: u32,
    suppressed: u32,
}

impl Guard {
    /// Records a refusal unless this second's cap is reached; a new second first records how
    /// many were left out of the last one.
    pub(super) fn record_denial(&self, agent: &str, event: &AuditEvent) {
        let now = self.clock.unix_seconds();
        let (flush, record) = {
            let Ok(mut d) = self.denials.lock() else {
                return;
            };
            let flush = if d.second != now {
                let left_out = std::mem::take(&mut d.suppressed);
                d.second = now;
                d.recorded = 0;
                left_out
            } else {
                0
            };
            let record = d.recorded < MAX_DENIALS_PER_SECOND;
            if record {
                d.recorded += 1;
            } else {
                d.suppressed += 1;
            }
            (flush, record)
        };
        if flush > 0 {
            self.record_suppressed(agent, flush);
        }
        if record {
            self.record(event);
        }
    }

    fn record_suppressed(&self, agent: &str, count: u32) {
        self.record(
            &AuditEvent::new(agent, AuditKind::DeniedSuppressed, &count.to_string())
                .session(self.session),
        );
    }
}

impl Drop for Guard {
    /// The supervisor is ending: the refusals left out of the last second are counted.
    fn drop(&mut self) {
        let left_out = self
            .denials
            .get_mut()
            .map(|d| std::mem::take(&mut d.suppressed))
            .unwrap_or(0);
        if left_out > 0 {
            self.record_suppressed(keyharbor_domain::policy::LOCAL_AGENT, left_out);
        }
    }
}
