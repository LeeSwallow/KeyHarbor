//! Before the browser starts: the private profile, screenshot and socket paths, and the
//! control socket listener.

use crate::paths;
use anyhow::{Context, Result, bail};
use keyharbor_broker::chrome::DEVTOOLS_ACTIVE_PORT;
use keyharbor_domain::SessionId;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tokio::net::UnixListener;

/// The paths one run uses, created privately.
pub(super) struct Prepared {
    pub profile: PathBuf,
    pub socket: PathBuf,
    pub shots: PathBuf,
}

pub(super) fn prepare(
    data: &Path,
    id: &SessionId,
    cdp: bool,
    profile_needed: bool,
) -> Result<Prepared> {
    let profile = paths::profile_dir(data, id);
    // ego lite runs on the person's own ego profile (ADR-0018): no profile of ours.
    if profile_needed {
        paths::private_dir(profile.parent().context("invalid profile path")?)?;
        paths::private_dir(&profile)?;
    }
    if cdp {
        let port_file = profile.join(DEVTOOLS_ACTIVE_PORT);
        match fs::symlink_metadata(&port_file) {
            Ok(meta) if meta.is_file() => fs::remove_file(port_file)?,
            Ok(_) => bail!("unexpected DevToolsActivePort file type"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    paths::check_socket_path(data, id)?;
    let socket = paths::socket_path(data, id);
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
    let shots = paths::shots_dir(data);
    paths::private_dir(shots.parent().context("invalid shots path")?)?;
    paths::private_dir(&shots)?;
    Ok(Prepared {
        profile,
        socket,
        shots,
    })
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
