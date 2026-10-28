//! Which CSS selectors an agent may use (I3). A selector that can test an element's state or
//! value (`[value^=…]`, `:has(…)`, `:placeholder-shown`, `:valid`, …) turns `click`, `fill`
//! and `wait` into an oracle for what a field holds, so only a small grammar is accepted:
//!
//! ```text
//! selector   := compound (combinator compound)*
//! combinator := whitespace | '>'
//! compound   := tag? ( '#' ident | '.' ident | attribute | ':nth-of-type(' digits ')' )*
//! attribute  := '[' (name | type | id | aria-label | data-testid) '=' (ident | quoted) ']'
//! ```
//!
//! CSS escapes are resolved first, so an escaped `\5b value` cannot hide an attribute test.

use anyhow::{Result, bail};

const ATTRIBUTES: &[&str] = &["name", "type", "id", "aria-label", "data-testid"];

/// The text a CSS parser sees once escapes (`\41`, `\[`) are resolved.
fn unescape(selector: &str) -> String {
    let mut out = String::with_capacity(selector.len());
    let mut chars = selector.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let mut hex = String::new();
        while hex.len() < 6 && chars.peek().is_some_and(char::is_ascii_hexdigit) {
            hex.push(chars.next().expect("peeked"));
        }
        if hex.is_empty() {
            if let Some(next) = chars.next() {
                out.push(next);
            }
            continue;
        }
        if chars.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            chars.next();
        }
        let code = u32::from_str_radix(&hex, 16).unwrap_or(0xFFFD);
        out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
    }
    out
}

fn ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_' || !c.is_ascii()
}

struct Parser<'a> {
    rest: &'a str,
}

impl Parser<'_> {
    fn eat(&mut self, prefix: &str) -> bool {
        match self.rest.strip_prefix(prefix) {
            Some(rest) => {
                self.rest = rest;
                true
            }
            None => false,
        }
    }
    fn spaces(&mut self) -> bool {
        let trimmed = self.rest.trim_start();
        let any = trimmed.len() != self.rest.len();
        self.rest = trimmed;
        any
    }
    fn ident(&mut self) -> Option<&str> {
        let end = self
            .rest
            .find(|c| !ident_char(c))
            .unwrap_or(self.rest.len());
        if end == 0 {
            return None;
        }
        let (ident, rest) = self.rest.split_at(end);
        self.rest = rest;
        Some(ident)
    }
    fn value(&mut self) -> bool {
        for quote in ['"', '\''] {
            if let Some(rest) = self.rest.strip_prefix(quote) {
                return match rest.find(quote) {
                    Some(end) => {
                        self.rest = &rest[end + 1..];
                        true
                    }
                    None => false,
                };
            }
        }
        self.ident().is_some()
    }
    fn attribute(&mut self) -> bool {
        self.spaces();
        let Some(name) = self.ident() else {
            return false;
        };
        if !ATTRIBUTES.contains(&name.to_ascii_lowercase().as_str()) {
            return false;
        }
        self.spaces();
        if !self.eat("=") {
            return false;
        }
        self.spaces();
        if !self.value() {
            return false;
        }
        self.spaces();
        self.eat("]")
    }
    /// One compound selector; false if nothing (or something not allowed) is there.
    fn compound(&mut self) -> bool {
        let mut parts = usize::from(self.ident().is_some());
        loop {
            let ok = if self.eat("#") || self.eat(".") {
                self.ident().is_some()
            } else if self.eat("[") {
                self.attribute()
            } else if self.eat(":nth-of-type(") {
                let digits = self.rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(0);
                self.rest = &self.rest[digits..];
                digits > 0 && self.eat(")")
            } else {
                return parts > 0;
            };
            if !ok {
                return false;
            }
            parts += 1;
        }
    }
}

/// Accepts `selector` only if it is in the grammar above.
pub fn check(selector: &str) -> Result<()> {
    let text = unescape(selector);
    let mut parser = Parser { rest: text.trim() };
    let mut valid = parser.compound();
    while valid && !parser.rest.is_empty() {
        let spaced = parser.spaces();
        let child = parser.eat(">");
        parser.spaces();
        valid = (spaced || child) && parser.compound();
    }
    if !valid || selector.len() > 1024 {
        bail!(
            "selector not allowed: use tags, #id, .class, [name|type|id|aria-label|data-testid=…], \
             descendant or > combinators and :nth-of-type(n)"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_structural_selectors_are_allowed() {
        for ok in [
            "#user",
            "input",
            ".btn.primary",
            "form > input[name=email]",
            "form input[type=\"password\"]",
            "[data-testid='sign-in']",
            "[aria-label=Search]",
            "ul > li:nth-of-type(3) a",
            "  #go  ",
            "#로그인",
            "div#a\\31 23",
        ] {
            assert!(check(ok).is_ok(), "{ok}");
        }
    }

    #[test]
    fn selectors_that_test_state_or_values_are_refused() {
        for bad in [
            "input[value^=a]",
            "input[value='KH']",
            "#pw[value*=x]",
            "form:has(input[value=a])",
            "input:placeholder-shown",
            "input:invalid",
            "input:valid",
            "input:checked",
            "#a, #b",
            "a + b",
            "a ~ b",
            "[name^=pw]",
            "[name]",
            "*",
            "",
            "input\\5b value=a]",
            "input\\[value=a]",
            "#a:not(#b)",
            "input::after",
            "a[name=\"unterminated]",
        ] {
            assert!(check(bad).is_err(), "{bad}");
        }
    }
}
