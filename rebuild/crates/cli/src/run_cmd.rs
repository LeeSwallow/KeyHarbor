//! `keyharbor run`: inject env items into one child process and mask its output (ADR-0010).
//!
//! Every referenced item is checked first (exists, not discarded, allowed for the agent, not
//! origin-bound); a refusal is recorded and the command is not started. The child's stdout and
//! stderr pass through a masker built from the whole vault, line by line.

use crate::compose::App;
use anyhow::{Context, Result, anyhow, bail};
use clap::Args;
use keyharbor_domain::policy::AgentName;
use keyharbor_guard::{pep::mask::Masker, run as guard_run};
use std::{
    ffi::OsString,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
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

/// Starts the child with `vars` and no core dumps, its output piped back to us. The
/// `Command` holding the values is dropped before this returns.
fn spawn(argv: &[OsString], vars: &[(String, Zeroizing<String>)]) -> Result<Child> {
    let mut command = Command::new(&argv[0]);
    command
        .args(&argv[1..])
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
    command
        .spawn()
        .with_context(|| format!("could not start {}", Path::new(&argv[0]).display()))
}

/// Runs the command and returns the exit code to pass on (128 + signal if it was killed).
pub fn run(app: &App, args: RunArgs) -> Result<i32> {
    let agent = AgentName::parse(&args.agent).map_err(|e| anyhow!("{e}"))?;
    if args.items.is_empty() && args.files.is_empty() {
        bail!("give at least one --env ITEM or --env-file PATH");
    }
    let (store, vault) = (app.store()?, app.vault()?);
    // The guard checks every item before resolving any, and builds the output masker.
    let mut injection = guard_run::prepare(
        &*store,
        &vault,
        &guard_run::Request {
            agent: agent.as_str(),
            items: &args.items,
            files: &args.files,
        },
    )?;
    let vars = std::mem::take(&mut injection.vars);
    let mut child = spawn(&args.command, &vars)?;
    drop(vars);
    injection.record_injected(&*store);
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
