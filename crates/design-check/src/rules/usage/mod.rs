//! Rules about what the pages use: every class and icon they name exists, and every stylesheet
//! is imported and used.

mod classes;
mod component_files;
mod icons;

pub(super) use classes::Classes;
pub(super) use component_files::ComponentFiles;
pub(super) use icons::Icons;
