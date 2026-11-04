//! Showing the main window past the `OpenApp` challenge (03 §3.10, J-126). Launch and the
//! menu bar's "KeyHarbor 열기" take this one path: refused at launch, the window stays hidden,
//! the app stays in the menu bar, and "열기" asks again (J-177).

use crate::{Ctx, commands};
use anyhow::Result;

/// The main window as far as opening it goes. The shell implements it over the Tauri window.
pub trait MainWindow {
    fn show(&self) -> Result<()>;
    fn focus(&self) -> Result<()>;
}

/// Shows and focuses `window` when the person passes `OpenApp`; refused, it touches nothing
/// and returns the refusal ([`crate::auth::Refused`]) for the caller to note.
pub fn open_main_window(ctx: &Ctx, window: &dyn MainWindow) -> Result<()> {
    commands::open_app(ctx)?;
    window.show()?;
    window.focus()
}
