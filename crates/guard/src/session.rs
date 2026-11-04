//! Which policy actions a session operation needs (02 §2.8, J-71). Interfaces name the
//! operation; this table, not the interface, decides what is checked.

use crate::pdp::{Action, Decision, decide};
use anyhow::{Result, anyhow, bail};
use keyharbor_domain::{
    SessionId,
    browser_choice::{BrowserChoice, BrowserKind},
    ports::PolicyStore,
};

/// An agent's operation on sessions. Listing is not one: it shows names, states and
/// descriptions only, and a scoped server already limits it to its session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionOp {
    Create,
    Open {
        id: SessionId,
        cdp: bool,
        browser: BrowserKind,
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
            Self::Open { id, cdp, browser } => {
                let mut actions = vec![Action::OpenSession(id), Action::UseBrowser(browser)];
                if cdp {
                    actions.push(Action::RawCdp);
                }
                actions
            }
            Self::Endpoint(id) => vec![Action::OpenSession(id), Action::RawCdp],
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

/// The browser an agent asks [`SessionOp::Open`] for. Only the person may name an executable
/// (a path, in the `browser` setting or `--browser`): an agent naming one would choose which
/// program KeyHarbor starts as its browser.
pub fn parse_agent_browser(value: &str) -> Result<BrowserChoice> {
    match BrowserChoice::parse(value).map_err(|e| anyhow!("{e}"))? {
        BrowserChoice::Path(_) => bail!("only the person may name a browser executable"),
        choice => Ok(choice),
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
    use keyharbor_domain::policy::{Effect, Resource, Rule};

    #[test]
    fn every_operation_on_one_session_needs_that_session() {
        let id = keyharbor_crypto::random::id::<SessionId>();
        let chrome = BrowserKind::Chrome;
        assert_eq!(
            SessionOp::Open {
                id,
                cdp: false,
                browser: chrome
            }
            .actions(),
            vec![Action::OpenSession(id), Action::UseBrowser(chrome)]
        );
        assert_eq!(
            SessionOp::Open {
                id,
                cdp: true,
                browser: chrome
            }
            .actions(),
            vec![
                Action::OpenSession(id),
                Action::UseBrowser(chrome),
                Action::RawCdp
            ]
        );
        for op in [
            SessionOp::Browser(id),
            SessionOp::Stop(id),
            SessionOp::WaitAuth(id),
            SessionOp::WaitForResume(id),
            SessionOp::WaitSite(id),
            SessionOp::SiteState(id),
        ] {
            assert_eq!(op.actions(), vec![Action::OpenSession(id)], "{op:?}");
        }
        assert_eq!(
            SessionOp::Endpoint(id).actions(),
            vec![Action::OpenSession(id), Action::RawCdp]
        );
        assert_eq!(SessionOp::Create.actions(), vec![Action::CreateSession]);
    }

    #[test]
    fn an_agent_names_a_browser_kind_but_never_an_executable() {
        assert_eq!(
            parse_agent_browser("chrome").unwrap(),
            BrowserChoice::Chrome
        );
        assert_eq!(parse_agent_browser("auto").unwrap(), BrowserChoice::Auto);
        assert_eq!(
            parse_agent_browser("/Applications/Evil.app/evil")
                .unwrap_err()
                .to_string(),
            "only the person may name a browser executable"
        );
        assert!(parse_agent_browser("firefox").is_err());
    }

    #[test]
    fn policy_can_deny_ego_lite() {
        let rules = [Rule::new("*", Resource::Browser, "ego-lite", Effect::Deny).unwrap()];
        let ego = Action::UseBrowser(BrowserKind::EgoLite);
        assert!(!crate::pdp::decide(&rules, "claude", &ego).allowed());
        let chrome = Action::UseBrowser(BrowserKind::Chrome);
        assert!(crate::pdp::decide(&rules, "claude", &chrome).allowed());
        let all = [Rule::new("codex", Resource::Browser, "*", Effect::Deny).unwrap()];
        assert!(!crate::pdp::decide(&all, "codex", &chrome).allowed());
        assert!(Rule::new("*", Resource::Browser, "firefox", Effect::Deny).is_err());
    }
}
