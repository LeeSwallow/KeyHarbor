//! The showcase renders in real Chrome, light and dark: the data-driven sections all appear.
//! Runs only when KEYHARBOR_TEST_CHROME names a Chrome executable, like the browser tests.

use std::path::PathBuf;
use std::process::Command;

use keyharbor_design_check::{repo_root, serve};

fn chrome() -> Option<PathBuf> {
    match std::env::var_os("KEYHARBOR_TEST_CHROME") {
        Some(path) if !path.is_empty() => Some(PathBuf::from(path)),
        _ => {
            eprintln!("skipped: set KEYHARBOR_TEST_CHROME to a Chrome executable to run");
            None
        }
    }
}

/// The DOM after scripts ran, as Chrome serialises it.
fn dump_dom(chrome: &PathBuf, url: &str) -> String {
    let profile = tempfile::tempdir().unwrap();
    let out = Command::new(chrome)
        .args([
            "--headless=new",
            "--disable-gpu",
            "--no-first-run",
            "--no-default-browser-check",
            "--virtual-time-budget=5000",
            "--dump-dom",
        ])
        .arg(format!("--user-data-dir={}", profile.path().display()))
        .arg(url)
        .output()
        .expect("run Chrome");
    assert!(
        out.status.success(),
        "Chrome failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// The text from `start` up to the next `end`.
fn between<'a>(dom: &'a str, start: &str, end: &str) -> &'a str {
    let from = dom
        .find(start)
        .unwrap_or_else(|| panic!("{start} not in the page"));
    let rest = &dom[from..];
    &rest[..rest.find(end).unwrap_or(rest.len())]
}

#[test]
fn the_showcase_renders_every_section_in_both_themes() {
    let Some(chrome) = chrome() else { return };
    let base = serve::spawn(repo_root(), "127.0.0.1:0").unwrap();
    for theme in ["light", "dark"] {
        let dom = dump_dom(
            &chrome,
            &format!("{base}/design-system/index.html?theme={theme}"),
        );
        assert!(
            dom.contains(&format!("data-theme=\"{theme}\"")),
            "{theme}: theme switch did not run"
        );
        for id in ["composed", "icons", "components", "tokens"] {
            assert!(
                dom.contains(&format!("<section id=\"{id}\"")),
                "{theme}: section {id} missing"
            );
        }
        let icons = between(&dom, "id=\"icons\"", "</div>")
            .matches("<figure>")
            .count();
        assert!(icons >= 41, "{theme}: {icons} icon figures");
        let buttons = between(&dom, "id=\"btnmatrix\"", "</div>")
            .matches("<button class=\"kh-btn")
            .count();
        assert_eq!(buttons, 4 * 6, "{theme}: button matrix");
        assert!(
            between(&dom, "id=\"colors\"", "</section>").contains("--c-dom-dev"),
            "{theme}: developer domain swatch"
        );
        assert_eq!(
            dom.matches("pattern-sample").count(),
            4,
            "{theme}: pattern samples"
        );
    }
}
