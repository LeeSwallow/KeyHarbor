//! The admin app's window: Tauri forwards each command to `keyharbor_app::commands` and
//! turns errors into short text. Nothing here decides; the app opens no network listener.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use keyharbor_app::{
    Ctx, SystemClock,
    commands::{self, AuditDto, InboxDto, ItemDto, OverviewDto, SessionDto, message},
};
use keyharbor_keystore::KeychainKeyStore;
use std::{path::PathBuf, sync::Arc};
use tauri::State;

type Reply<T> = Result<T, String>;

#[tauri::command]
fn overview(ctx: State<'_, Ctx>) -> Reply<OverviewDto> {
    commands::overview(&ctx).map_err(message)
}
#[tauri::command]
fn items(ctx: State<'_, Ctx>) -> Reply<Vec<ItemDto>> {
    commands::items(&ctx).map_err(message)
}
#[tauri::command]
fn reveal(ctx: State<'_, Ctx>, item: String, field: String) -> Reply<String> {
    commands::reveal(&ctx, &item, &field).map_err(message)
}
#[tauri::command]
fn sessions(ctx: State<'_, Ctx>) -> Reply<Vec<SessionDto>> {
    commands::sessions(&ctx).map_err(message)
}
#[tauri::command]
fn inbox(ctx: State<'_, Ctx>) -> Reply<InboxDto> {
    commands::inbox(&ctx).map_err(message)
}
#[tauri::command]
fn audit(ctx: State<'_, Ctx>, limit: usize) -> Reply<Vec<AuditDto>> {
    commands::audit(&ctx, limit).map_err(message)
}
#[tauri::command]
fn decline_request(ctx: State<'_, Ctx>, id: String) -> Reply<()> {
    commands::decline_request(&ctx, &id).map_err(message)
}
#[tauri::command]
fn confirm_draft(ctx: State<'_, Ctx>, name: String) -> Reply<String> {
    commands::confirm_draft(&ctx, &name).map_err(message)
}
#[tauri::command]
fn discard_draft(ctx: State<'_, Ctx>, name: String) -> Reply<()> {
    commands::discard_draft(&ctx, &name).map_err(message)
}

/// The data folder the CLI uses: `KEYHARBOR_HOME`, else the macOS application data folder.
fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("KEYHARBOR_HOME") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join("Library/Application Support/KeyHarbor")
}

/// The CLI that runs supervisors: `KEYHARBOR_CLI`, else `Contents/Helpers/keyharbor` beside
/// this binary in the bundle (R-45), else `keyharbor` on PATH.
fn cli_binary() -> PathBuf {
    if let Some(path) = std::env::var_os("KEYHARBOR_CLI") {
        return PathBuf::from(path);
    }
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("../Helpers/keyharbor")));
    match bundled {
        Some(path) if path.exists() => path,
        _ => PathBuf::from("keyharbor"),
    }
}

fn main() {
    let keys: keyharbor_app::KeyStores = Arc::new(|| Box::new(KeychainKeyStore));
    let ctx = Ctx::new(data_dir(), keys, Arc::new(SystemClock), cli_binary());
    tauri::Builder::default()
        .manage(ctx)
        .invoke_handler(tauri::generate_handler![
            overview,
            items,
            reveal,
            sessions,
            inbox,
            audit,
            decline_request,
            confirm_draft,
            discard_draft
        ])
        .run(tauri::generate_context!())
        .expect("KeyHarbor app failed to start");
}
