//! What the app says outside its window (09 §9.3, §9.4.6): the menu bar text and macOS
//! notices, both from what waits and what was refused.

mod menu;
mod notices;

pub use menu::{MenuText, menu_text};
pub use notices::{Notice, NoticeKinds, Seen, Snapshot, notice_kinds, notices, snapshot};
