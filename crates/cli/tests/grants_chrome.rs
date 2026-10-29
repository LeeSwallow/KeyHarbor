//! Grants hold a session to the sites it was given (R-43, phase 4 Task 7), with real Chrome.
#![cfg(unix)]

mod common;

use common::{chrome, mcp::Mcp, on_terminal, person::Person, serve_fn, short_tempdir};
use serde_json::json;
use std::{
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

fn cli(data: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .arg("--data-dir")
        .arg(data)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn wait_until(mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done() {
        assert!(Instant::now() < deadline, "timed out");
        thread::sleep(Duration::from_millis(200));
    }
}

/// A server that records the paths it was asked for.
fn recording(
    page: impl Fn(&str) -> String + Send + Sync + 'static,
) -> (String, Arc<Mutex<Vec<String>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let base = serve_fn(move |path| {
        log.lock().unwrap().push(path.to_owned());
        page(path)
    });
    (base, seen)
}

#[test]
fn a_granted_session_loads_only_its_sites_and_loses_their_cookies_when_revoked() {
    let Some(chrome) = chrome() else { return };
    let (other, other_seen) =
        recording(|_| "<!doctype html><title>Other</title><p>other</p>".into());
    let link = other.clone();
    let (site, _) = recording(move |_| {
        format!(
            "<!doctype html><title>Site</title><script>document.cookie='kh=1; path=/'</script>\
             <img id=pixel src=\"{link}/pixel\"><a id=out href=\"{link}/landing\">out</a>"
        )
    });
    let tmp = short_tempdir();
    let (data, keys) = (tmp.path().join("data"), tmp.path().join("keys"));
    let id = String::from_utf8(cli(&data, &["create", "c"]).stdout)
        .unwrap()
        .split('\t')
        .next()
        .unwrap()
        .to_owned();

    // Adding a grant needs a person; without one nothing is added.
    assert!(!cli(&data, &["grant", "add", &id, &site]).status.success());
    let added = on_terminal(
        &data,
        &keys,
        &["grant", "add", &id, &site, "--ttl", "1h"],
        "yes",
    );
    assert!(added.success, "{}", added.output);
    let listed = String::from_utf8(cli(&data, &["grant", "list", &id]).stdout).unwrap();
    let grant = listed.split('\t').next().unwrap().to_owned();
    assert!(listed.contains(&format!("\t{site}\tactive\t")), "{listed}");

    let mut mcp = Mcp::start(
        &data,
        tmp.path(),
        &["--browser", chrome.to_str().unwrap(), "--allow-cdp"],
    );
    mcp.ok("session_open", json!({"id": id, "cdp": true}));
    let endpoint = mcp.ok("session_endpoint", json!({"id": id}))["endpoint"]
        .as_str()
        .unwrap()
        .to_owned();
    let page = mcp.ok(
        "browser_navigate",
        json!({"session": id, "url": format!("{site}/")}),
    );
    assert_eq!(page["title"], "Site");
    // Subresources from elsewhere still load.
    wait_until(|| other_seen.lock().unwrap().iter().any(|p| p == "/pixel"));

    let refused = mcp.err(
        "browser_navigate",
        json!({"session": id, "url": format!("{other}/")}),
    );
    assert!(refused.contains("out-of-grant"), "{refused}");
    // A link out of the grant does not load either, whoever follows it.
    mcp.ok("browser_click", json!({"session": id, "selector": "#out"}));
    thread::sleep(Duration::from_secs(1));
    // (`serve_fn` logs a preconnect that sends nothing as "/", so only the target counts.)
    let seen = other_seen.lock().unwrap().clone();
    assert!(!seen.iter().any(|p| p == "/landing"), "{seen:?}");

    let mut person = Person::connect(&endpoint);
    let cookies = |person: &mut Person| {
        person.browser("Storage.getCookies", json!({}))["cookies"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "kh")
    };
    assert!(cookies(&mut person));
    let revoked = cli(&data, &["grant", "revoke", &grant]);
    assert!(
        revoked.status.success(),
        "{}",
        String::from_utf8_lossy(&revoked.stderr)
    );
    wait_until(|| !cookies(&mut person));
    let refused = mcp.err(
        "browser_navigate",
        json!({"session": id, "url": format!("{site}/")}),
    );
    assert!(refused.contains("out-of-grant"), "{refused}");

    let audit = String::from_utf8(cli(&data, &["audit", "--json"]).stdout).unwrap();
    assert!(audit.contains("navigation_denied"), "{audit}");
    assert!(audit.contains("\"cleared\""), "{audit}");
    mcp.ok("session_stop", json!({"id": id}));
    mcp.finish();
}
