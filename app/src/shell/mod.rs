//! The app's presence outside its main window: the menu bar icon (09 §9.4.6) and the secure
//! input window (09 §9.4.5). Closing the main window keeps the app in the menu bar; "종료" quits.

pub mod request_window;
mod tray;

pub use tray::install;
