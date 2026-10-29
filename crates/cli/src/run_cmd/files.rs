//! Values handed over as files (channel ②, developer secrets spec §3): each in a 0600 file in
//! a private folder made for this run, removed when the run ends, however it ends.

use anyhow::{Context, Result};
use std::{
    fs::OpenOptions,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::PathBuf,
};
use zeroize::Zeroizing;

/// `(variable, value)` pairs for the child's environment.
pub(super) type Vars = Vec<(String, Zeroizing<String>)>;

/// The run's private folder (0700); dropping it removes it and every file in it.
pub(super) struct SecretFiles {
    /// Held only so that dropping `SecretFiles` removes the folder.
    _dir: tempfile::TempDir,
}

impl SecretFiles {
    /// Writes each `(variable, contents)` to its own file and returns `(variable, path)`.
    pub(super) fn write(files: &[(String, Zeroizing<String>)]) -> Result<(Self, Vars)> {
        let dir = tempfile::Builder::new()
            .prefix("keyharbor-run-")
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .context("could not make a private folder for the files")?;
        let mut paths = Vec::new();
        for (index, (var, contents)) in files.iter().enumerate() {
            let path: PathBuf = dir
                .path()
                .join(format!("{index}-{}", var.to_ascii_lowercase()));
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
                .with_context(|| format!("could not write the file for {var}"))?;
            file.write_all(contents.as_bytes())?;
            let path = path
                .to_str()
                .context("temporary folder path is not UTF-8")?;
            paths.push((var.clone(), Zeroizing::new(path.to_owned())));
        }
        Ok((Self { _dir: dir }, paths))
    }

    #[cfg(test)]
    fn path(&self) -> &std::path::Path {
        self._dir.path()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_are_private_and_gone_with_the_run() {
        let files = [(
            "GOOGLE_APPLICATION_CREDENTIALS".to_owned(),
            Zeroizing::new("{}".to_owned()),
        )];
        let (held, paths) = SecretFiles::write(&files).unwrap();
        let dir = held.path().to_path_buf();
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        let file = PathBuf::from(paths[0].1.as_str());
        assert_eq!(mode(&file), 0o600);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{}");
        drop(held);
        assert!(!dir.exists());
    }
}
