//! The value as written.

use super::Variant;
use zeroize::Zeroizing;

pub struct Raw;

impl Variant for Raw {
    fn expand(&self, value: &str) -> Vec<Zeroizing<String>> {
        vec![Zeroizing::new(value.to_owned())]
    }
}
