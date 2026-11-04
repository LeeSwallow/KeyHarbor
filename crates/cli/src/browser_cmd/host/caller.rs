//! Who started the host. A browser starts it for its extension; an agent with a shell can
//! start it too (`keyharbor browser host`), so the one reply that carries a value
//! (`autofill_fill`) is given only when the parent process is a browser's own executable
//! (ADR-0017, 03 §3.12.2). A weak boundary like the terminal check: a process of the same
//! user that runs a real browser with a look-alike extension is not stopped (N1).

use crate::compose::App;

/// Whether the host may give what only a browser gets: the bridge and `filled`. Test builds
/// with a fixed Touch ID answer trust any caller ([`App::trusts_any_caller`]).
pub(super) fn is_trusted(app: &App) -> bool {
    app.trusts_any_caller() || parent_path().is_some_and(|path| is_browser(&path))
}

/// The browsers whose app bundles may start the host, by bundle name.
const BROWSERS: &[&str] = &[
    "Google Chrome.app",
    "Google Chrome Beta.app",
    "Google Chrome Dev.app",
    "Google Chrome Canary.app",
    "Chromium.app",
    "Microsoft Edge.app",
    "Brave Browser.app",
    "ego lite.app",
    "Firefox.app",
    "Firefox Developer Edition.app",
    "Firefox Nightly.app",
];

/// Whether `path` is the executable inside one of [`BROWSERS`]
/// (`…/<Browser>.app/Contents/MacOS/<file>`).
fn is_browser(path: &str) -> bool {
    let mut parts = path.rsplit('/');
    let (Some(file), Some("MacOS"), Some("Contents"), Some(bundle)) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    !file.is_empty() && BROWSERS.contains(&bundle)
}

/// The executable path of this process's parent.
#[cfg(target_os = "macos")]
fn parent_path() -> Option<String> {
    let mut buffer = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: getppid has no preconditions; proc_pidpath writes at most `buffer.len()` bytes
    // into the buffer we own and returns how many it wrote.
    let written = unsafe {
        libc::proc_pidpath(
            libc::getppid(),
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
        )
    };
    let written = usize::try_from(written).ok().filter(|&n| n > 0)?;
    buffer.truncate(written);
    String::from_utf8(buffer).ok()
}

#[cfg(not(target_os = "macos"))]
fn parent_path() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_browsers_own_executable_counts() {
        for path in [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Users/me/Applications/Firefox.app/Contents/MacOS/firefox",
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
        ] {
            assert!(is_browser(path), "{path}");
        }
        for path in [
            "/bin/zsh",
            "/usr/bin/python3",
            "/Applications/Terminal.app/Contents/MacOS/Terminal",
            "/Applications/Google Chrome.app/Contents/Resources/fake",
            "/tmp/Google Chrome.app/Contents/MacOS/",
            "Google Chrome",
            "",
        ] {
            assert!(!is_browser(path), "{path}");
        }
    }

    #[test]
    fn ego_lite_is_a_browser_caller() {
        assert!(is_browser(
            "/Applications/ego lite.app/Contents/MacOS/ego lite"
        ));
        assert!(
            !is_browser("/Users/me/.local/bin/ego-browser"),
            "the agent CLI is not the browser"
        );
    }
}
