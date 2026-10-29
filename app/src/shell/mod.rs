//! The app's presence outside its window: the menu bar icon (09 §9.4.6). Closing the window
//! keeps the app in the menu bar; "종료" quits.

mod tray;

pub use tray::install;
