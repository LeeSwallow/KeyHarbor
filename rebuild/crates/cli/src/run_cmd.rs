//! `keyharbor run`: inject env items into one child process and mask its output (ADR-0010).
//!
//! Every referenced item is checked first (exists, not discarded, allowed for the agent, not
//! origin-bound); a refusal is recorded and the command is not started. The child's stdout and
//! stderr pass through a masker built from the whole vault, line by line.

use anyhow::{Context, Result, anyhow, bail};
use clap::Args;
use keyharbor_core::Vault;
use keyharbor_domain::policy::{ANY, parse_agent};
use keyharbor_guard::{pep::mask::Masker, run as guard_run};
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
    // The guard checks every item before resolving any, and builds the output masker.
    let mut injection = guard_run::prepare(
        &store,
        &vault,
        &guard_run::Request {
            agent: &agent,
            items: &args.items,
            files: &args.files,
        },
    )?;
    let vars = std::mem::take(&mut injection.vars);

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
    injection.record_injected(&store);
    let (out, err) = (
        child.stdout.take().context("child stdout")?,
        child.stderr.take().context("child stderr")?,
    );
    let status = std::thread::scope(|scope| {
        scope.spawn(|| pump(out, std::io::stdout().lock(), &injection.masker));
        scope.spawn(|| pump(err, std::io::stderr().lock(), &injection.masker));
        child.wait()
    })?;
    use std::os::unix::process::ExitStatusExt;
    Ok(status
        .code()
        .or_else(|| status.signal().map(|s| 128 + s))
        .unwrap_or(1))
}
