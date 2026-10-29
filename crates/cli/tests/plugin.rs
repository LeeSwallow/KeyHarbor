//! The Claude Code / Codex plugin in `plugin/` (phase 6): manifests parse and agree, skills
//! are well formed and name only tools and commands that exist, the MCP command starts, and
//! the session hook stays quiet without keyharbor.
#![cfg(unix)]

mod common;

use common::{mcp::Mcp, short_tempdir};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn json(path: &str) -> Value {
    let text = fs::read_to_string(repo().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}
fn keyharbor(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_keyharbor"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}
/// Every Markdown file the agent reads: skills, commands, rules.
fn agent_docs() -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    for dir in ["plugin/skills", "plugin/commands", "plugin/rules"] {
        let mut stack = vec![repo().join(dir)];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "md") {
                    let text = fs::read_to_string(&path).unwrap();
                    out.push((path, text));
                }
            }
        }
    }
    out
}
/// The text between each pair of backticks (code spans and fenced lines alike).
fn code_spans(text: &str) -> Vec<String> {
    let mut spans: Vec<String> = text
        .split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect();
    spans.extend(
        text.split("```")
            .skip(1)
            .step_by(2)
            .flat_map(|block| block.lines().map(str::to_owned).collect::<Vec<_>>()),
    );
    spans
}

#[test]
fn manifests_parse_and_agree_on_name_and_version() {
    let version = env!("CARGO_PKG_VERSION");
    let claude = json("plugin/.claude-plugin/plugin.json");
    let codex = json("plugin/.codex-plugin/plugin.json");
    let market = json(".claude-plugin/marketplace.json");
    let agents = json(".agents/plugins/marketplace.json");
    for (what, manifest) in [("claude", &claude), ("codex", &codex)] {
        assert_eq!(manifest["name"], "keyharbor", "{what}");
        assert_eq!(manifest["version"], version, "{what}");
    }
    for (what, market) in [("claude", &market), ("agents", &agents)] {
        assert_eq!(market["plugins"][0]["name"], "keyharbor", "{what}");
        assert_eq!(market["plugins"][0]["version"], version, "{what}");
    }
    assert_eq!(codex["interface"]["brandColor"], "#05A8A8");
    for hooks in [
        "plugin/hooks/claude-hooks.json",
        "plugin/hooks/codex-hooks.json",
    ] {
        let command = json(hooks)["hooks"]["SessionStart"][0]["hooks"][0]["command"].clone();
        assert!(
            command
                .as_str()
                .unwrap()
                .contains("scripts/session-start.sh"),
            "{hooks}"
        );
    }
    let script = repo().join("plugin/scripts/session-start.sh");
    use std::os::unix::fs::PermissionsExt;
    assert_ne!(
        fs::metadata(&script).unwrap().permissions().mode() & 0o111,
        0
    );
    assert!(
        Command::new("sh")
            .arg("-n")
            .arg(&script)
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn skills_and_commands_are_well_formed() {
    let mut skills = 0;
    for (path, text) in agent_docs() {
        let rules = path.parent().unwrap().ends_with("rules");
        if rules {
            continue;
        }
        let front = text
            .strip_prefix("---\n")
            .and_then(|rest| rest.split_once("\n---\n"))
            .unwrap_or_else(|| panic!("{} has no front matter", path.display()))
            .0;
        let field = |key: &str| {
            front
                .lines()
                .find_map(|l| l.strip_prefix(&format!("{key}: ")))
                .unwrap_or_else(|| panic!("{} has no {key}", path.display()))
        };
        if path.file_name().unwrap() == "SKILL.md" {
            skills += 1;
            let dir = path
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap();
            assert_eq!(field("name"), dir, "{}", path.display());
            assert!(
                field("description").starts_with("Use when"),
                "{}",
                path.display()
            );
            assert!(
                text.lines().count() <= 80,
                "{} is over 80 lines",
                path.display()
            );
        } else {
            assert!(!field("description").is_empty(), "{}", path.display());
        }
    }
    assert_eq!(skills, 6);
}

#[test]
fn skills_name_only_tools_and_commands_that_exist() {
    let tools: BTreeSet<String> = String::from_utf8(keyharbor(&["mcp", "--list-tools"]).stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    assert!(tools.contains("vault_list"), "{tools:?}");
    let mut checked = 0;
    for (path, text) in agent_docs() {
        for span in code_spans(&text) {
            let word: String = span
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || *c == '_')
                .collect();
            let tool_like = ["session_", "browser_", "site_state_", "vault_"]
                .iter()
                .any(|p| word.starts_with(p));
            if tool_like && word.len() > 6 {
                assert!(
                    tools.contains(&word),
                    "{} names `{word}`, which is no tool",
                    path.display()
                );
                checked += 1;
            }
            for (i, _) in span.match_indices("keyharbor ") {
                let words: Vec<&str> = span[i + "keyharbor ".len()..]
                    .split_whitespace()
                    .take_while(|w| {
                        !w.starts_with('-') && w.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                    })
                    .take(2)
                    .collect();
                if words.is_empty() {
                    continue;
                }
                let mut args = words.clone();
                args.push("--help");
                let out = keyharbor(&args);
                assert!(
                    out.status.success(),
                    "{} names `keyharbor {}`",
                    path.display(),
                    words.join(" ")
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 30, "only {checked} names checked");
}

#[test]
fn the_mcp_command_starts_and_lists_tools() {
    let server = &json("plugin/.mcp.json")["mcpServers"]["keyharbor"];
    assert_eq!(server["command"], "keyharbor");
    let args: Vec<&str> = server["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert_eq!(args[0], "mcp");
    let tmp = short_tempdir();
    let mut mcp = Mcp::start(&tmp.path().join("data"), tmp.path(), &args[1..]);
    assert!(mcp.tools().iter().any(|t| t == "browser_login"));
    mcp.finish();
}

#[test]
fn the_session_hook_is_silent_without_keyharbor_and_names_nothing_with_it() {
    let script = repo().join("plugin/scripts/session-start.sh");
    let run = |path: &str, home: &Path| {
        let out = Command::new("sh")
            .arg(&script)
            .env("PATH", path)
            .env("KEYHARBOR_HOME", home)
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap()
    };
    let tmp = short_tempdir();
    let empty = tmp.path().join("empty");
    fs::create_dir(&empty).unwrap();
    assert_eq!(
        run(
            &format!("{}:/usr/bin:/bin", empty.display()),
            &tmp.path().join("d")
        ),
        ""
    );
    let bin = tmp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_keyharbor"), bin.join("keyharbor")).unwrap();
    let said = run(
        &format!("{}:/usr/bin:/bin", bin.display()),
        &tmp.path().join("d"),
    );
    assert_eq!(said.lines().count(), 1, "{said}");
    assert!(
        said.contains("vault not initialized") && said.contains("using-keyharbor"),
        "{said}"
    );
}
