//! The `Confirm` port as the Mac asks it: Touch ID, or the login password when there is no
//! fingerprint (LocalAuthentication `deviceOwnerAuthentication`). The person at the Mac sees
//! the action in the system prompt. Used for each autofill into their own browser (ADR-0017).
//! Nothing is kept: every confirmation is its own prompt.

use keyharbor_domain::ports::{Answer, Confirm};

/// The test answer named by [`TEST_PRESENCE_ENV`] (`yes` or `no`), honoured only when built
/// with `insecure-test-keystore`; anything else means "ask the Mac".
pub const TEST_PRESENCE_ENV: &str = "KEYHARBOR_TEST_PRESENCE";

pub fn test_presence(value: Option<std::ffi::OsString>) -> Option<Answer> {
    if !cfg!(feature = "insecure-test-keystore") {
        return None;
    }
    match value?.to_str()? {
        "yes" => Some(Answer::Yes),
        "no" => Some(Answer::No),
        _ => None,
    }
}

/// A fixed answer, for tests (see [`test_presence`]).
pub struct FixedConfirm(pub Answer);
impl Confirm for FixedConfirm {
    fn confirm(&self, _action: &str) -> Answer {
        self.0
    }
}

pub struct LocalAuthConfirm;

#[cfg(target_os = "macos")]
impl Confirm for LocalAuthConfirm {
    fn confirm(&self, action: &str) -> Answer {
        use block2::RcBlock;
        use objc2::runtime::Bool;
        use objc2_foundation::{NSError, NSString};
        use objc2_local_authentication::{LAContext, LAPolicy};
        use std::{sync::mpsc, time::Duration};

        /// Long enough to find the finger or type the password; the extension waits as long.
        const PROMPT_WAIT: Duration = Duration::from_secs(60);
        let policy = LAPolicy::DeviceOwnerAuthentication;
        // SAFETY: LAContext is a plain Objective-C object; `new` returns a retained instance.
        let context = unsafe { LAContext::new() };
        // SAFETY: a valid policy constant on a live context.
        if unsafe { context.canEvaluatePolicy_error(policy) }.is_err() {
            return Answer::Unavailable;
        }
        let (tx, rx) = mpsc::channel();
        let reply = RcBlock::new(move |ok: Bool, _error: *mut NSError| {
            let _ = tx.send(ok.as_bool());
        });
        let reason = NSString::from_str(&format!("KeyHarbor: {action}"));
        // SAFETY: the context, reason and block stay alive until the reply (block retained by
        // the framework); the reply only sends on a channel.
        unsafe { context.evaluatePolicy_localizedReason_reply(policy, &reason, &reply) };
        match rx.recv_timeout(PROMPT_WAIT) {
            Ok(true) => Answer::Yes,
            Ok(false) => Answer::No,
            Err(_) => Answer::Unavailable,
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl Confirm for LocalAuthConfirm {
    fn confirm(&self, _action: &str) -> Answer {
        Answer::Unavailable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_answer_is_only_honoured_in_test_builds() {
        let answer = test_presence(Some("yes".into()));
        if cfg!(feature = "insecure-test-keystore") {
            assert_eq!(answer, Some(Answer::Yes));
            assert_eq!(test_presence(Some("no".into())), Some(Answer::No));
        } else {
            assert_eq!(answer, None);
        }
        assert_eq!(test_presence(Some("maybe".into())), None);
        assert_eq!(test_presence(None), None);
    }
}
