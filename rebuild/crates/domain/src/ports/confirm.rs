//! A person confirming an operation that weakens a protection (R-35): turning origin binding
//! off, adding origins, letting `keyharbor run` use an item, allowing in a policy, turning the
//! usage record off. The CLI asks at the terminal; the admin app has its own dialog.

/// What the person answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Yes,
    No,
    /// Nobody can be asked (no terminal, no app).
    Unavailable,
}

pub trait Confirmation {
    /// Asks the person whether to go ahead with `action`, a one-line description without values.
    fn confirm(&self, action: &str) -> Answer;
}
