use anyhow::{Context, Result, bail};
mod mcp;
use clap::{Parser, Subcommand};
use keyharbor::{Store, validate_url};
use std::{
    fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Parser)]
#[command(version, about = "Local browser session management")]
struct Cli {
    #[arg(long, global = true, env = "KEYHARBOR_HOME")]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    Create {
        name: String,
    },
    List,
    Backup {
        destination: PathBuf,
    },
    Show {
        id: String,
    },
    BindAccount {
        id: String,
        item: String,
    },
    Account {
        id: String,
    },
    UnbindAccount {
        id: String,
    },
    Mcp {
        #[arg(long)]
        browser: Option<PathBuf>,
        #[arg(
            long,
            help = "Allow full local browser control for sessions opened by this MCP server"
        )]
        allow_cdp: bool,
        #[arg(long)]
        session: Option<String>,
    },
    Open {
        id: String,
        #[arg(long)]
        browser: Option<PathBuf>,
        #[arg(long)]
        url: Option<String>,
        #[arg(long, help = "Enable full local browser control through CDP")]
        cdp: bool,
    },
    Endpoint {
        id: String,
    },
    WaitAuth {
        id: String,
        #[arg(long)]
        origin: Option<String>,
    },
    Handoff {
        id: String,
    },
    Resume {
        id: String,
        request_id: String,
    },
    Stop {
        id: String,
    },
    Recover {
        id: String,
        #[arg(long)]
        confirm_browser_closed: bool,
    },
    #[command(hide = true)]
    Supervise {
        id: String,
        generation: String,
        browser: PathBuf,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        cdp: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = match cli.data_dir {
        Some(path) => path,
        None => PathBuf::from(std::env::var_os("HOME").context("HOME is not set; use --data-dir")?)
            .join(".keyharbor"),
    };
    let store = Store::open(&root)?;
    match cli.command {
        Action::Create { name } => {
            let s = store.create(&name)?;
            println!("{}\t{}\t{}", s.id, s.name, s.state);
        }
        Action::List => {
            for s in store.list()? {
                show(&store, s)?;
            }
        }
        Action::Backup { destination } => {
            store.backup(&destination)?;
            println!("SQLite metadata backup created: {}", destination.display());
        }
        Action::Show { id } => show(&store, store.get(&id)?)?,
        Action::BindAccount { id, item } => {
            store.bind_account(&id, &item)?;
            println!("account reference saved; no credential was accessed");
        }
        Action::Account { id } => {
            if let Some((provider, item)) = store.account(&id)? {
                println!("{provider}\t{item}");
            } else {
                println!("no account reference");
            }
        }
        Action::UnbindAccount { id } => {
            store.unbind_account(&id)?;
            println!("account reference removed");
        }
        Action::Mcp {
            browser,
            allow_cdp,
            session,
        } => {
            if let Some(id) = &session {
                store.get(id)?;
            }
            let _owner = session
                .as_deref()
                .map(|id| store.lock_mcp_session(id))
                .transpose()?;
            tokio::runtime::Runtime::new()?
                .block_on(mcp::serve(root, browser, allow_cdp, session))?;
        }
        Action::Open {
            id,
            browser,
            url,
            cdp,
        } => {
            let url = url.as_deref().map(validate_url).transpose()?;
            let browser = browser.unwrap_or_else(|| {
                PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome")
            });
            if !browser.is_file() {
                bail!("browser executable not found: {}", browser.display());
            }
            let lock = store.lock_session(&id)?;
            let generation = store.reserve(&id)?;
            drop(lock);
            let mut supervisor = Command::new(std::env::current_exe()?);
            supervisor
                .arg("--data-dir")
                .arg(&root)
                .arg("supervise")
                .arg(&id)
                .arg(&generation)
                .arg(&browser);
            if let Some(url) = &url {
                supervisor.arg("--url").arg(url);
            }
            if cdp {
                supervisor.arg("--cdp");
            }
            let result = supervisor
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            let mut supervisor = match result {
                Ok(child) => child,
                Err(error) => {
                    store.transition(&id, &generation, "starting", "failed")?;
                    return Err(error).context("start session supervisor");
                }
            };
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let session = store.get(&id)?;
                if session.generation.as_deref() != Some(&generation) {
                    bail!("session reservation changed during startup");
                }
                if let Some(status) = supervisor.try_wait()? {
                    if session.state == "starting" {
                        store.transition(&id, &generation, "starting", "failed")?;
                    }
                    bail!(
                        "session supervisor exited during startup ({status}); inspect before recovery"
                    );
                }
                if session.state == "running" {
                    println!("session {} running", id);
                    break;
                }
                if session.state != "starting" {
                    bail!("session did not start: {}", session.state);
                }
                if Instant::now() >= deadline {
                    bail!("supervisor did not report ready; inspect session state before retrying");
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
        Action::Endpoint { id } => println!("{}", store.cdp_endpoint(&id)?),
        Action::WaitAuth { id, origin } => {
            let session = store.get(&id)?;
            store.require_supervisor(&id)?;
            let generation = session.generation.context("session has no generation")?;
            let request_id = store.request_handoff(&id, &generation, origin.as_deref())?;
            println!("{id}\tawaiting_user\t{request_id}");
        }
        Action::Handoff { id } => {
            store.require_supervisor(&id)?;
            let (request_id, origin) = store
                .pending_handoff(&id)?
                .context("no pending authentication request")?;
            println!(
                "{id}\tawaiting_user\t{request_id}\t{}",
                origin.as_deref().unwrap_or("origin not provided")
            );
        }
        Action::Resume { id, request_id } => {
            let session = store.get(&id)?;
            store.require_supervisor(&id)?;
            let generation = session.generation.context("session has no generation")?;
            store.resume_handoff(&id, &generation, &request_id)?;
            println!("{id}\trunning");
        }
        Action::Stop { id } => {
            let session = store.get(&id)?;
            if !matches!(session.state.as_str(), "running" | "awaiting_user") {
                bail!("session cannot be stopped from {}", session.state);
            }
            store.require_supervisor(&id)?;
            let generation = session.generation.context("session has no generation")?;
            if !store.transition(&id, &generation, &session.state, "stopping")? {
                bail!("session state changed; retry");
            }
            println!("stop requested; close the browser window, then use list to check completion");
        }
        Action::Recover {
            id,
            confirm_browser_closed,
        } => {
            if !confirm_browser_closed {
                bail!(
                    "confirm that all browser processes using this session profile have exited with --confirm-browser-closed"
                );
            }
            store.recover(&id)?;
            println!("{id}\tstopped");
        }
        Action::Supervise {
            id,
            generation,
            browser,
            url,
            cdp,
        } => supervise(&store, &id, &generation, &browser, url.as_deref(), cdp)?,
    }
    Ok(())
}

fn show(store: &Store, session: keyharbor::Session) -> Result<()> {
    let mut state = session.state;
    if matches!(state.as_str(), "running" | "awaiting_user" | "stopping")
        && !store.supervisor_active(&session.id)?
    {
        state = store.get(&session.id)?.state;
        if matches!(state.as_str(), "running" | "awaiting_user" | "stopping") {
            state = "recovery_required".to_owned();
        }
    }
    println!("{}\t{}\t{}", session.id, session.name, state);
    Ok(())
}

fn supervise(
    store: &Store,
    id: &str,
    generation: &str,
    browser: &std::path::Path,
    url: Option<&str>,
    cdp: bool,
) -> Result<()> {
    let _lock = store.lock_session(id)?;
    let session = store.get(id)?;
    if session.state != "starting" || session.generation.as_deref() != Some(generation) {
        bail!("session reservation is no longer valid");
    }
    let profile = match store.prepare_profile(id) {
        Ok(profile) => profile,
        Err(error) => {
            store.transition(id, generation, "starting", "failed")?;
            return Err(error);
        }
    };
    if cdp {
        let prepare = || -> Result<()> {
            let port_file = profile.join("DevToolsActivePort");
            match fs::symlink_metadata(&port_file) {
                Ok(meta) if meta.is_file() => fs::remove_file(port_file)?,
                Ok(_) => bail!("unexpected debug port file type"),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
            let marker = store.cdp_marker(id, generation)?;
            fs::create_dir_all(marker.parent().context("invalid marker path")?)?;
            fs::write(marker, b"enabled")?;
            Ok(())
        };
        if let Err(error) = prepare() {
            store.transition(id, generation, "starting", "failed")?;
            return Err(error);
        }
    }
    let url = url.map(validate_url).transpose()?;
    let mut browser_command = Command::new(browser);
    browser_command
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg("--no-first-run")
        .arg("--no-default-browser-check");
    if cdp {
        browser_command.arg("--remote-debugging-port=0");
    }
    if let Some(url) = url {
        browser_command.arg(url);
    }
    let child = browser_command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) => {
            store.transition(id, generation, "starting", "failed")?;
            return Err(error.into());
        }
    };
    mark_running_or_stop(&mut child, store, id, generation)?;
    loop {
        if let Some(status) = child.try_wait()? {
            let session = store.get(id)?;
            if session.generation.as_deref() == Some(generation) {
                let next = if status.success() || session.state == "stopping" {
                    "stopped"
                } else {
                    "failed"
                };
                store.transition(id, generation, &session.state, next)?;
            }
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn mark_running_or_stop(
    child: &mut Child,
    store: &Store,
    id: &str,
    generation: &str,
) -> Result<()> {
    let transition = store.transition(id, generation, "starting", "running");
    if matches!(transition, Ok(true)) {
        return Ok(());
    }
    if child
        .try_wait()
        .context("inspect browser after startup failure")?
        .is_none()
    {
        if let Err(error) = child.kill()
            && child.try_wait()?.is_none()
        {
            return Err(error).context(
                "stop browser after startup failure; inspect the profile before recovery",
            );
        }
        child.wait().context(
            "wait for browser after startup failure; inspect the profile before recovery",
        )?;
    }
    match transition {
        Err(error) => Err(error).context("record browser startup; browser process stopped"),
        Ok(_) => bail!("session reservation changed; browser process stopped"),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn startup_database_failure_stops_launched_child() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        let store = Store::open(&root).unwrap();
        let session = store.create("startup-failure").unwrap();
        let generation = store.reserve(&session.id).unwrap();
        let conn = rusqlite::Connection::open(root.join("keyharbor.sqlite3")).unwrap();
        conn.execute_batch("CREATE TRIGGER fail_running BEFORE UPDATE ON sessions WHEN NEW.state='running' BEGIN SELECT RAISE(ABORT, 'synthetic failure'); END;").unwrap();
        let mut child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
        assert!(mark_running_or_stop(&mut child, &store, &session.id, &generation).is_err());
        assert!(child.try_wait().unwrap().is_some());
        assert_eq!(store.get(&session.id).unwrap().state, "starting");
    }
}
