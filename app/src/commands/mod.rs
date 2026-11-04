//! What the app's screens ask for, as plain functions over core and guard (no Tauri types),
//! so each is tested without a window. The Tauri layer (`main.rs`) only forwards. Results are
//! shaped for the screens (`*Dto`); none carries a value except [`reveal`].

mod admin;
mod answers;
mod ids;
mod inbox;
mod items;
mod manage;
mod overview;
mod presence;
mod sessions;
mod templates;

pub use admin::{
    RuleDto, RuleForm, SettingDto, WeakeningDto, policies, remove_policy, set_policy, set_setting,
    settings,
};
pub use answers::{
    HandoffDto, RequestForm, fulfill_request, fulfill_with_item, handoffs, request_form,
    request_window_path, resume_handoff,
};
pub use inbox::{InboxDto, confirm_draft, decline_request, discard_draft, inbox};
pub use items::{ItemDto, copy_placeholder, copy_value, items, reveal};
pub use manage::{
    ChangeForm, Edited, FormField, KindForm, NewItemForm, Saved, ValueWarningDto, add_item,
    edit_item, kinds, remove_item, rename_item,
};
pub use overview::{AuditDto, OverviewDto, audit, open_app, overview};
pub use presence::{
    MenuText, Notice, NoticeKinds, Seen, Snapshot, menu_text, notice_kinds, notices, snapshot,
};
pub use sessions::{
    BrowsersDto, SessionDto, SiteHandOff, browsers, open_session, recover_session, remove_session,
    request_site, sessions, stop_session, unlock_session,
};
pub use templates::{TemplateDto, TemplateFieldDto, add_template, remove_template, templates};

/// Errors cross to the window as text: a short reason, never a value.
pub fn message(error: anyhow::Error) -> String {
    format!("{error:#}")
}
