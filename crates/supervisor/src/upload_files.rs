//! What `browser_upload` needs from this Mac's file system: the person's allowed folders
//! (`upload_dirs`, `~/` meaning the home folder) and the files the agent named, each with its
//! symbolic links followed so the domain rule compares real places (03 §3.14).

use anyhow::{Context, Result};
use keyharbor_core::settings;
use keyharbor_domain::{
    ports::SettingsStore,
    settings::{UPLOAD_DIRS, UPLOAD_MAX_FILES},
    upload::{UploadFile, UploadPolicy, parse_dirs},
};
use std::path::{Path, PathBuf};

/// The rule from the settings, with folders that exist resolved (missing ones dropped).
pub(crate) fn policy(store: &dyn SettingsStore, home: &Path) -> Result<UploadPolicy> {
    let dirs = parse_dirs(&settings::get(store, UPLOAD_DIRS)?, home)
        .map_err(|e| anyhow::anyhow!("{UPLOAD_DIRS} {e}"))?
        .into_iter()
        .filter_map(|dir| dir.canonicalize().ok())
        .collect();
    let max_files = settings::get(store, UPLOAD_MAX_FILES)?
        .parse()
        .context("invalid upload_max_files")?;
    Ok(UploadPolicy { dirs, max_files })
}

/// Each named file as the file system has it. A path that does not resolve is kept as
/// given and is not a regular file, so the rule refuses it by name.
pub(crate) fn resolve(asked: &[String]) -> Vec<UploadFile> {
    asked
        .iter()
        .map(|path| match PathBuf::from(path).canonicalize() {
            Ok(resolved) => UploadFile {
                is_file: resolved.is_file(),
                asked: path.clone(),
                resolved,
            },
            Err(_) => UploadFile {
                asked: path.clone(),
                resolved: PathBuf::from(path),
                is_file: false,
            },
        })
        .collect()
}

/// The person's home folder, for `~/` in `upload_dirs`.
pub(crate) fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_resolves_to_where_it_points() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tmp.path().join("secret.txt");
        std::fs::write(&outside, "x").unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let link = allowed.join("resume.pdf");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let files = resolve(&[link.to_string_lossy().into_owned()]);
        assert_eq!(files[0].resolved, outside.canonicalize().unwrap());
        let policy = UploadPolicy {
            dirs: vec![allowed.canonicalize().unwrap()],
            max_files: 10,
        };
        assert!(policy.check(&files).is_err(), "the link points outside");
    }

    #[test]
    fn a_missing_file_is_not_a_file() {
        let files = resolve(&["/no/such/file.pdf".to_owned()]);
        assert!(!files[0].is_file);
    }
}
