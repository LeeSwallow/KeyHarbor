//! Files the vault commands read and write: export files are created new and private (0600)
//! and never overwrite anything (07 §7.7). Core works on bytes only (#19).

use anyhow::{Context, Result};
use std::{io::Write, path::Path};

/// Creates `path` with mode 0600 and writes `bytes`. Fails if it exists.
pub fn write_new_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options
        .open(path)
        .with_context(|| format!("cannot create {} (it must not exist yet)", path.display()))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

pub fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_files_are_private_and_never_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let out = tmp.path().join("export.age");
        write_new_private(&out, b"first").unwrap();
        assert!(write_new_private(&out, b"second").is_err());
        assert_eq!(read(&out).unwrap(), b"first");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&out).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}
