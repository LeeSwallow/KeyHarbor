//! Where a session's files live under the data directory.
//!
//! `profiles/<id>/` (Chrome user-data-dir), `run/<id>.lock` (supervisor lock),
//! `run/<id>.sock` (control socket), `run/mcp-<id>.lock` (scoped MCP server), `run/shots/`.

use anyhow::{Result, bail};
use keyharbor_domain::SessionId;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn profile_dir(data: &Path, id: &SessionId) -> PathBuf {
    data.join("profiles").join(id.to_string())
}
pub fn run_dir(data: &Path) -> PathBuf {
    data.join("run")
}
pub fn lock_path(data: &Path, id: &SessionId) -> PathBuf {
    run_dir(data).join(format!("{id}.lock"))
}
pub fn mcp_lock_path(data: &Path, id: &SessionId) -> PathBuf {
    run_dir(data).join(format!("mcp-{id}.lock"))
}
pub fn socket_path(data: &Path, id: &SessionId) -> PathBuf {
    run_dir(data).join(format!("{id}.sock"))
}
pub fn shots_dir(data: &Path) -> PathBuf {
    run_dir(data).join("shots")
}

/// Creates `path` as a 0700 directory, or accepts an existing real directory.
/// A symbolic link or a file in its place is an error, and nothing is replaced.
pub fn private_dir(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error.into()),
    }
    if !fs::symlink_metadata(path)?.is_dir() {
        bail!(
            "{} must be a directory and not a symbolic link",
            path.display()
        );
    }
    Ok(())
}

/// Unix socket addresses are limited to 104 bytes on macOS (108 on Linux), terminator included.
pub const MAX_SOCKET_PATH: usize = 103;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layout_and_private_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let id = keyharbor_crypto::random::id::<SessionId>();
        assert_eq!(
            socket_path(tmp.path(), &id),
            tmp.path().join("run").join(format!("{id}.sock"))
        );
        let dir = tmp.path().join("p");
        private_dir(&dir).unwrap();
        private_dir(&dir).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
            let link = tmp.path().join("link");
            std::os::unix::fs::symlink(&dir, &link).unwrap();
            assert!(private_dir(&link).is_err());
        }
        fs::write(tmp.path().join("file"), "x").unwrap();
        assert!(private_dir(&tmp.path().join("file")).is_err());
    }
}
