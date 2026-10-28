//! The body of a JSON string literal (`"` → `\"`, `\` → `\\`, control characters → `\n`,
//! `\u001f`, …), as snapshots and other JSON text carry a value (I2).

use super::Variant;
use zeroize::Zeroizing;

pub struct Escaped;

impl Variant for Escaped {
    fn expand(&self, value: &str) -> Vec<Zeroizing<String>> {
        let quoted = Zeroizing::new(serde_json::to_string(value).expect("strings serialize"));
        vec![Zeroizing::new(quoted[1..quoted.len() - 1].to_owned())]
    }
}
