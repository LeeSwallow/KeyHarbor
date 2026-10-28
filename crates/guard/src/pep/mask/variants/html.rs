//! HTML-escaped text (`&amp;`, `&lt;`, `&gt;`, `&quot;`, and an apostrophe entity).

use super::Variant;
use zeroize::Zeroizing;

pub struct Html {
    apostrophe: &'static str,
}

pub const APOSTROPHE_DECIMAL: Html = Html {
    apostrophe: "&#39;",
};
pub const APOSTROPHE_HEX: Html = Html {
    apostrophe: "&#x27;",
};

impl Variant for Html {
    fn expand(&self, value: &str) -> Vec<Zeroizing<String>> {
        let mut out = Zeroizing::new(String::with_capacity(value.len() + 16));
        for c in value.chars() {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str(self.apostrophe),
                c => out.push(c),
            }
        }
        vec![out]
    }
}
