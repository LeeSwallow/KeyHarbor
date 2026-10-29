//! The app's answer to "a person confirms this" (R-35), the counterpart of the CLI's
//! `TtyConfirm`. The window first sends a change unconfirmed; if it weakens a protection, the
//! command answers with the question instead of doing it ([`Outcome::Ask`]). The window shows
//! it in a dialog, and only a click on its confirm button resends the change confirmed.

use keyharbor_domain::ports::{Answer, Confirm};
use serde::Serialize;
use std::sync::Mutex;

/// Confirms only when the window says the person confirmed, and remembers what it was asked.
pub struct DialogConfirm {
    confirmed: bool,
    asked: Mutex<Option<String>>,
}

impl DialogConfirm {
    pub fn new(confirmed: bool) -> Self {
        Self {
            confirmed,
            asked: Mutex::new(None),
        }
    }
    /// What the change needed confirming for, if it was asked and not confirmed.
    pub fn question(&self) -> Option<String> {
        self.asked.lock().ok().and_then(|q| q.clone())
    }
}

impl Confirm for DialogConfirm {
    fn confirm(&self, action: &str) -> Answer {
        if self.confirmed {
            return Answer::Yes;
        }
        if let Ok(mut asked) = self.asked.lock() {
            *asked = Some(action.to_owned());
        }
        Answer::No
    }
}

/// A change that may need a person: done, or the question to put to them.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    Done,
    Ask { question: String },
}

/// Runs `change` with a [`DialogConfirm`]; a refusal for want of confirmation becomes
/// [`Outcome::Ask`], any other error stays an error.
pub fn confirmed_by_dialog(
    confirmed: bool,
    change: impl FnOnce(&DialogConfirm) -> anyhow::Result<()>,
) -> anyhow::Result<Outcome> {
    let confirm = DialogConfirm::new(confirmed);
    match change(&confirm) {
        Ok(()) => Ok(Outcome::Done),
        Err(error) => match confirm.question() {
            Some(question) => Ok(Outcome::Ask { question }),
            None => Err(error),
        },
    }
}
