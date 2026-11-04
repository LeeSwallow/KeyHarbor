//! `vault import-env`, `import-chrome`, `export` and `import`: the files the vault commands
//! read and write. Export files are created new and private (0600) and never overwrite
//! anything (07 §7.7). Core works on bytes only (#19).

use super::input::{item_name, read_passphrase};
use crate::compose::App;
use anyhow::{Context, Result};
use std::{io::Write, path::Path};
use zeroize::Zeroizing;

pub fn import_env(app: &App, name: &str, path: &Path) -> Result<()> {
    let name = item_name(name)?;
    let text = read_text(path)?;
    let count = app.vault()?.import_env(name.clone(), &text)?;
    println!("imported\t{}\t{count} variables", name.as_str());
    Ok(())
}

pub fn import_chrome(app: &App, path: &Path) -> Result<()> {
    let text = read_text(path)?;
    let report = app.vault()?.import_chrome_csv(&text)?;
    println!(
        "imported\t{} logins\tskipped {} existing, {} without a web address",
        report.added, report.skipped_existing, report.skipped_not_web
    );
    Ok(())
}

pub fn export(app: &App, path: &Path, passphrase_stdin: bool) -> Result<()> {
    let vault = app.vault()?;
    let bytes = vault.export_age(read_passphrase(passphrase_stdin, true)?)?;
    write_new_private(path, &bytes)?;
    println!("exported\t{}", path.display());
    Ok(())
}

pub fn import(app: &App, path: &Path, passphrase_stdin: bool) -> Result<()> {
    let vault = app.vault()?;
    let encrypted = read(path)?;
    let report = vault.import_age(read_passphrase(passphrase_stdin, false)?, &encrypted)?;
    for name in &report.skipped_existing {
        eprintln!("skipped existing item {name}");
    }
    println!("imported\t{}", report.added);
    Ok(())
}

/// A text file that holds values (a .env file, a password export), zeroized on drop.
fn read_text(path: &Path) -> Result<Zeroizing<String>> {
    std::fs::read_to_string(path)
        .map(Zeroizing::new)
        .with_context(|| format!("cannot read {}", path.display()))
}

/// Creates `path` with mode 0600 and writes `bytes`. Fails if it exists.
fn write_new_private(path: &Path, bytes: &[u8]) -> Result<()> {
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

fn read(path: &Path) -> Result<Vec<u8>> {
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
