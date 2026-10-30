//! Which policy actions a session operation needs (02 §2.8, J-71). Interfaces name the
//! operation; this table, not the interface, decides what is checked.

use crate::pdp::{Action, Decision, decide};
use anyhow::{Result, bail};
use keyharbor_domain::{SessionId, ports::PolicyStore};

/// An agent's operation on sessions. Listing is not one: it shows names, states and
/// descriptions only, and a scoped server already limits it to its session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionOp {
    Create,
    Open {
        id: SessionId,
        cdp: bool,
    },
    /// Every browser tool.
    Browser(SessionId),
    /// Checked although stopping only closes: an agent kept out of a session must not be
    /// able to end another agent's work in it.
    Stop(SessionId),
    /// Checked: asking the user to sign in is acting in the session, and a denied agent must
    /// not put a sign-in prompt in front of the user.
    WaitAuth(SessionId),
    WaitForResume(SessionId),
    /// Waiting for the person to hand a site over: it tells the agent what the session can now
    /// reach, so it needs the session.
    WaitSite(SessionId),
    Endpoint(SessionId),
    /// Reading or recording a site's sign-in state: it tells an agent where the session is
    /// signed in, so it needs the session.
    SiteState(SessionId),
}

impl SessionOp {
    /// The actions that must all be allowed.
    pub fn actions(self) -> Vec<Action> {
        match self {
            Self::Create => vec![Action::CreateSession],
            Self::Open { id, cdp: false } => vec![Action::OpenSession(id)],
            Self::Open { id, cdp: true } | Self::Endpoint(id) => {
                vec![Action::OpenSession(id), Action::RawCdp]
            }
            Self::Browser(id)
            | Self::Stop(id)
            | Self::WaitAuth(id)
            | Self::WaitForResume(id)
            | Self::WaitSite(id)
            | Self::SiteState(id) => {
                vec![Action::OpenSession(id)]
            }
        }
    }
}

/// Fails, naming the deciding rule, unless policy allows `agent` every action `op` needs.
pub fn authorize(store: &dyn PolicyStore, agent: &str, op: SessionOp) -> Result<()> {
    let rules = store.policy_rules()?;
    for action in op.actions() {
        if let Decision::Deny { rule } = decide(&rules, agent, &action) {
            bail!("denied by policy ({rule})");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_operation_on_one_session_needs_that_session() {
        let id = keyharbor_crypto::random::id::<SessionId>();
        for op in [
            SessionOp::Open { id, cdp: false },
            SessionOp::Browser(id),
            SessionOp::Stop(id),
            SessionOp::WaitAuth(id),
            SessionOp::WaitForResume(id),
            SessionOp::WaitSite(id),
            SessionOp::SiteState(id),
        ] {
            assert_eq!(op.actions(), vec![Action::OpenSession(id)], "{op:?}");
        }
        for op in [SessionOp::Open { id, cdp: true }, SessionOp::Endpoint(id)] {
            assert_eq!(
                op.actions(),
                vec![Action::OpenSession(id), Action::RawCdp],
                "{op:?}"
            );
        }
        assert_eq!(SessionOp::Create.actions(), vec![Action::CreateSession]);
    }
}
