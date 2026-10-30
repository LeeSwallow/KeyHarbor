//! What the app's screens ask for, as plain functions over core and guard (no Tauri types),
//! so each is tested without a window. The Tauri layer (`main.rs`) only forwards. Results are
//! shaped for the screens (`*Dto`); none carries a value except [`reveal`].

mod admin;
mod answers;
mod inbox;
mod items;
mod manage;
mod overview;
mod sessions;

pub use admin::{RuleDto, SettingDto, policies, remove_policy, set_policy, set_setting, settings};
pub use answers::{
    HandoffDto, RequestForm, fulfill_request, fulfill_with_item, handoffs, request_form,
    request_window_path, resume_handoff,
};
pub use inbox::{InboxDto, confirm_draft, decline_request, discard_draft, inbox};
pub use items::{ItemDto, copy_placeholder, copy_value, items, reveal};
pub use manage::{
    ChangeForm, KindForm, NewItemForm, add_item, edit_item, kinds, remove_item, rename_item,
};
pub use overview::{
    AuditDto, MenuText, Notice, NoticeKinds, OverviewDto, Seen, Snapshot, audit, menu_text,
    notice_kinds, notices, overview, snapshot,
};
pub use sessions::{
    SessionDto, open_session, recover_session, request_site, sessions, stop_session,
};

/// Errors cross to the window as text: a short reason, never a value.
pub fn message(error: anyhow::Error) -> String {
    format!("{error:#}")
}
