//! `install-host`: registers the Native Messaging host with the user's browsers. A browser
//! starts the program a manifest names with no arguments of ours, so the manifest names a
//! small script in the data directory that runs this binary as `browser host` for this data
//! directory. Only our extension may start it (`allowed_origins` / `allowed_extensions`).

use super::HostBrowser;
use crate::compose::App;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

/// The host's registered name (`HOST_NAME` in the extension's protocol.ts).
pub const HOST_NAME: &str = "dev.keyharbor.host";
/// The unpacked Chrome extension's fixed ID (its manifest `key`).
pub const CHROME_EXTENSION: &str = "chrome-extension://fimjcgdlhnkdmjnhcccnkpcnmcecgjoc/";
/// The Firefox add-on ID.
pub const FIREFOX_EXTENSION: &str = "browser-extension@keyharbor.dev";
const WRAPPER: &str = "native-host";

/// Where each browser looks for Native Messaging manifests, under the home directory.
fn manifest_dir(home: &Path, browser: HostBrowser) -> PathBuf {
    let relative = if cfg!(target_os = "macos") {
        match browser {
            HostBrowser::Chrome => "Library/Application Support/Google/Chrome/NativeMessagingHosts",
            HostBrowser::Chromium => "Library/Application Support/Chromium/NativeMessagingHosts",
            _ => "Library/Application Support/Mozilla/NativeMessagingHosts",
        }
    } else {
        match browser {
            HostBrowser::Chrome => ".config/google-chrome/NativeMessagingHosts",
            HostBrowser::Chromium => ".config/chromium/NativeMessagingHosts",
            _ => ".mozilla/native-messaging-hosts",
        }
    };
    home.join(relative)
}

/// The manifest for `browser`, starting `wrapper`.
fn manifest(browser: HostBrowser, wrapper: &Path) -> Value {
    let mut manifest = json!({
        "name": HOST_NAME,
        "description": "KeyHarbor native host: receives one site login for an agent session (R-42)",
        "path": wrapper,
        "type": "stdio",
    });
    if browser == HostBrowser::Firefox {
        manifest["allowed_extensions"] = json!([FIREFOX_EXTENSION]);
    } else {
        manifest["allowed_origins"] = json!([CHROME_EXTENSION]);
    }
    manifest
}

/// Single-quotes `value` for `sh`.
fn quoted(value: &Path) -> Result<String> {
    let text = value.to_str().context("path is not UTF-8")?;
    Ok(format!("'{}'", text.replace('\'', r"'\''")))
}

/// The script a manifest names: this binary, for this data directory.
fn wrapper(exe: &Path, data: &Path) -> Result<String> {
    Ok(format!(
        "#!/bin/sh\nexec {} --data-dir {} browser host \"$@\"\n",
        quoted(exe)?,
        quoted(data)?
    ))
}

pub fn install(app: &App, browser: HostBrowser) -> Result<()> {
    let home = PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?);
    let exe = std::env::current_exe()?;
    fs::create_dir_all(app.root())?;
    let script = app.root().join(WRAPPER);
    fs::write(&script, wrapper(&exe, app.root())?)?;
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700))?;
    let browsers = match browser {
        HostBrowser::All => vec![
            HostBrowser::Chrome,
            HostBrowser::Chromium,
            HostBrowser::Firefox,
        ],
        one => vec![one],
    };
    for browser in browsers {
        let dir = manifest_dir(&home, browser);
        fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{HOST_NAME}.json"));
        fs::write(
            &path,
            serde_json::to_string_pretty(&manifest(browser, &script))?,
        )?;
        println!("{}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wrapper_quotes_its_paths() {
        let script = wrapper(
            Path::new("/opt/kh/keyharbor"),
            Path::new("/Users/o'neil/data"),
        )
        .unwrap();
        assert_eq!(
            script,
            "#!/bin/sh\nexec '/opt/kh/keyharbor' --data-dir '/Users/o'\\''neil/data' browser host \"$@\"\n"
        );
    }

    #[test]
    fn only_our_extension_may_start_the_host() {
        let chrome = manifest(HostBrowser::Chrome, Path::new("/x"));
        assert_eq!(chrome["allowed_origins"], json!([CHROME_EXTENSION]));
        assert!(chrome.get("allowed_extensions").is_none());
        let firefox = manifest(HostBrowser::Firefox, Path::new("/x"));
        assert_eq!(firefox["allowed_extensions"], json!([FIREFOX_EXTENSION]));
        assert_eq!(firefox["name"], HOST_NAME);
    }
}
