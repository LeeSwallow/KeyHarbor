//! `keyharbor run`: inject env items into one child process and mask its output (ADR-0010).
//!
//! Every referenced item is checked first (exists, not discarded, allowed for the agent, not
//! origin-bound); a refusal is recorded and the command is not started. The child's stdout and
//! stderr pass through a masker built from the whole vault, line by line.

use anyhow::{Context, Result, anyhow, bail};
use clap::Args;
use keyharbor_core::{
    ItemStatus, Vault, fill_template,
    mask::Masker,
    parse_dotenv,
    policy::{Action, decide},
};
use keyharbor_domain::{
    audit::{AuditEvent, AuditKind},
    policy::{ANY, Rule, parse_agent},
    vault::{FieldName, ItemKind, ItemName, PlaceholderRef, parse_placeholders},
};
use keyharbor_sqlite::SqliteStore;
use keyharbor_vault::keystore::keystore_from_env;
use std::{
    ffi::OsString,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use zeroize::Zeroizing;

#[derive(Args)]
pub struct RunArgs {
    /// Agent name for policies and the usage record
    #[arg(long, default_value = keyharbor_core::LOCAL_AGENT)]
    agent: String,
    /// Inject every variable of this env item
    #[arg(long = "env", value_name = "ITEM")]
    items: Vec<String>,
    /// Inject KEY=VALUE lines whose values may hold {{secret:item.field}} placeholders
    #[arg(long = "env-file", value_name = "PATH")]
    files: Vec<PathBuf>,
    /// The command and its arguments, after `--`
    #[arg(last = true, required = true, value_name = "COMMAND")]
    command: Vec<OsString>,
}

struct Checker<'a> {
    vault: &'a Vault,
    store: &'a SqliteStore,
    rules: Vec<Rule>,
    agent: &'a str,
    checked: Vec<ItemName>,
}

impl Checker<'_> {
    fn deny(&self, item: &ItemName, reason: &str) -> anyhow::Error {
        let _ = self.store.record(
            &AuditEvent::new(self.agent, AuditKind::SecretDenied, reason).item(item.as_str()),
        );
        anyhow!("denied: {} ({reason})", item.as_str())
    }

    /// Checks `item` once; returns its field names.
    fn check(&mut self, item: &ItemName, env_only: bool) -> Result<Vec<String>> {
        let info = match self.vault.item_use(item) {
            Ok(Some(info)) => info,
            Ok(None) => return Err(self.deny(item, "not_found")),
            Err(_) => return Err(self.deny(item, "tampered")),
        };
        if env_only && info.kind != ItemKind::Env {
            bail!(
                "{} is not an env item; use --env-file with placeholders",
                item.as_str()
            );
        }
        if info.status == ItemStatus::Discarded {
            return Err(self.deny(item, "discarded"));
        }
        let action = Action::UseItem {
            item: item.clone(),
            domain: info.domain,
        };
        if !decide(&self.rules, self.agent, &action).allowed() {
            return Err(self.deny(item, "policy"));
        }
        // Origin-bound values are only ever typed into their sites, never handed to a process.
        if info.origin_binding {
            return Err(self.deny(item, "origin"));
        }
        if !self.checked.contains(item) {
            self.checked.push(item.clone());
        }
        Ok(info.fields)
    }
}

fn add_var(
    vars: &mut Vec<(String, Zeroizing<String>)>,
    name: &str,
    value: Zeroizing<String>,
) -> Result<()> {
    if vars.iter().any(|(n, _)| n == name) {
        bail!("variable {name} given twice");
    }
    vars.push((name.to_owned(), value));
    Ok(())
}

/// Copies `input` to `output` a line at a time through the masker.
fn pump(input: impl Read, mut output: impl Write, masker: &Masker) {
    let mut reader = BufReader::new(input);
    let mut line = Zeroizing::new(Vec::new());
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {
                let masked = masker.redact_bytes(&line);
                if output
                    .write_all(&masked)
                    .and_then(|()| output.flush())
                    .is_err()
                {
                    return;
                }
            }
        }
    }
}

/// Runs the command and returns the exit code to pass on (128 + signal if it was killed).
pub fn run(root: &Path, args: RunArgs) -> Result<i32> {
    if args.agent == ANY {
        bail!("agent name * is reserved for rules that apply to every agent");
    }
    let agent = parse_agent(&args.agent).map_err(|e| anyhow!("{e}"))?;
    if args.items.is_empty() && args.files.is_empty() {
        bail!("give at least one --env ITEM or --env-file PATH");
    }
    let store = SqliteStore::open(root)?;
    let vault = Vault::open(SqliteStore::open(root)?, keystore_from_env())?;
    let mut checker = Checker {
        vault: &vault,
        store: &store,
        rules: store.policy_rules()?,
        agent: &agent,
        checked: Vec::new(),
    };

    // Check everything before resolving anything.
    let mut item_vars: Vec<(ItemName, Vec<String>)> = Vec::new();
    for name in &args.items {
        let item = ItemName::parse(name).map_err(|e| anyhow!("{e}"))?;
        let fields = checker.check(&item, true)?;
        item_vars.push((item, fields));
    }
    let mut templates = Vec::new();
    for path in &args.files {
        let text = Zeroizing::new(
            std::fs::read_to_string(path)
                .with_context(|| format!("cannot read {}", path.display()))?,
        );
        for (name, template) in parse_dotenv(&text)? {
            for reference in parse_placeholders(&template).map_err(|e| anyhow!("{e}"))? {
                let fields = checker.check(&reference.item, false)?;
                if !fields.iter().any(|f| f == reference.field.as_str()) {
                    return Err(checker.deny(&reference.item, "not_found"));
                }
            }
            templates.push((name, template));
        }
    }

    let mut vars: Vec<(String, Zeroizing<String>)> = Vec::new();
    for (item, fields) in &item_vars {
        for field in fields {
            let reference = PlaceholderRef {
                item: item.clone(),
                field: FieldName::parse(field).map_err(|e| anyhow!("{e}"))?,
            };
            add_var(&mut vars, field, vault.resolve(&reference)?)?;
        }
    }
    for (name, template) in &templates {
        let value = fill_template(template, |r| vault.resolve(r))?;
        add_var(&mut vars, name.as_str(), value)?;
    }
    let masker = vault.masker()?;

    let mut command = Command::new(&args.command[0]);
    command
        .args(&args.command[1..])
        .envs(vars.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // SAFETY: setrlimit is async-signal-safe.
    unsafe {
        use std::os::unix::process::CommandExt;
        command.pre_exec(|| {
            let none = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            if libc::setrlimit(libc::RLIMIT_CORE, &none) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("could not start {}", Path::new(&args.command[0]).display()))?;
    drop(command);
    drop(vars);
    for item in &checker.checked {
        let _ = store
            .record(&AuditEvent::new(&agent, AuditKind::EnvInjected, "ok").item(item.as_str()));
    }
    let (out, err) = (
        child.stdout.take().context("child stdout")?,
        child.stderr.take().context("child stderr")?,
    );
    let status = std::thread::scope(|scope| {
        scope.spawn(|| pump(out, std::io::stdout().lock(), &masker));
        scope.spawn(|| pump(err, std::io::stderr().lock(), &masker));
        child.wait()
    })?;
    use std::os::unix::process::ExitStatusExt;
    Ok(status
        .code()
        .or_else(|| status.signal().map(|s| 128 + s))
        .unwrap_or(1))
}
