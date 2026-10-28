//! Repository checks for the design system in `design-system/` (ADR-0015). A dev tool: nothing in the
//! workspace depends on it and it is never shipped.
//!
//! `scanner` reads CSS, `model` gathers everything into a `DesignSet`, `layers` is the table of
//! what each layer may do, `rules` holds one check per file behind the `Rule` trait, `report`
//! prints what they find. `color` and `theme` back the contrast and parity rules. `serve` is a
//! small static server for looking at the showcase.

pub mod color;
pub mod layers;
pub mod model;
pub mod report;
pub mod rules;
pub mod scanner;
pub mod serve;
pub mod theme;

use std::path::PathBuf;

/// The repository root this crate was built in.
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
