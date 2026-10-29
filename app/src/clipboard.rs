//! Copying (09 §9.9). The clipboard is readable by every process of the user, agents' `pbpaste`
//! included, so a copied value is marked `org.nspasteboard.ConcealedType` (clipboard managers
//! that respect it keep no history) and cleared after [`CLEAR_AFTER`] if nothing was copied
//! since. Whether something was copied since is told by the pasteboard's change count, so
//! clearing never reads the clipboard.

use anyhow::Result;
use std::time::Duration;

/// How long a copied value stays. Fixed for now (09 §9.4.7 shows it as a setting to come).
pub const CLEAR_AFTER: Duration = Duration::from_secs(30);

/// The system clipboard, as the app uses it.
pub trait Clipboard: Send + Sync {
    /// Puts `text` on the clipboard; `concealed` marks it as a secret. Returns a token that
    /// identifies this copy.
    fn copy(&self, text: &str, concealed: bool) -> Result<i64>;
    /// Empties the clipboard if `token`'s copy is still what it holds.
    fn clear_if_unchanged(&self, token: i64);
}

/// Copies a value and clears it after [`CLEAR_AFTER`] on a background thread.
pub fn copy_secret(clipboard: std::sync::Arc<dyn Clipboard>, text: &str) -> Result<()> {
    let token = clipboard.copy(text, true)?;
    std::thread::spawn(move || {
        std::thread::sleep(CLEAR_AFTER);
        clipboard.clear_if_unchanged(token);
    });
    Ok(())
}

#[cfg(target_os = "macos")]
pub use mac::MacClipboard;

#[cfg(target_os = "macos")]
mod mac {
    use super::Clipboard;
    use anyhow::{Result, bail};
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    use objc2_foundation::NSString;

    /// The marker type clipboard managers check (nspasteboard.org).
    const CONCEALED: &str = "org.nspasteboard.ConcealedType";

    /// `NSPasteboard.generalPasteboard`.
    pub struct MacClipboard;

    impl Clipboard for MacClipboard {
        fn copy(&self, text: &str, concealed: bool) -> Result<i64> {
            let board = NSPasteboard::generalPasteboard();
            board.clearContents();
            // SAFETY: NSPasteboardTypeString is a constant AppKit exports for this purpose.
            let string_type = unsafe { NSPasteboardTypeString };
            if !board.setString_forType(&NSString::from_str(text), string_type) {
                bail!("the clipboard refused the text");
            }
            if concealed {
                board.setString_forType(&NSString::from_str(""), &NSString::from_str(CONCEALED));
            }
            Ok(board.changeCount() as i64)
        }

        fn clear_if_unchanged(&self, token: i64) {
            let board = NSPasteboard::generalPasteboard();
            if board.changeCount() as i64 == token {
                board.clearContents();
            }
        }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::NSString;

    /// Uses the real system clipboard, so it runs only where asked (`cargo test -- --ignored`,
    /// the CI app job): it would overwrite a person's clipboard.
    #[test]
    #[ignore = "writes the real clipboard"]
    fn a_secret_copy_is_marked_concealed_and_cleared_only_while_unchanged() {
        let board = MacClipboard;
        let token = board.copy("kh-clipboard-test", true).unwrap();
        let general = NSPasteboard::generalPasteboard();
        let concealed =
            general.stringForType(&NSString::from_str("org.nspasteboard.ConcealedType"));
        assert!(concealed.is_some(), "the concealed marker is set");
        board.clear_if_unchanged(token + 1);
        // SAFETY: NSPasteboardTypeString is AppKit's constant for plain text.
        let text = general.stringForType(unsafe { objc2_app_kit::NSPasteboardTypeString });
        assert_eq!(
            text.map(|s| s.to_string()).as_deref(),
            Some("kh-clipboard-test")
        );
        board.clear_if_unchanged(token);
        let text = general.stringForType(unsafe { objc2_app_kit::NSPasteboardTypeString });
        assert!(text.is_none(), "cleared");
        let plain = board.copy("{{secret:a.b}}", false).unwrap();
        let marker = general.stringForType(&NSString::from_str("org.nspasteboard.ConcealedType"));
        assert!(marker.is_none(), "a placeholder is not marked");
        board.clear_if_unchanged(plain);
    }
}
