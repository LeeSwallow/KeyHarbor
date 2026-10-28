//! The forms of a value the masker looks for (Strategy): each [`Variant`] turns a value into
//! the ways pages, URLs and JSON commonly carry it. Adding a form is one file here plus one
//! line in [`VARIANTS`].

mod card;
mod html;
mod json;
mod percent;
mod raw;

use super::MIN_MASKED_CHARS;
use zeroize::Zeroizing;

pub trait Variant: Sync {
    /// The forms of `value` this variant produces (possibly none, possibly `value` itself).
    fn expand(&self, value: &str) -> Vec<Zeroizing<String>>;
}

/// Every variant, in the order its forms are listed.
static VARIANTS: &[&dyn Variant] = &[
    &raw::Raw,
    &percent::COMPONENT_UPPER,
    &percent::COMPONENT_LOWER,
    &percent::STRICT_UPPER,
    &percent::COMPONENT_PLUS,
    &percent::STRICT_PLUS,
    &percent::FORM_UPPER,
    &percent::FORM_LOWER,
    &html::APOSTROPHE_DECIMAL,
    &html::APOSTROPHE_HEX,
    &json::Escaped,
    &card::Groups,
];

/// Every distinct form of `value` that is masked; none for values under
/// [`MIN_MASKED_CHARS`] characters.
pub fn variants(value: &str) -> Vec<Zeroizing<String>> {
    if value.chars().count() < MIN_MASKED_CHARS {
        return Vec::new();
    }
    let mut unique: Vec<Zeroizing<String>> = Vec::new();
    for form in VARIANTS.iter().flat_map(|v| v.expand(value)) {
        if !unique.contains(&form) {
            unique.push(form);
        }
    }
    unique
}
