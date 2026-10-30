//! The secure input window (09 §9.4.5): small, always on top, kept out of screenshots and
//! screen sharing, showing only the form for one request. It loads the same UI with
//! `?request=<id>`; the path comes from `commands::request_window_path`, which takes only a
//! pending request's id.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "request";

/// Opens the window on `path`, replacing one already open (it may show another request).
pub fn open(app: &AppHandle, path: String) -> tauri::Result<()> {
    if let Some(open) = app.get_webview_window(LABEL) {
        open.destroy()?;
    }
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(path.into()))
        .title("KeyHarbor · 정보 요청")
        .inner_size(480.0, 640.0)
        .resizable(false)
        .always_on_top(true)
        .content_protected(true)
        .build()?;
    window.set_focus()
}

pub fn close(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.destroy();
    }
}
