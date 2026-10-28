//! The per-session supervisor process: holds the OS lock, runs Chrome with the CDP pipe,
//! serves the control socket and records the session state when the browser exits.

use crate::{control, ops::Ops, paths};
use anyhow::{Context, Result, bail};
use keyharbor_domain::{Generation, SessionId, State};
use keyharbor_sqlite::SqliteStore;
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{net::UnixListener, process::Child, time::Instant};

/// How long the browser gets to exit after `close` before it is killed.
const CLOSE_GRACE: Duration = Duration::from_secs(10);

/// Takes the exclusive lock at `path`, creating it. Fails at once if another process holds it.
pub fn acquire_lock(path: &Path) -> Result<File> {
    paths::private_dir(path.parent().context("invalid lock path")?)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => bail!("lock is held by another process"),
        Err(TryLockError::Error(error)) => Err(error.into()),
    }
}

/// True while some process holds `path` locked.
pub fn is_locked(path: &Path) -> bool {
    match File::open(path) {
        Ok(file) => matches!(file.try_lock(), Err(TryLockError::WouldBlock)),
        Err(_) => false,
    }
}

/// True while a supervisor holds this session's lock.
pub fn is_alive(data: &Path, id: &SessionId) -> bool {
    is_locked(&paths::lock_path(data, id))
}

/// What the supervisor shares with its control connections.
pub(crate) struct Shared {
    pub id: SessionId,
    pub generation: Generation,
    pub store: Mutex<SqliteStore>,
    /// Requests run one at a time (02 §2.6.1).
    pub ops: tokio::sync::Mutex<Ops>,
    pub close: tokio::sync::Notify,
    pub close_requested: std::sync::atomic::AtomicBool,
}

impl Shared {
    pub fn request_close(&self) {
        self.close_requested
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.close.notify_one();
    }
}

/// Runs the supervisor until the browser exits. The session must be `starting` under `generation`.
pub fn run(
    data: &Path,
    id: SessionId,
    generation: Generation,
    browser: &Path,
    url: Option<&str>,
    cdp: bool,
) -> Result<()> {
    let _lock = acquire_lock(&paths::lock_path(data, &id))
        .context("another supervisor is running this session")?;
    let store = SqliteStore::open(data)?;
    let row = store.get(id)?.context("session not found")?;
    if row.state != State::Starting || row.generation != Some(generation) {
        bail!("session reservation is no longer valid");
    }
    let fail = |store: &SqliteStore, error: anyhow::Error| {
        let _ = store.transition(id, generation, State::Starting, State::Failed);
        error
    };
    let url = match url.map(keyharbor_domain::validate_start_url).transpose() {
        Ok(url) => url,
        Err(error) => return Err(fail(&store, anyhow::anyhow!("{error}"))),
    };
    let (profile, socket) = match prepare(data, &id, cdp) {
        Ok(paths) => paths,
        Err(error) => return Err(fail(&store, error)),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let launch = Launch {
        browser: browser.to_path_buf(),
        profile,
        url,
        cdp,
    };
    runtime.block_on(supervise(data, id, generation, store, launch, socket))
}

struct Launch {
    browser: PathBuf,
    profile: PathBuf,
    url: Option<String>,
    cdp: bool,
}

fn prepare(data: &Path, id: &SessionId, cdp: bool) -> Result<(PathBuf, PathBuf)> {
    let profile = paths::profile_dir(data, id);
    paths::private_dir(profile.parent().context("invalid profile path")?)?;
    paths::private_dir(&profile)?;
    if cdp {
        let port_file = profile.join("DevToolsActivePort");
        match fs::symlink_metadata(&port_file) {
            Ok(meta) if meta.is_file() => fs::remove_file(port_file)?,
            Ok(_) => bail!("unexpected DevToolsActivePort file type"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    let socket = paths::socket_path(data, id);
    if socket.as_os_str().len() > paths::MAX_SOCKET_PATH {
        bail!(
            "control socket path is longer than {} bytes; use a shorter --data-dir",
            paths::MAX_SOCKET_PATH
        );
    }
    // We hold the lock, so a socket file left here belongs to a dead supervisor.
    match fs::symlink_metadata(&socket) {
        Ok(meta) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::FileTypeExt;
                if !meta.file_type().is_socket() {
                    bail!("{} exists and is not a socket", socket.display());
                }
            }
            #[cfg(not(unix))]
            let _ = meta;
            fs::remove_file(&socket)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    Ok((profile, socket))
}

/// Starts Chrome with the CDP pipe on fds 3 (to Chrome) and 4 (from Chrome).
fn spawn_browser(
    launch: &Launch,
) -> Result<(
    Child,
    tokio::net::unix::pipe::Sender,
    tokio::net::unix::pipe::Receiver,
)> {
    use std::os::fd::{AsRawFd, OwnedFd};
    let (to_chrome_read, to_chrome_write) = std::io::pipe()?;
    let (from_chrome_read, from_chrome_write) = std::io::pipe()?;
    let mut command = tokio::process::Command::new(&launch.browser);
    command
        .arg(format!("--user-data-dir={}", launch.profile.display()))
        .arg("--remote-debugging-pipe")
        .arg("--no-first-run")
        .arg("--no-default-browser-check");
    if std::env::var_os("KEYHARBOR_BROWSER_HEADLESS").is_some_and(|v| v == "1") {
        command.arg("--headless=new");
    }
    if launch.cdp {
        command.arg("--remote-debugging-port=0");
    }
    if let Some(url) = &launch.url {
        command.arg(url);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let (read_fd, write_fd) = (to_chrome_read.as_raw_fd(), from_chrome_write.as_raw_fd());
    // SAFETY: only async-signal-safe calls (fcntl, dup2) run between fork and exec.
    unsafe {
        command.pre_exec(move || {
            // Copy above 4 first so dup2 never sees source == target (which would keep CLOEXEC).
            let read_copy = libc::fcntl(read_fd, libc::F_DUPFD_CLOEXEC, 10);
            let write_copy = libc::fcntl(write_fd, libc::F_DUPFD_CLOEXEC, 10);
            if read_copy < 0
                || write_copy < 0
                || libc::dup2(read_copy, 3) < 0
                || libc::dup2(write_copy, 4) < 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = command
        .spawn()
        .with_context(|| format!("start browser {}", launch.browser.display()))?;
    drop((to_chrome_read, from_chrome_write));
    let sender = tokio::net::unix::pipe::Sender::from_owned_fd(OwnedFd::from(to_chrome_write))?;
    let receiver =
        tokio::net::unix::pipe::Receiver::from_owned_fd(OwnedFd::from(from_chrome_read))?;
    Ok((child, sender, receiver))
}

async fn supervise(
    data: &Path,
    id: SessionId,
    generation: Generation,
    store: SqliteStore,
    launch: Launch,
    socket: PathBuf,
) -> Result<()> {
    let fail = |store: &SqliteStore, error: anyhow::Error| {
        let _ = store.transition(id, generation, State::Starting, State::Failed);
        error
    };
    let listener = match bind(&socket) {
        Ok(listener) => listener,
        Err(error) => return Err(fail(&store, error)),
    };
    let (mut child, sender, receiver) = match spawn_browser(&launch) {
        Ok(spawned) => spawned,
        Err(error) => {
            let _ = fs::remove_file(&socket);
            return Err(fail(&store, error));
        }
    };
    match store.transition(id, generation, State::Starting, State::Running) {
        Ok(true) => (),
        outcome => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            let _ = fs::remove_file(&socket);
            return match outcome {
                Err(error) => Err(error.context("record browser start; browser stopped")),
                Ok(_) => bail!("session reservation changed; browser stopped"),
            };
        }
    }
    let _ = data;
    let shared = Arc::new(Shared {
        id,
        generation,
        store: Mutex::new(store),
        ops: tokio::sync::Mutex::new(Ops::new(sender, receiver)),
        close: tokio::sync::Notify::new(),
        close_requested: false.into(),
    });
    let status = serve(&shared, &listener, &mut child).await;
    drop(listener);
    let _ = fs::remove_file(&socket);
    let status = status?;
    record_exit(&shared, status).await;
    Ok(())
}

fn bind(socket: &Path) -> Result<UnixListener> {
    let listener = UnixListener::bind(socket).context("open control socket")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(socket, fs::Permissions::from_mode(0o600))?;
    }
    Ok(listener)
}

/// Accepts control connections until the browser exits; kills it if `close` is not honoured.
async fn serve(
    shared: &Arc<Shared>,
    listener: &UnixListener,
    child: &mut Child,
) -> Result<ExitStatus> {
    let mut kill_at: Option<Instant> = None;
    loop {
        let deadline = async {
            match kill_at {
                Some(at) => tokio::time::sleep_until(at).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            status = child.wait() => return Ok(status?),
            accepted = listener.accept() => {
                if let Ok((stream, _)) = accepted {
                    tokio::spawn(control::serve_connection(stream, shared.clone()));
                }
            }
            () = shared.close.notified(), if kill_at.is_none() => {
                kill_at = Some(Instant::now() + CLOSE_GRACE);
            }
            () = deadline => {
                let _ = child.start_kill();
                kill_at = Some(Instant::now() + Duration::from_secs(3600));
            }
        }
    }
}

/// Records `stopped` (clean exit or requested close) or `failed`, retrying through brief DB errors.
async fn record_exit(shared: &Shared, status: ExitStatus) {
    let requested = shared
        .close_requested
        .load(std::sync::atomic::Ordering::SeqCst);
    for _ in 0..40 {
        let outcome = (|| -> Result<bool> {
            let store = shared
                .store
                .lock()
                .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
            let Some(row) = store.get(shared.id)? else {
                return Ok(true);
            };
            if row.generation != Some(shared.generation) {
                return Ok(true);
            }
            if !matches!(
                row.state,
                State::Running | State::AwaitingUser | State::Stopping
            ) {
                return Ok(true);
            }
            let next = if status.success() || requested || row.state == State::Stopping {
                State::Stopped
            } else {
                State::Failed
            };
            store.transition(shared.id, shared.generation, row.state, next)
        })();
        if matches!(outcome, Ok(true)) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}
