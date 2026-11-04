//! Letting a locked session substitute again (#190, J-220). The guard's lock (03 §3.8) is
//! right to fire, but it also fires on honest pages; only a person lifts it, so it is a
//! weakening ([`WeakeningOp::SessionUnlock`]) and nothing here is offered to agents.

use super::require_supervisor;
use crate::weaken::{self, WeakeningOp};
use anyhow::{Result, bail};
use keyharbor_domain::{
    SessionId,
    ports::{Confirm, SessionRuntime},
    session::substitution_lock::LockReason,
};

/// The refusal when there is nothing to lift.
const NOT_LOCKED: &str = "session is not locked; it still substitutes";

/// Lifts the session's substitution lock once `confirm` says yes, and returns why it had
/// locked. The supervisor records the lifting in the usage record. Asks nothing when the
/// session is not running or not locked.
pub fn unlock(
    runtime: &dyn SessionRuntime,
    confirm: &dyn Confirm,
    id: SessionId,
) -> Result<LockReason> {
    require_supervisor(runtime, id)?;
    let Some(reason) = runtime.lock_reason(id)? else {
        bail!(NOT_LOCKED);
    };
    weaken::require(
        confirm,
        &[WeakeningOp::SessionUnlock {
            session: id.to_string(),
            reason,
        }],
    )?;
    runtime.unlock(id)?;
    Ok(reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyharbor_domain::{
        Generation,
        browser_choice::BrowserChoice,
        ports::{
            Answer,
            runtime::{LaunchOptions, ResolvedBrowser, RuntimeLock, StartedSupervisor},
        },
    };
    use std::{
        cell::RefCell,
        path::Path,
        sync::{Mutex, atomic::AtomicBool, atomic::Ordering},
        time::Duration,
    };

    const ID: &str = "11111111-1111-4111-8111-111111111111";

    /// A running supervisor whose guard holds `locked`.
    struct Locked {
        alive: bool,
        locked: Mutex<Option<LockReason>>,
        unlocked: AtomicBool,
    }
    fn running(locked: Option<LockReason>) -> Locked {
        Locked {
            alive: true,
            locked: Mutex::new(locked),
            unlocked: AtomicBool::new(false),
        }
    }

    impl SessionRuntime for Locked {
        fn is_alive(&self, _: SessionId) -> bool {
            self.alive
        }
        fn lock_reason(&self, _: SessionId) -> Result<Option<LockReason>> {
            Ok(*self.locked.lock().unwrap())
        }
        fn unlock(&self, _: SessionId) -> Result<()> {
            *self.locked.lock().unwrap() = None;
            self.unlocked.store(true, Ordering::SeqCst);
            Ok(())
        }
        fn has_browser(&self, _: &Path) -> bool {
            unreachable!()
        }
        fn resolve_browser(&self, _: &BrowserChoice, _: Duration) -> Result<ResolvedBrowser> {
            unreachable!()
        }
        fn preflight(&self, _: SessionId) -> Result<()> {
            unreachable!()
        }
        fn lock(&self, _: SessionId) -> Result<RuntimeLock> {
            unreachable!()
        }
        fn start(
            &self,
            _: SessionId,
            _: Generation,
            _: &LaunchOptions<'_>,
        ) -> Result<Box<dyn StartedSupervisor>> {
            unreachable!()
        }
        fn close(&self, _: SessionId) -> Result<()> {
            unreachable!()
        }
        fn remove_socket(&self, _: SessionId) {
            unreachable!()
        }
        fn remove_profile(&self, _: SessionId) -> Result<()> {
            unreachable!()
        }
        fn debug_endpoint(&self, _: SessionId) -> Result<String> {
            unreachable!()
        }
    }

    /// Answers `answer` and remembers what it was asked.
    struct Person {
        answer: Answer,
        asked: RefCell<Vec<String>>,
    }
    fn person(answer: Answer) -> Person {
        Person {
            answer,
            asked: RefCell::default(),
        }
    }
    impl Confirm for Person {
        fn confirm(&self, action: &str) -> Answer {
            self.asked.borrow_mut().push(action.to_owned());
            self.answer
        }
    }

    fn id() -> SessionId {
        SessionId::parse(ID).unwrap()
    }

    #[test]
    fn a_person_who_confirms_lifts_the_lock() {
        let runtime = running(Some(LockReason::MaskProbe));
        let yes = person(Answer::Yes);
        assert_eq!(unlock(&runtime, &yes, id()).unwrap(), LockReason::MaskProbe);
        assert!(runtime.unlocked.load(Ordering::SeqCst));
        assert_eq!(
            *yes.asked.borrow(),
            [format!(
                "letting session {ID} substitute again after it locked (mask_probe)"
            )]
        );
    }

    #[test]
    fn without_a_person_the_lock_stays() {
        for answer in [Answer::No, Answer::Unavailable] {
            let runtime = running(Some(LockReason::FocusMoved));
            let err = unlock(&runtime, &person(answer), id()).unwrap_err();
            assert!(!runtime.unlocked.load(Ordering::SeqCst), "{err}");
            assert_eq!(
                runtime.lock_reason(id()).unwrap(),
                Some(LockReason::FocusMoved)
            );
        }
        let runtime = running(Some(LockReason::FocusMoved));
        let err = unlock(&runtime, &person(Answer::Unavailable), id()).unwrap_err();
        assert!(
            err.to_string()
                .ends_with("needs confirmation from a person at a terminal (or the admin app)"),
            "{err}"
        );
    }

    #[test]
    fn nothing_is_asked_when_there_is_nothing_to_lift() {
        let yes = person(Answer::Yes);
        let err = unlock(&running(None), &yes, id()).unwrap_err();
        assert_eq!(err.to_string(), NOT_LOCKED);
        let stopped = Locked {
            alive: false,
            ..running(Some(LockReason::MaskProbe))
        };
        let err = unlock(&stopped, &yes, id()).unwrap_err();
        assert!(err.to_string().contains("not running"), "{err}");
        assert!(yes.asked.borrow().is_empty());
    }
}
