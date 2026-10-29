//! What the app's screens ask for, as plain functions over core and guard (no Tauri types),
//! so each is tested without a window. The Tauri layer (`main.rs`) only forwards. Results are
//! shaped for the screens (`*Dto`); none carries a value except [`reveal`].

mod admin;
mod answers;
mod inbox;
mod items;
mod overview;
mod sessions;

pub use admin::{RuleDto, SettingDto, policies, remove_policy, set_policy, set_setting, settings};
pub use answers::{
    HandoffDto, RequestForm, fulfill_request, fulfill_with_item, handoffs, request_form,
    resume_handoff,
};
pub use inbox::{InboxDto, confirm_draft, decline_request, discard_draft, inbox};
pub use items::{ItemDto, items, reveal};
pub use overview::{AuditDto, OverviewDto, audit, overview};
pub use sessions::{SessionDto, sessions};

/// Errors cross to the window as text: a short reason, never a value.
pub fn message(error: anyhow::Error) -> String {
    format!("{error:#}")
}
