//! The menu bar icon: a template image, the count of what waits beside it, and a menu with
//! the status, the waiting count, "KeyHarbor 열기" and "종료"; and notifications for what is
//! new. The text comes from `commands::menu_text` and `commands::notices`; this file only
//! builds Tauri menus and posts.

use keyharbor_app::{
    Ctx,
    commands::{self, MenuText, Seen},
};
use std::time::Duration;
use tauri::{
    AppHandle, Manager,
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{TrayIcon, TrayIconBuilder},
};

/// How often the menu bar counts are read again. Fixed: the counts are a glance, not an alarm.
pub const REFRESH: Duration = Duration::from_secs(10);

const OPEN: &str = "open";
const QUIT: &str = "quit";
const ANSWER: &str = "answer";

fn menu(app: &AppHandle, text: &MenuText) -> tauri::Result<Menu<tauri::Wry>> {
    let status = MenuItem::with_id(app, "status", &text.status, false, None::<&str>)?;
    let waiting = MenuItem::with_id(app, "waiting", &text.waiting, false, None::<&str>)?;
    let open = MenuItem::with_id(app, OPEN, "KeyHarbor 열기", true, Some("CmdOrCtrl+O"))?;
    let quit = MenuItem::with_id(app, QUIT, "종료", true, Some("CmdOrCtrl+Q"))?;
    let line = PredefinedMenuItem::separator(app)?;
    let line2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&status, &waiting, &line, &open, &line2, &quit])?;
    if let Some(answer) = &text.answer {
        menu.insert(
            &MenuItem::with_id(app, ANSWER, answer, true, None::<&str>)?,
            2,
        )?;
    }
    Ok(menu)
}

fn current(app: &AppHandle) -> Option<MenuText> {
    let ctx = app.state::<Ctx>();
    commands::overview(&ctx)
        .ok()
        .map(|o| commands::menu_text(&o))
}

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Puts the icon in the menu bar and keeps its counts fresh every [`REFRESH`].
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let text = current(app).unwrap_or(MenuText {
        status: "KeyHarbor".into(),
        waiting: String::new(),
        badge: None,
        answer: None,
    });
    let icon = Image::from_bytes(include_bytes!("../../icons/menubar.png"))?;
    let tray = TrayIconBuilder::with_id("keyharbor")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("KeyHarbor")
        .menu(&menu(app, &text)?)
        .on_menu_event(|app, event| match event.id().as_ref() {
            OPEN => show_window(app),
            ANSWER => answer(app),
            QUIT => app.exit(0),
            _ => {}
        })
        .build(app)?;
    let _ = tray.set_title(text.badge.as_deref());
    let handle = app.clone();
    std::thread::spawn(move || {
        // The bundle identifier (tauri.conf.json): notices come from KeyHarbor, not from Terminal.
        #[cfg(target_os = "macos")]
        let _ = mac_notification_sys::set_application("io.github.leeswallow.keyharbor");
        let mut seen = Seen::default();
        notify(&handle, &mut seen);
        loop {
            std::thread::sleep(REFRESH);
            refresh(&handle, &tray);
            notify(&handle, &mut seen);
        }
    });
    Ok(())
}

fn refresh(app: &AppHandle, tray: &TrayIcon) {
    let Some(text) = current(app) else { return };
    if let Ok(menu) = menu(app, &text) {
        let _ = tray.set_menu(Some(menu));
    }
    let _ = tray.set_title(text.badge.as_deref());
}

/// Posts a notification for what is new since the last look (`commands::notices`). A failed
/// read skips this round; `seen` stays as it was, so nothing is lost.
fn notify(app: &AppHandle, seen: &mut Seen) {
    let ctx = app.state::<Ctx>();
    let (Ok(now), Ok(kinds)) = (commands::snapshot(&ctx), commands::notice_kinds(&ctx)) else {
        return;
    };
    for notice in commands::notices(seen, &now, kinds) {
        #[cfg(target_os = "macos")]
        let _ = mac_notification_sys::send_notification(&notice.title, None, &notice.body, None);
        #[cfg(not(target_os = "macos"))]
        let _ = notice;
    }
}

/// Opens the secure input window on the oldest waiting request.
fn answer(app: &AppHandle) {
    let ctx = app.state::<Ctx>();
    if let Ok(Some(path)) = commands::request_window_path(&ctx, None) {
        let _ = super::request_window::open(app, path);
    }
}
