//! The app's presence outside its main window: the menu bar icon (09 §9.4.6) and the secure
//! input window (09 §9.4.5). Closing the main window keeps the app in the menu bar; "종료" quits.

pub mod request_window;
mod tray;

use keyharbor_app::{
    Ctx,
    window::{MainWindow, open_main_window},
};
use std::fmt::Display;
use tauri::{AppHandle, Manager, WebviewWindow};

pub use tray::install;

/// The main window's label (tauri.conf.json `app.windows[0].label`).
pub const MAIN_LABEL: &str = "main";

/// Notes a failure the shell goes on past (a menu, a title, a notification, a refused open),
/// so it is not silently lost. The errors are Tauri's, the OS's or a refusal: never a value.
pub fn log_ignored<T, E: Display>(what: &str, result: Result<T, E>) {
    if let Err(error) = result {
        eprintln!("KeyHarbor: could not {what}: {error:#}");
    }
}

struct TauriWindow(WebviewWindow);

impl MainWindow for TauriWindow {
    fn show(&self) -> anyhow::Result<()> {
        Ok(self.0.show()?)
    }
    fn focus(&self) -> anyhow::Result<()> {
        Ok(self.0.set_focus()?)
    }
}

/// Shows the main window past the `OpenApp` challenge, at launch and from the menu bar. A
/// refusal leaves it hidden and the app in the menu bar, to be asked again (J-177).
pub fn show_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_LABEL) else {
        log_ignored("find the main window", Err::<(), _>(MAIN_LABEL));
        return;
    };
    log_ignored(
        "open the main window",
        open_main_window(&app.state::<Ctx>(), &TauriWindow(window)),
    );
}
