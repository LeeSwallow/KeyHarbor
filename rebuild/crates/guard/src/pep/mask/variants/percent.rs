//! Percent-encoding as URLs and form bodies carry a value: which bytes stay as they are,
//! upper- or lowercase hex, and `+` for spaces.

use super::Variant;
use zeroize::Zeroizing;

pub struct Percent {
    keep: fn(u8) -> bool,
    upper: bool,
    plus: bool,
}

/// encodeURIComponent's unreserved set.
fn component(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b)
}
/// RFC 3986 unreserved characters.
fn strict(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"-_.~".contains(&b)
}
/// What browsers send for form fields (WHATWG URL, urlencoded serializer; I5).
fn form(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"*-._".contains(&b)
}

pub const COMPONENT_UPPER: Percent = Percent {
    keep: component,
    upper: true,
    plus: false,
};
pub const COMPONENT_LOWER: Percent = Percent {
    keep: component,
    upper: false,
    plus: false,
};
pub const STRICT_UPPER: Percent = Percent {
    keep: strict,
    upper: true,
    plus: false,
};
pub const COMPONENT_PLUS: Percent = Percent {
    keep: component,
    upper: true,
    plus: true,
};
pub const STRICT_PLUS: Percent = Percent {
    keep: strict,
    upper: true,
    plus: true,
};
pub const FORM_UPPER: Percent = Percent {
    keep: form,
    upper: true,
    plus: true,
};
pub const FORM_LOWER: Percent = Percent {
    keep: form,
    upper: false,
    plus: true,
};

impl Variant for Percent {
    fn expand(&self, value: &str) -> Vec<Zeroizing<String>> {
        let mut out = Zeroizing::new(String::with_capacity(value.len() * 3));
        for b in value.bytes() {
            if self.plus && b == b' ' {
                out.push('+');
            } else if (self.keep)(b) {
                out.push(b as char);
            } else if self.upper {
                out.push_str(&format!("%{b:02X}"));
            } else {
                out.push_str(&format!("%{b:02x}"));
            }
        }
        vec![out]
    }
}
