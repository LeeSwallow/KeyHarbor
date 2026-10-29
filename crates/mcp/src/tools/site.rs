//! Site tools: a session's sign-in state at an origin (02 §2.7). The state is the agent's
//! own note; KeyHarbor does not visit the site to check it.

use crate::server::{Server, fail, outcome};
use keyharbor_core::site::{self, SiteChange};
use keyharbor_domain::{
    site::SiteState,
    vault::{ItemName, Origin},
};
use keyharbor_guard::session::SessionOp;
use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router,
};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize, schemars::JsonSchema)]
struct GetSite {
    /// Session ID.
    session: String,
    /// Site origin without a path, e.g. https://github.com.
    origin: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct SetSite {
    /// Session ID.
    session: String,
    /// Site origin without a path, e.g. https://github.com.
    origin: String,
    /// logged_in, expired or unknown.
    state: String,
    /// Vault item name used to sign in, if any. A name, never a value.
    item: Option<String>,
}

#[tool_router(router = site_tools, vis = "pub(crate)")]
impl Server {
    #[tool(
        description = "The session's recorded sign-in state at an origin: logged_in, expired or unknown (nothing recorded). It is what an agent last recorded, not a fresh check; verify in the page before relying on it."
    )]
    async fn site_state_get(
        &self,
        Parameters(GetSite { session, origin }): Parameters<GetSite>,
    ) -> CallToolResult {
        let id = match self.session(&session) {
            Ok(id) => id,
            Err(error) => return fail(error.to_string()),
        };
        let origin = match Origin::parse(&origin) {
            Ok(origin) => origin,
            Err(error) => return fail(error.to_string()),
        };
        outcome(
            self.blocking(move |inner| {
                inner.allow(SessionOp::SiteState(id))?;
                let state = site::get(&**inner.store()?, id, &origin)?;
                Ok(json!({"session": id.to_string(), "origin": origin.as_str(), "state": state.as_str()}))
            })
            .await,
        )
    }

    #[tool(
        description = "Record the session's sign-in state at an origin after you checked the page, e.g. logged_in after signing in or expired when a site asks to sign in again."
    )]
    async fn site_state_set(
        &self,
        Parameters(SetSite {
            session,
            origin,
            state,
            item,
        }): Parameters<SetSite>,
    ) -> CallToolResult {
        let id = match self.session(&session) {
            Ok(id) => id,
            Err(error) => return fail(error.to_string()),
        };
        let change = match (|| {
            Ok::<_, keyharbor_domain::vault::InvalidValue>(SiteChange {
                session: id,
                origin: Origin::parse(&origin)?,
                state: SiteState::parse(&state)?,
                item: item.as_deref().map(ItemName::parse).transpose()?,
            })
        })() {
            Ok(change) => change,
            Err(error) => return fail(error.to_string()),
        };
        outcome(
            self.blocking(move |inner| {
                inner.allow(SessionOp::SiteState(id))?;
                let record = site::set(&**inner.store()?, &*inner.clock, inner.agent(), change)?;
                Ok(json!({"session": id.to_string(), "origin": record.origin.as_str(), "state": record.state.as_str()}))
            })
            .await,
        )
    }
}
