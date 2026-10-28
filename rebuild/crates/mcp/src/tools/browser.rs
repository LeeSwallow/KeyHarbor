//! Browser tools: each one is a control request to the session's supervisor, where the guard
//! resolves placeholders and masks the response.

use super::session::Session;
use crate::server::{Server, fail};
use anyhow::{Result, bail};
use rmcp::{
    handler::server::wrapper::Parameters, model::CallToolResult, schemars, tool, tool_router,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize, schemars::JsonSchema)]
struct Navigate {
    session: String,
    /// HTTP or HTTPS URL.
    url: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct Target {
    session: String,
    /// Element ref from browser_snapshot (the number in brackets).
    #[serde(rename = "ref")]
    reference: Option<u64>,
    /// CSS selector, used when ref is not given.
    selector: Option<String>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct Fill {
    session: String,
    /// Element ref from browser_snapshot.
    #[serde(rename = "ref")]
    reference: Option<u64>,
    /// CSS selector, used when ref is not given.
    selector: Option<String>,
    /// Text that replaces the element's value. May contain {{secret:item.field}} placeholders.
    text: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct Text {
    session: String,
    /// Text typed into the focused element. May contain {{secret:item.field}} placeholders.
    text: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct Key {
    session: String,
    /// Enter, Tab, Escape, Backspace, Delete, Space, ArrowUp/Down/Left/Right, Home, End, PageUp or PageDown.
    key: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct NewTab {
    session: String,
    /// HTTP or HTTPS URL; about:blank when omitted.
    url: Option<String>,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct SwitchTab {
    session: String,
    /// Tab id from browser_tabs.
    id: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct Wait {
    session: String,
    /// Milliseconds to sleep (at most 15000).
    ms: Option<u64>,
    /// Wait until an element matches this CSS selector (at most 15 seconds).
    selector: Option<String>,
}

fn target(reference: Option<u64>, selector: Option<String>) -> Result<Value> {
    match (reference, selector) {
        (Some(reference), _) => Ok(json!({"ref": reference})),
        (None, Some(selector)) => Ok(json!({"selector": selector})),
        (None, None) => bail!("give ref or selector"),
    }
}

#[tool_router(router = browser_tools, vis = "pub(crate)")]
impl Server {
    #[tool(
        description = "Navigate the active tab to an HTTP(S) URL and wait for it to load. Returns url and title."
    )]
    async fn browser_navigate(
        &self,
        Parameters(Navigate { session, url }): Parameters<Navigate>,
    ) -> CallToolResult {
        self.browser(&session, json!({"op": "navigate", "url": url}))
            .await
    }

    #[tool(
        description = "Accessibility snapshot of the active tab: one line per element, `[ref] role \"name\"`. Use ref with browser_click and browser_fill."
    )]
    async fn browser_snapshot(
        &self,
        Parameters(Session { session }): Parameters<Session>,
    ) -> CallToolResult {
        self.browser(&session, json!({"op": "snapshot"})).await
    }

    #[tool(description = "Save a PNG screenshot of the active tab and return its file path.")]
    async fn browser_screenshot(
        &self,
        Parameters(Session { session }): Parameters<Session>,
    ) -> CallToolResult {
        self.browser(&session, json!({"op": "screenshot"})).await
    }

    #[tool(description = "Click an element given by ref (from browser_snapshot) or CSS selector.")]
    async fn browser_click(
        &self,
        Parameters(Target {
            session,
            reference,
            selector,
        }): Parameters<Target>,
    ) -> CallToolResult {
        match target(reference, selector) {
            Ok(target) => {
                self.browser(&session, json!({"op": "click", "target": target}))
                    .await
            }
            Err(error) => fail(error.to_string()),
        }
    }

    #[tool(
        description = "Replace the value of an input given by ref or CSS selector with text. To enter a secret, write its placeholder from vault_list, e.g. {{secret:github.password}}: KeyHarbor types the value only if the policy allows it and, for origin-bound items, the input's frame is one of the item's origins. You never see the value; results show placeholders instead."
    )]
    async fn browser_fill(
        &self,
        Parameters(Fill {
            session,
            reference,
            selector,
            text,
        }): Parameters<Fill>,
    ) -> CallToolResult {
        match target(reference, selector) {
            Ok(target) => {
                self.browser(
                    &session,
                    json!({"op": "fill", "target": target, "text": text}),
                )
                .await
            }
            Err(error) => fail(error.to_string()),
        }
    }

    #[tool(
        description = "Type text into the focused element. Accepts {{secret:item.field}} placeholders under the same checks as browser_fill."
    )]
    async fn browser_type(
        &self,
        Parameters(Text { session, text }): Parameters<Text>,
    ) -> CallToolResult {
        self.browser(&session, json!({"op": "type", "text": text}))
            .await
    }

    #[tool(description = "Press a key in the active tab, e.g. Enter or Tab.")]
    async fn browser_press(
        &self,
        Parameters(Key { session, key }): Parameters<Key>,
    ) -> CallToolResult {
        self.browser(&session, json!({"op": "press", "key": key}))
            .await
    }

    #[tool(description = "List the browser's tabs with id, url, title and which one is active.")]
    async fn browser_tabs(
        &self,
        Parameters(Session { session }): Parameters<Session>,
    ) -> CallToolResult {
        self.browser(&session, json!({"op": "tabs"})).await
    }

    #[tool(description = "Open a new tab (optionally at a URL) and make it active.")]
    async fn browser_new_tab(
        &self,
        Parameters(NewTab { session, url }): Parameters<NewTab>,
    ) -> CallToolResult {
        self.browser(&session, json!({"op": "new_tab", "url": url}))
            .await
    }

    #[tool(description = "Make the tab with the given id (from browser_tabs) active.")]
    async fn browser_switch_tab(
        &self,
        Parameters(SwitchTab { session, id }): Parameters<SwitchTab>,
    ) -> CallToolResult {
        self.browser(&session, json!({"op": "switch_tab", "id": id}))
            .await
    }

    #[tool(
        description = "Wait for a CSS selector to match (up to 15 s) or for a number of milliseconds."
    )]
    async fn browser_wait(
        &self,
        Parameters(Wait {
            session,
            ms,
            selector,
        }): Parameters<Wait>,
    ) -> CallToolResult {
        let mut request = json!({"op": "wait"});
        if let Some(ms) = ms {
            request["ms"] = json!(ms);
        }
        if let Some(selector) = selector {
            request["selector"] = json!(selector);
        }
        self.browser(&session, request).await
    }
}
