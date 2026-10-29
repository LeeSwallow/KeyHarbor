//! `keyharbor run`: inject env items into one child process and mask its output (ADR-0010).
//!
//! Every referenced item is checked first (exists, not discarded, allowed for the agent, not
//! origin-bound); a refusal is recorded and the command is not started. The child's stdout and
//! stderr pass through a masker built from the whole vault, line by line.

use crate::compose::App;
use anyhow::{Context, Result, anyhow, bail};
use clap::Args;
use keyharbor_domain::policy::AgentName;
use keyharbor_guard::{
    pep::mask::{Masker, StreamMasker},
    run as guard_run,
};
use std::{
    ffi::OsString,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
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

/// How long output may sit unshown when the child stops writing mid-line (a prompt, a
/// progress bar). Fixed: short enough to feel immediate, long enough that one write is not
/// split by a pause between the reads of it.
const IDLE_FLUSH: Duration = Duration::from_millis(100);

/// Copies `input` to `output` through a stream masker: what is safe is shown at once, and
/// the rest when the child pauses or closes the stream (J-83).
fn pump(input: impl Read + Send, mut output: impl Write, masker: &Masker) {
    let (chunks, received) = mpsc::channel::<Zeroizing<Vec<u8>>>();
    std::thread::scope(|scope| {
        scope.spawn(move || read_chunks(input, &chunks));
        let mut stream = StreamMasker::new(masker);
        loop {
            let shown = match received.recv_timeout(IDLE_FLUSH) {
                Ok(chunk) => stream.push(&chunk),
                Err(mpsc::RecvTimeoutError::Timeout) => stream.idle(),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let _ = output
                        .write_all(&stream.flush())
                        .and_then(|()| output.flush());
                    return;
                }
            };
            if !shown.is_empty()
                && output
                    .write_all(&shown)
                    .and_then(|()| output.flush())
                    .is_err()
            {
                return;
            }
        }
    });
}

/// Sends what `input` gives, as it comes, until it ends.
fn read_chunks(mut input: impl Read, chunks: &mpsc::Sender<Zeroizing<Vec<u8>>>) {
    let mut buffer = Zeroizing::new(vec![0u8; 8192]);
    loop {
        match input.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                if chunks.send(Zeroizing::new(buffer[..n].to_vec())).is_err() {
                    return;
                }
            }
        }
    }
}

/// Signals asking `keyharbor run` to stop, which the child gets instead (J-83). SIGINT is not
/// here: Ctrl-C reaches the child from the terminal, as it is in our process group.
const FORWARDED: [libc::c_int; 2] = [libc::SIGTERM, libc::SIGHUP];

fn forwarded_set() -> libc::sigset_t {
    // SAFETY: sigemptyset and sigaddset only write the set they are given.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        for signal in FORWARDED {
            libc::sigaddset(&mut set, signal);
        }
        set
    }
}

/// Blocks the forwarded signals in this thread (and the threads it starts after), so that
/// only [`forward_signals`] receives them.
fn block_forwarded() -> Result<()> {
    let set = forwarded_set();
    // SAFETY: pthread_sigmask reads `set` and changes this thread's mask only.
    if unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut()) } != 0 {
        bail!("could not block signals");
    }
    Ok(())
}

/// Passes every forwarded signal on to the child until this process exits.
fn forward_signals(child: u32) {
    let Ok(pid) = libc::pid_t::try_from(child) else {
        return;
    };
    std::thread::spawn(move || {
        let set = forwarded_set();
        loop {
            let mut signal = 0;
            // SAFETY: sigwait reads `set` and writes `signal`; the signals are blocked.
            if unsafe { libc::sigwait(&set, &mut signal) } != 0 {
                return;
            }
            // SAFETY: kill only sends a signal.
            unsafe { libc::kill(pid, signal) };
        }
    });
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
    let set = forwarded_set();
    // SAFETY: pthread_sigmask and setrlimit are async-signal-safe; the set is built before fork.
    unsafe {
        use std::os::unix::process::CommandExt;
        command.pre_exec(move || {
            if libc::pthread_sigmask(libc::SIG_UNBLOCK, &set, std::ptr::null_mut()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
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

/// The `--env-file` templates; they may hold values, so they are zeroized on drop.
fn read_env_files(paths: &[PathBuf]) -> Result<Vec<Zeroizing<String>>> {
    paths
        .iter()
        .map(|path| {
            std::fs::read_to_string(path)
                .map(Zeroizing::new)
                .with_context(|| format!("cannot read {}", path.display()))
        })
        .collect()
}

/// Runs the command and returns the exit code to pass on (128 + signal if it was killed).
pub fn run(app: &App, args: RunArgs) -> Result<i32> {
    let agent = AgentName::parse(&args.agent).map_err(|e| anyhow!("{e}"))?;
    if args.items.is_empty() && args.files.is_empty() {
        bail!("give at least one --env ITEM or --env-file PATH");
    }
    let files = read_env_files(&args.files)?;
    let (store, vault) = (app.store()?, app.vault()?);
    // The guard checks every item before resolving any, and builds the output masker.
    let mut injection = guard_run::prepare(
        &*store,
        &vault,
        &guard_run::Request {
            agent: agent.as_str(),
            items: &args.items,
            files: &files,
        },
    )?;
    let vars = std::mem::take(&mut injection.vars);
    block_forwarded()?;
    let mut child = spawn(&args.command, &vars)?;
    forward_signals(child.id());
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
