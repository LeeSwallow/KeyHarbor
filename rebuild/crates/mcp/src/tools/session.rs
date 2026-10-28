//! Session tools: list, create, open, stop, hand to the user and wait, raw CDP endpoint.

use crate::server::{Server, fail, ok, outcome};
use keyharbor_broker::ProcessRuntime;
use keyharbor_core::session::{self as lifecycle, OpenOptions};
use keyharbor_domain::{
    RequestId, SessionName, State,
    ports::{SessionRow, SessionRuntime, SessionStore},
    vault::Description,
};
use keyharbor_guard::pdp::Action;
use keyharbor_sqlite::SqliteStore;
use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Deserialize, schemars::JsonSchema)]
struct Name {
    /// Stored locally in plain text; must not contain secrets.
    name: String,
    /// What the session is for (at most 500 characters); plain text, never a secret.
    description: Option<String>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct Id {
    id: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct OpenSession {
    id: String,
    /// HTTPS URL, or HTTP on localhost, without credentials, query or fragment.
    url: Option<String>,
    /// Also expose a raw CDP endpoint (bypasses KeyHarbor); needs the server's --allow-cdp.
    #[serde(default)]
    cdp: bool,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct WaitAuth {
    id: String,
    /// Site origin the user should sign in to (no path), e.g. https://example.com. Not verified.
    origin: Option<String>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct WaitForResume {
    id: String,
    /// Request ID returned by session_wait_auth.
    request_id: String,
    /// Maximum wait in seconds, 1 to 300; default 120.
    timeout_seconds: Option<u64>,
}
#[derive(Deserialize, schemars::JsonSchema)]
pub(crate) struct Session {
    /// Session ID.
    pub session: String,
}

fn row_json(runtime: &ProcessRuntime, row: &SessionRow) -> Value {
    json!({
        "id": row.id.to_string(),
        "name": row.name.as_str(),
        "state": lifecycle::display_state(runtime, row),
        "cdp": row.cdp,
        "description": row.description.as_ref().map(|d| d.as_str()),
    })
}

#[tool_router(router = session_tools, vis = "pub(crate)")]
impl Server {
    #[tool(
        description = "List KeyHarbor browser sessions with their state. recovery_required means the supervisor is gone; the user must run recover. States do not prove that a site is signed in."
    )]
    async fn session_list(&self) -> CallToolResult {
        outcome(
            self.blocking(|inner| {
                let rows = SqliteStore::open(&inner.data)?.list()?;
                Ok(Value::Array(
                    rows.iter()
                        .filter(|row| inner.scope.is_none_or(|scope| scope == row.id))
                        .map(|row| row_json(&inner.runtime, row))
                        .collect(),
                ))
            })
            .await,
        )
    }

    #[tool(description = "Create a stopped browser session with its own browser profile.")]
    async fn session_create(
        &self,
        Parameters(Name { name, description }): Parameters<Name>,
    ) -> CallToolResult {
        if self.inner.scope.is_some() {
            return fail("session creation is disabled for this scoped server");
        }
        outcome(
            self.blocking(move |inner| {
                inner.allow(&Action::CreateSession)?;
                let name = SessionName::parse(&name)?;
                let description = description
                    .as_deref()
                    .map(Description::parse)
                    .transpose()
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                let store = SqliteStore::open(&inner.data)?;
                let session = store.create(name)?;
                store.set_description(session.id, description.as_ref())?;
                Ok(json!({"id": session.id.to_string(), "name": session.name.as_str(), "state": session.state.as_str()}))
            })
            .await,
        )
    }

    #[tool(
        description = "Open a session's browser, optionally at a URL. Do not put secrets in URLs. Use the browser_* tools to operate it."
    )]
    async fn session_open(
        &self,
        Parameters(OpenSession { id, url, cdp }): Parameters<OpenSession>,
    ) -> CallToolResult {
        if cdp && !self.inner.allow_cdp {
            return fail(
                "raw CDP is disabled; the user must start the MCP server with --allow-cdp",
            );
        }
        let id = match self.session(&id) {
            Ok(id) => id,
            Err(error) => return fail(error.to_string()),
        };
        outcome(
            self.blocking(move |inner| {
                inner.allow(&Action::OpenSession(id))?;
                if cdp {
                    inner.allow(&Action::RawCdp)?;
                }
                let options = OpenOptions {
                    url: url.as_deref(),
                    cdp,
                    browser: &inner.browser,
                    agent: inner.agent(),
                };
                lifecycle::open(
                    &SqliteStore::open(&inner.data)?,
                    &inner.runtime,
                    id,
                    &options,
                )?;
                Ok(json!({"id": id.to_string(), "state": "running"}))
            })
            .await,
        )
    }

    #[tool(
        description = "Close the session's browser and stop the session. This does not sign out of websites."
    )]
    async fn session_stop(&self, Parameters(Id { id }): Parameters<Id>) -> CallToolResult {
        let id = match self.session(&id) {
            Ok(id) => id,
            Err(error) => return fail(error.to_string()),
        };
        outcome(
            self.blocking(move |inner| {
                let state = lifecycle::stop(
                    &SqliteStore::open(&inner.data)?,
                    &inner.runtime,
                    id,
                    inner.agent(),
                )?;
                Ok(json!({"id": id.to_string(), "state": state.as_str()}))
            })
            .await,
        )
    }

    #[tool(
        description = "Ask the user to sign in or solve a challenge in the browser window themselves. Returns a request_id; then call session_wait_for_resume. The user resumes with `keyharbor resume <id> <request_id>`."
    )]
    async fn session_wait_auth(
        &self,
        Parameters(WaitAuth { id, origin }): Parameters<WaitAuth>,
    ) -> CallToolResult {
        let id = match self.session(&id) {
            Ok(id) => id,
            Err(error) => return fail(error.to_string()),
        };
        outcome(
            self.blocking(move |inner| {
                let request = lifecycle::wait_auth(&SqliteStore::open(&inner.data)?, &inner.runtime, id, origin.as_deref(), inner.agent())?;
                Ok(json!({"id": id.to_string(), "state": "awaiting_user", "request_id": request.to_string()}))
            })
            .await,
        )
    }

    #[tool(
        description = "Wait until the user resumes the given authentication request, up to timeout_seconds (1–300, default 120). Success means the user said they are done; sign-in success is not verified."
    )]
    async fn session_wait_for_resume(
        &self,
        Parameters(WaitForResume {
            id,
            request_id,
            timeout_seconds,
        }): Parameters<WaitForResume>,
    ) -> CallToolResult {
        let id = match self.session(&id) {
            Ok(id) => id,
            Err(error) => return fail(error.to_string()),
        };
        let Ok(request) = RequestId::parse(&request_id) else {
            return fail("invalid request_id");
        };
        let timeout = timeout_seconds.unwrap_or(120);
        if !(1..=300).contains(&timeout) {
            return fail("timeout_seconds must be between 1 and 300");
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout);
        loop {
            let snapshot = self
                .blocking(move |inner| {
                    let store = SqliteStore::open(&inner.data)?;
                    let status = store.handoff_status(id, request)?;
                    let row = store.get(id)?;
                    Ok((status, row, inner.runtime.is_alive(id)))
                })
                .await;
            let (status, row, alive) = match snapshot {
                Ok(snapshot) => snapshot,
                Err(error) => return fail(format!("{error:#}")),
            };
            use keyharbor_domain::HandoffStatus::*;
            match status {
                None => return fail("authentication request not found"),
                Some(Superseded) => return fail("request was replaced by a newer one"),
                Some(Cancelled) => {
                    return fail("request was cancelled because the session stopped");
                }
                Some(Resumed) => {
                    let state = row.map(|r| r.state);
                    return if state == Some(State::Running) {
                        ok(
                            json!({"id": id.to_string(), "state": "running", "note": "the user resumed; sign-in success is not verified"}),
                        )
                    } else {
                        fail(format!(
                            "request was resumed but the session is now {}",
                            state.map_or("gone", State::as_str)
                        ))
                    };
                }
                Some(Pending) if !alive => {
                    return fail("session supervisor is not running; the user must run recover");
                }
                Some(Pending) => (),
            }
            if tokio::time::Instant::now() >= deadline {
                return fail(
                    "Timed out waiting for resume; the session may still be awaiting the user",
                );
            }
            tokio::time::sleep_until(
                (tokio::time::Instant::now() + Duration::from_millis(250)).min(deadline),
            )
            .await;
        }
    }

    #[tool(
        description = "Raw CDP WebSocket endpoint of a session opened with cdp: true. Anyone holding it has full control of the browser and bypasses KeyHarbor's protections."
    )]
    async fn session_endpoint(&self, Parameters(Id { id }): Parameters<Id>) -> CallToolResult {
        if !self.inner.allow_cdp {
            return fail(
                "raw CDP is disabled; the user must start the MCP server with --allow-cdp",
            );
        }
        let id = match self.session(&id) {
            Ok(id) => id,
            Err(error) => return fail(error.to_string()),
        };
        outcome(
            self.blocking(move |inner| {
                inner.allow(&Action::OpenSession(id))?;
                inner.allow(&Action::RawCdp)?;
                Ok(json!({"endpoint": lifecycle::endpoint(&SqliteStore::open(&inner.data)?, &inner.runtime, id)?, "warning": "full browser control; KeyHarbor protections do not apply"}))
            })
            .await,
        )
    }
}
