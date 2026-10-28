//! Starting the browser: the profile and socket paths, Chrome with the CDP pipe on fds 3/4,
//! and the control socket listener.

use crate::paths;
use anyhow::{Context, Result, bail};
use keyharbor_domain::SessionId;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::{net::UnixListener, process::Child};

pub(super) struct Launch {
    pub browser: PathBuf,
    pub profile: PathBuf,
    pub url: Option<String>,
    pub cdp: bool,
}

pub(super) fn prepare(data: &Path, id: &SessionId, cdp: bool) -> Result<(PathBuf, PathBuf)> {
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
pub(super) fn spawn_browser(
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

pub(super) fn bind(socket: &Path) -> Result<UnixListener> {
    let listener = UnixListener::bind(socket).context("open control socket")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(socket, fs::Permissions::from_mode(0o600))?;
    }
    Ok(listener)
}
