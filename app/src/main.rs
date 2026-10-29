//! The admin app's window: Tauri forwards each command to `keyharbor_app::commands` and
//! turns errors into short text. Nothing here decides; the app opens no network listener.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod shell;

use keyharbor_app::{
    Ctx, SystemClock,
    commands::{
        self, AuditDto, ChangeForm, HandoffDto, InboxDto, ItemDto, KindForm, NewItemForm,
        OverviewDto, RequestForm, RuleDto, SessionDto, SettingDto, message,
    },
    confirm::Outcome,
};
use keyharbor_keystore::KeychainKeyStore;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
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

#[tauri::command]
fn request_form(ctx: State<'_, Ctx>, id: String) -> Reply<RequestForm> {
    commands::request_form(&ctx, &id).map_err(message)
}
#[tauri::command]
fn fulfill_request(
    ctx: State<'_, Ctx>,
    id: String,
    values: BTreeMap<String, String>,
) -> Reply<String> {
    commands::fulfill_request(&ctx, &id, values).map_err(message)
}
#[tauri::command]
fn fulfill_with_item(ctx: State<'_, Ctx>, id: String, item: String) -> Reply<String> {
    commands::fulfill_with_item(&ctx, &id, &item).map_err(message)
}
#[tauri::command]
fn handoffs(ctx: State<'_, Ctx>) -> Reply<Vec<HandoffDto>> {
    commands::handoffs(&ctx).map_err(message)
}
#[tauri::command]
fn resume_handoff(ctx: State<'_, Ctx>, session: String, request: String) -> Reply<()> {
    commands::resume_handoff(&ctx, &session, &request).map_err(message)
}
#[tauri::command]
fn settings(ctx: State<'_, Ctx>) -> Reply<Vec<SettingDto>> {
    commands::settings(&ctx).map_err(message)
}
#[tauri::command]
fn set_setting(ctx: State<'_, Ctx>, key: String, value: String, confirmed: bool) -> Reply<Outcome> {
    commands::set_setting(&ctx, &key, &value, confirmed).map_err(message)
}
#[tauri::command]
fn policies(ctx: State<'_, Ctx>) -> Reply<Vec<RuleDto>> {
    commands::policies(&ctx).map_err(message)
}
#[tauri::command]
fn set_policy(
    ctx: State<'_, Ctx>,
    agent: String,
    resource: String,
    target: String,
    effect: String,
    confirmed: bool,
) -> Reply<Outcome> {
    commands::set_policy(&ctx, (&agent, &resource, &target, &effect), confirmed).map_err(message)
}
#[tauri::command]
fn remove_policy(
    ctx: State<'_, Ctx>,
    agent: String,
    resource: String,
    target: String,
    confirmed: bool,
) -> Reply<Outcome> {
    commands::remove_policy(&ctx, &agent, &resource, &target, confirmed).map_err(message)
}

#[tauri::command]
fn copy_value(ctx: State<'_, Ctx>, item: String, field: String) -> Reply<()> {
    commands::copy_value(&ctx, &item, &field).map_err(message)
}
#[tauri::command]
fn copy_placeholder(ctx: State<'_, Ctx>, text: String) -> Reply<()> {
    commands::copy_placeholder(&ctx, &text).map_err(message)
}
#[tauri::command]
fn open_session(ctx: State<'_, Ctx>, id: String) -> Reply<()> {
    commands::open_session(&ctx, &id).map_err(message)
}
#[tauri::command]
fn stop_session(ctx: State<'_, Ctx>, id: String) -> Reply<()> {
    commands::stop_session(&ctx, &id).map_err(message)
}
#[tauri::command]
fn recover_session(ctx: State<'_, Ctx>, id: String, browser_closed: bool) -> Reply<()> {
    commands::recover_session(&ctx, &id, browser_closed).map_err(message)
}

#[tauri::command]
fn kinds() -> Vec<KindForm> {
    commands::kinds()
}
#[tauri::command]
fn add_item(ctx: State<'_, Ctx>, form: NewItemForm) -> Reply<String> {
    commands::add_item(&ctx, form).map_err(message)
}
#[tauri::command]
fn edit_item(
    ctx: State<'_, Ctx>,
    name: String,
    form: ChangeForm,
    confirmed: bool,
) -> Reply<Outcome> {
    commands::edit_item(&ctx, &name, form, confirmed).map_err(message)
}
#[tauri::command]
fn remove_item(ctx: State<'_, Ctx>, name: String) -> Reply<()> {
    commands::remove_item(&ctx, &name).map_err(message)
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
    let clipboard = Arc::new(keyharbor_app::clipboard::MacClipboard);
    let ctx = Ctx::new(
        data_dir(),
        keys,
        Arc::new(SystemClock),
        cli_binary(),
        clipboard,
    );
    tauri::Builder::default()
        .manage(ctx)
        .setup(|app| {
            shell::install(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps KeyHarbor in the menu bar (09 §9.2).
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            overview,
            items,
            reveal,
            sessions,
            inbox,
            audit,
            decline_request,
            confirm_draft,
            discard_draft,
            request_form,
            fulfill_request,
            fulfill_with_item,
            handoffs,
            resume_handoff,
            settings,
            set_setting,
            policies,
            set_policy,
            remove_policy,
            copy_value,
            copy_placeholder,
            open_session,
            stop_session,
            recover_session,
            kinds,
            add_item,
            edit_item,
            remove_item
        ])
        .run(tauri::generate_context!())
        .expect("KeyHarbor app failed to start");
}
