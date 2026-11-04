//! Which local files an agent may put into a page's file input (`browser_upload`, 03 §3.14):
//! regular files inside a folder the person allowed (`upload_dirs`), a few at a time. The
//! paths here are already resolved (symbolic links followed) by the adapter that reads the
//! file system; this module only decides.

use std::{
    fmt,
    path::{Path, PathBuf},
};

/// The separator between folders in the `upload_dirs` setting, as in `PATH`.
pub const DIR_SEPARATOR: char = ':';

/// A file the agent named, as the file system resolved it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UploadFile {
    /// What the agent asked for, for messages.
    pub asked: String,
    /// The path with every symbolic link followed.
    pub resolved: PathBuf,
    /// A regular file (not a folder, device or socket).
    pub is_file: bool,
}

impl UploadFile {
    /// The name the usage record keeps: the last part of the path, never its contents.
    pub fn name(&self) -> String {
        self.resolved
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// The person's rule for uploads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UploadPolicy {
    /// Allowed folders, resolved like the files.
    pub dirs: Vec<PathBuf>,
    pub max_files: usize,
}

/// Why an upload is refused. Messages name the file the agent gave, never its contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UploadRefusal {
    NoAllowedDirs,
    NoFiles,
    TooMany { max: usize },
    NotAFile(String),
    OutsideAllowedDirs(String),
}

impl fmt::Display for UploadRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAllowedDirs => f.write_str(
                "upload_dirs is not set; a person can allow a folder with `keyharbor settings set upload_dirs <folder>`",
            ),
            Self::NoFiles => f.write_str("give at least one file"),
            Self::TooMany { max } => write!(f, "at most {max} files at a time (upload_max_files)"),
            Self::NotAFile(asked) => write!(f, "{asked} is not a regular file"),
            Self::OutsideAllowedDirs(asked) => write!(f, "{asked} is outside upload_dirs"),
        }
    }
}

impl std::error::Error for UploadRefusal {}

impl UploadPolicy {
    /// `Ok` if every file may be uploaded.
    pub fn check(&self, files: &[UploadFile]) -> Result<(), UploadRefusal> {
        if self.dirs.is_empty() {
            return Err(UploadRefusal::NoAllowedDirs);
        }
        if files.is_empty() {
            return Err(UploadRefusal::NoFiles);
        }
        if files.len() > self.max_files {
            return Err(UploadRefusal::TooMany {
                max: self.max_files,
            });
        }
        for file in files {
            if !file.is_file {
                return Err(UploadRefusal::NotAFile(file.asked.clone()));
            }
            if !self.dirs.iter().any(|dir| file.resolved.starts_with(dir)) {
                return Err(UploadRefusal::OutsideAllowedDirs(file.asked.clone()));
            }
        }
        Ok(())
    }
}

/// The folders an `upload_dirs` value names, with a leading `~/` meaning `home`. Empty parts
/// are skipped; every other part must be absolute.
pub fn parse_dirs(value: &str, home: &Path) -> Result<Vec<PathBuf>, String> {
    value
        .split(DIR_SEPARATOR)
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            let path = match part.strip_prefix("~/") {
                Some(rest) => home.join(rest),
                None if part == "~" => home.to_path_buf(),
                None => PathBuf::from(part),
            };
            if path.is_absolute() {
                Ok(path)
            } else {
                Err(format!("{part} is not an absolute folder"))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, is_file: bool) -> UploadFile {
        UploadFile {
            asked: path.to_owned(),
            resolved: PathBuf::from(path),
            is_file,
        }
    }
    fn policy(dirs: &[&str]) -> UploadPolicy {
        UploadPolicy {
            dirs: dirs.iter().map(PathBuf::from).collect(),
            max_files: 2,
        }
    }

    #[test]
    fn empty_upload_dirs_refuse() {
        assert_eq!(
            policy(&[]).check(&[file("/a/x.pdf", true)]),
            Err(UploadRefusal::NoAllowedDirs)
        );
    }

    #[test]
    fn file_inside_allowed_dir_passes() {
        assert_eq!(policy(&["/a"]).check(&[file("/a/b/x.pdf", true)]), Ok(()));
    }

    #[test]
    fn symlink_out_of_allowed_dir_is_refused() {
        // The adapter resolved /a/link.pdf to /etc/passwd.
        let linked = UploadFile {
            asked: "/a/link.pdf".into(),
            resolved: PathBuf::from("/etc/passwd"),
            is_file: true,
        };
        assert_eq!(
            policy(&["/a"]).check(&[linked]),
            Err(UploadRefusal::OutsideAllowedDirs("/a/link.pdf".into()))
        );
        // A sibling whose name starts like the folder is outside too.
        assert!(policy(&["/a"]).check(&[file("/ab/x.pdf", true)]).is_err());
    }

    #[test]
    fn directory_is_refused() {
        assert_eq!(
            policy(&["/a"]).check(&[file("/a/b", false)]),
            Err(UploadRefusal::NotAFile("/a/b".into()))
        );
    }

    #[test]
    fn more_than_max_files_is_refused() {
        let three = [file("/a/1", true), file("/a/2", true), file("/a/3", true)];
        assert_eq!(
            policy(&["/a"]).check(&three),
            Err(UploadRefusal::TooMany { max: 2 })
        );
        assert_eq!(policy(&["/a"]).check(&[]), Err(UploadRefusal::NoFiles));
    }

    #[test]
    fn dirs_expand_home_and_must_be_absolute() {
        let home = Path::new("/Users/me");
        assert_eq!(
            parse_dirs("~/Documents/Recruiting: /tmp/x ::", home).unwrap(),
            vec![
                PathBuf::from("/Users/me/Documents/Recruiting"),
                PathBuf::from("/tmp/x")
            ]
        );
        assert!(parse_dirs("relative/dir", home).is_err());
        assert!(parse_dirs("", home).unwrap().is_empty());
    }
}
