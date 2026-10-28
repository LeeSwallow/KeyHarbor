use std::{path::PathBuf, process::Stdio};

use rmcp::{
    ServiceExt,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    schemars, tool, tool_router,
    transport::stdio,
};
use serde::Deserialize;

#[derive(Clone)]
struct Server {
    root: PathBuf,
    browser: Option<PathBuf>,
    allow_cdp: bool,
    session: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct Name {
    name: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct Id {
    id: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct WaitAuth {
    id: String,
    /// Claimed site origin, without a path, query, fragment or credentials; not verified against browser tabs.
    origin: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct OpenSession {
    id: String,
    /// HTTPS URL or local HTTP URL without credentials, query, or fragment.
    url: Option<String>,
    /// Enable full browser control; requires the server's --allow-cdp option.
    #[serde(default)]
    cdp: bool,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct WaitForResume {
    id: String,
    /// Request ID returned by session_wait_auth.
    request_id: String,
    /// Maximum wait in seconds, between 1 and 300; defaults to 120.
    timeout_seconds: Option<u64>,
}

impl Server {
    async fn execute(
        &self,
        command: &str,
        argument: Option<&str>,
        url: Option<&str>,
        cdp: bool,
    ) -> CallToolResult {
        let (command, argument) = if let Some(scope) = self.session.as_deref() {
            match command {
                "list" => ("show", Some(scope)),
                "create" => {
                    return CallToolResult::error(vec![ContentBlock::text(
                        "Session creation is disabled for this scoped server",
                    )]);
                }
                _ if argument != Some(scope) => {
                    return CallToolResult::error(vec![ContentBlock::text(
                        "Session is outside this server's scope",
                    )]);
                }
                _ => (command, argument),
            }
        } else {
            (command, argument)
        };
        let result = async {
            let executable = std::env::current_exe()?;
            let mut child = tokio::process::Command::new(executable);
            child
                .arg("--data-dir")
                .arg(&self.root)
                .arg(command)
                .stdin(Stdio::null());
            if command == "open"
                && let Some(browser) = &self.browser
            {
                child.arg("--browser").arg(browser);
            }
            if command == "open" && cdp {
                child.arg("--cdp");
            }
            if command == "open"
                && let Some(url) = url
            {
                child.arg("--url").arg(url);
            }
            if command == "wait-auth"
                && let Some(origin) = url
            {
                child.arg("--origin").arg(origin);
            }
            if let Some(value) = argument {
                child.arg("--").arg(value);
            }
            child.output().await
        }
        .await;
        match result {
            Ok(output) if output.status.success() => {
                CallToolResult::success(vec![ContentBlock::text(
                    String::from_utf8_lossy(&output.stdout).trim().to_owned(),
                )])
            }
            Ok(output) => CallToolResult::error(vec![ContentBlock::text(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            )]),
            Err(_) => CallToolResult::error(vec![ContentBlock::text(
                "Could not execute session command",
            )]),
        }
    }
}

#[tool_router(server_handler)]
impl Server {
    #[tool(
        description = "List local KeyHarbor sessions and their recorded states. States do not prove a browser is alive or a site is authenticated."
    )]
    async fn sessions_list(&self) -> CallToolResult {
        self.execute("list", None, None, false).await
    }

    #[tool(
        description = "Create a local browser session. The name is stored locally and must not contain secrets."
    )]
    async fn session_create(&self, Parameters(Name { name }): Parameters<Name>) -> CallToolResult {
        self.execute("create", Some(&name), None, false).await
    }

    #[tool(
        description = "Get the optional OpenKeylet item name associated by the user with a session. This is metadata only; KeyHarbor does not read or fill credentials or verify the item exists."
    )]
    async fn session_account(&self, Parameters(Id { id }): Parameters<Id>) -> CallToolResult {
        self.execute("account", Some(&id), None, false).await
    }

    #[tool(
        description = "Open a session in the configured browser, optionally navigating to HTTPS or local HTTP without query or fragment. Do not put secrets in URLs. CDP defaults to off; enabling it requires --allow-cdp on the server."
    )]
    async fn session_open(
        &self,
        Parameters(OpenSession { id, url, cdp }): Parameters<OpenSession>,
    ) -> CallToolResult {
        if cdp && !self.allow_cdp {
            return CallToolResult::error(vec![ContentBlock::text(
                "Browser control is disabled; the user must start the MCP server with --allow-cdp",
            )]);
        }
        self.execute("open", Some(&id), url.as_deref(), cdp).await
    }

    #[tool(
        description = "Mark a running session as awaiting user authentication and return a request ID. Existing CDP clients retain control. The user can inspect the request using CLI handoff and resume using the same request ID."
    )]
    async fn session_wait_auth(
        &self,
        Parameters(WaitAuth { id, origin }): Parameters<WaitAuth>,
    ) -> CallToolResult {
        self.execute("wait-auth", Some(&id), origin.as_deref(), false)
            .await
    }

    #[tool(
        description = "Wait for a specific authentication request ID to be resumed, up to 300 seconds. This does not verify login success or who resumed it."
    )]
    async fn session_wait_for_resume(
        &self,
        Parameters(WaitForResume {
            id,
            request_id,
            timeout_seconds,
        }): Parameters<WaitForResume>,
    ) -> CallToolResult {
        if self.session.as_ref().is_some_and(|scope| scope != &id) {
            return CallToolResult::error(vec![ContentBlock::text(
                "Session is outside this server's scope",
            )]);
        }
        let timeout = timeout_seconds.unwrap_or(120);
        if !(1..=300).contains(&timeout) {
            return CallToolResult::error(vec![ContentBlock::text(
                "timeout_seconds must be between 1 and 300",
            )]);
        }
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout);
        loop {
            let root = self.root.clone();
            let id = id.clone();
            let request_id = request_id.clone();
            let snapshot = tokio::time::timeout_at(
                deadline,
                tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
                    let store = keyharbor::Store::open(&root)?;
                    let active = store.supervisor_active(&id)?;
                    let handoff = store.handoff_status(&id, &request_id)?;
                    Ok((active, handoff))
                }),
            )
            .await;
            let (active, handoff) = match snapshot {
                Ok(Ok(Ok(value))) => value,
                Err(_) => {
                    return CallToolResult::error(vec![ContentBlock::text(
                        "Timed out waiting for resume; session may still be awaiting the user",
                    )]);
                }
                _ => {
                    return CallToolResult::error(vec![ContentBlock::text(
                        "Could not read session status",
                    )]);
                }
            };
            if !active {
                return CallToolResult::error(vec![ContentBlock::text(
                    "Session supervisor is not running; inspect before recovery",
                )]);
            }
            let Some(handoff) = handoff else {
                return CallToolResult::error(vec![ContentBlock::text(
                    "Authentication request not found or replaced",
                )]);
            };
            if handoff.session_generation.as_deref() != Some(&handoff.generation) {
                return CallToolResult::error(vec![ContentBlock::text(
                    "Session changed while waiting",
                )]);
            }
            match (handoff.status.as_str(), handoff.session_state.as_str()) {
                ("resumed", "running") => {
                    return CallToolResult::success(vec![ContentBlock::text(
                        "Session state is running; login success and who resumed it are not verified",
                    )]);
                }
                ("pending", "awaiting_user") => (),
                _ => {
                    return CallToolResult::error(vec![ContentBlock::text(format!(
                        "Session is {}",
                        handoff.session_state
                    ))]);
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return CallToolResult::error(vec![ContentBlock::text(
                    "Timed out waiting for resume; session may still be awaiting the user",
                )]);
            }
            tokio::time::sleep_until(
                (tokio::time::Instant::now() + std::time::Duration::from_millis(250)).min(deadline),
            )
            .await;
        }
    }

    #[tool(
        description = "Request session stop. The user must close the browser window; this does not log out of the website."
    )]
    async fn session_stop(&self, Parameters(Id { id }): Parameters<Id>) -> CallToolResult {
        self.execute("stop", Some(&id), None, false).await
    }

    #[tool(
        description = "Get the local CDP endpoint only if the user enabled --allow-cdp. CDP grants full browser control and may expose logged-in data; existing connections are not revoked during authentication handoff."
    )]
    async fn session_endpoint(&self, Parameters(Id { id }): Parameters<Id>) -> CallToolResult {
        if !self.allow_cdp {
            return CallToolResult::error(vec![ContentBlock::text(
                "Browser control is disabled; the user must start the MCP server with --allow-cdp",
            )]);
        }
        self.execute("endpoint", Some(&id), None, false).await
    }
}

pub async fn serve(
    root: PathBuf,
    browser: Option<PathBuf>,
    allow_cdp: bool,
    session: Option<String>,
) -> anyhow::Result<()> {
    Server {
        root,
        browser,
        allow_cdp,
        session,
    }
    .serve(stdio())
    .await?
    .waiting()
    .await?;
    Ok(())
}
