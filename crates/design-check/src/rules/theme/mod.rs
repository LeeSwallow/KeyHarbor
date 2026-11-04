//! Rules about the light and dark themes: both dark blocks match the light one, and text colours
//! meet WCAG contrast in each.

mod contrast;
mod theme_parity;

pub(super) use contrast::Contrast;
pub use contrast::{CONTRAST_PAIRS, ContrastPair, Measured, measure};
pub(super) use theme_parity::ThemeParity;
