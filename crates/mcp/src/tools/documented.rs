//! Values the tool schemas spell out for agents, checked against the tables and constants
//! that enforce them: if one changes, the schema text must change with it (J-195).

use crate::server::router;
use keyharbor_core::session::ResumeWait;
use keyharbor_domain::vault::KINDS;
use keyharbor_supervisor::{BROWSER_WAIT_TIMEOUT, browser_keys};
use rmcp::model::Tool;
use std::collections::BTreeSet;

fn tools() -> Vec<Tool> {
    router(true).list_all()
}

/// `(tool, description)` of every argument named `property`.
fn argument(property: &str) -> Vec<(String, String)> {
    tools()
        .into_iter()
        .filter_map(|tool| {
            let described = tool.input_schema.get("properties")?.get(property)?;
            let text = described.get("description")?.as_str()?.to_owned();
            Some((tool.name.to_string(), text))
        })
        .collect()
}

/// `a, b or c.`, the way the schemas list choices.
fn choices(names: &[&str]) -> String {
    let (last, rest) = names.split_last().expect("at least one choice");
    format!("{} or {last}.", rest.join(", "))
}

#[test]
fn the_kind_argument_lists_every_registered_kind() {
    let kinds: Vec<&str> = KINDS.iter().map(|spec| spec.name).collect();
    let described = argument("kind");
    assert!(described.len() >= 2, "{described:?}");
    for (tool, text) in described {
        assert_eq!(text, choices(&kinds), "{tool}");
    }
}

#[test]
fn wait_timeouts_state_the_enforced_bounds() {
    let (max, default) = (ResumeWait::MAX_SECONDS, ResumeWait::DEFAULT_SECONDS);
    let described = argument("timeout_seconds");
    assert!(described.len() >= 3, "{described:?}");
    for (tool, text) in described {
        assert!(
            text.contains(&format!("1 to {max}; default {default}")),
            "{tool}: {text}"
        );
    }
    let in_tool = format!("timeout_seconds (1–{max}, default {default})");
    let waiting: Vec<Tool> = tools()
        .into_iter()
        .filter(|tool| {
            tool.input_schema
                .get("properties")
                .and_then(|p| p.get("timeout_seconds"))
                .is_some()
        })
        .collect();
    for tool in waiting {
        let text = tool.description.as_deref().unwrap_or_default();
        assert!(text.contains(&in_tool), "{}: {text}", tool.name);
    }
}

#[test]
fn browser_wait_states_the_brokers_limit() {
    let limit = BROWSER_WAIT_TIMEOUT;
    let ms = argument("ms");
    assert_eq!(ms.len(), 1, "{ms:?}");
    assert!(
        ms[0].1.contains(&format!("at most {})", limit.as_millis())),
        "{ms:?}"
    );
    let selector: Vec<_> = argument("selector")
        .into_iter()
        .filter(|(tool, _)| tool == "browser_wait")
        .collect();
    assert_eq!(selector.len(), 1, "{selector:?}");
    assert!(
        selector[0]
            .1
            .contains(&format!("at most {} seconds", limit.as_secs())),
        "{selector:?}"
    );
}

#[test]
fn browser_press_lists_exactly_the_keys_the_broker_presses() {
    let described = argument("key");
    assert_eq!(described.len(), 1, "{described:?}");
    let text = described[0].1.trim_end_matches('.').replace(" or ", ", ");
    let mut listed = BTreeSet::new();
    for name in text.split(", ") {
        // `ArrowUp/Down/Left/Right` is four keys.
        let mut parts = name.split('/');
        let first = parts.next().unwrap_or_default();
        listed.insert(first.to_owned());
        let stem = first.strip_suffix("Up").unwrap_or(first);
        listed.extend(parts.map(|part| format!("{stem}{part}")));
    }
    let pressed: BTreeSet<String> = browser_keys().map(str::to_owned).collect();
    assert_eq!(listed, pressed);
}
