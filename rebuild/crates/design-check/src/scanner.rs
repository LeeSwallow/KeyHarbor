//! A small CSS reader: enough to see custom properties, `var()` references, literal values
//! and class selectors. It is not a CSS parser; it understands blocks, at-rules, comments,
//! strings and parentheses, which is all the design system uses.

/// One `name: value` inside a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decl {
    pub name: String,
    pub value: String,
    pub line: usize,
}

/// A rule block with its selector and the at-rules around it (`@media (…)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub at_rules: Vec<String>,
    pub selector: String,
    pub decls: Vec<Decl>,
}

impl Block {
    /// The custom properties this block defines, in order.
    pub fn custom_properties(&self) -> impl Iterator<Item = &Decl> {
        self.decls.iter().filter(|d| d.name.starts_with("--"))
    }
}

/// A stylesheet: its blocks and its `@import`s.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sheet {
    pub blocks: Vec<Block>,
    pub imports: Vec<String>,
}

/// Reads a stylesheet. Unbalanced input is read as far as it goes.
pub fn parse_sheet(src: &str) -> Sheet {
    let text = strip_comments(src);
    let mut sheet = Sheet::default();
    let mut reader = Reader {
        text: &text,
        pos: 0,
    };
    reader.read_items(&mut Vec::new(), &mut sheet);
    sheet
}

/// Replaces `/* … */` with spaces, keeping newlines so line numbers stay right.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after.find("*/").map_or(after.len(), |e| e + 2);
        out.extend(
            rest[start..start + 2 + end]
                .chars()
                .map(|c| if c == '\n' { '\n' } else { ' ' }),
        );
        rest = &after[end.min(after.len())..];
    }
    out.push_str(rest);
    out
}

struct Reader<'a> {
    text: &'a str,
    pos: usize,
}

impl Reader<'_> {
    fn line_at(&self, pos: usize) -> usize {
        self.text[..pos].matches('\n').count() + 1
    }

    /// Reads up to the next top-level `{`, `;` or `}`; returns the text and the stop char.
    fn read_prelude(&mut self) -> (String, Option<char>) {
        let start = self.pos;
        let mut depth = 0usize;
        let mut quote: Option<char> = None;
        for (i, c) in self.text[start..].char_indices() {
            match (quote, c) {
                (Some(q), _) if c == q => quote = None,
                (Some(_), _) => {}
                (None, '"' | '\'') => quote = Some(c),
                (None, '(') => depth += 1,
                (None, ')') => depth = depth.saturating_sub(1),
                (None, '{' | ';' | '}') if depth == 0 => {
                    self.pos = start + i + 1;
                    return (self.text[start..start + i].trim().to_string(), Some(c));
                }
                _ => {}
            }
        }
        self.pos = self.text.len();
        (self.text[start..].trim().to_string(), None)
    }

    fn read_items(&mut self, at_rules: &mut Vec<String>, sheet: &mut Sheet) {
        loop {
            let (prelude, stop) = self.read_prelude();
            match stop {
                Some(';') if prelude.starts_with("@import") => {
                    sheet.imports.push(import_target(&prelude))
                }
                Some('{') if prelude.starts_with('@') => {
                    at_rules.push(prelude);
                    self.read_items(at_rules, sheet);
                    at_rules.pop();
                }
                Some('{') => {
                    let decls = self.read_decls();
                    sheet.blocks.push(Block {
                        at_rules: at_rules.clone(),
                        selector: prelude,
                        decls,
                    });
                }
                Some('}') | None => return,
                Some(_) => {}
            }
        }
    }

    fn read_decls(&mut self) -> Vec<Decl> {
        let mut decls = Vec::new();
        loop {
            let rest = &self.text[self.pos..];
            let line = self.line_at(self.pos + (rest.len() - rest.trim_start().len()));
            let (text, stop) = self.read_prelude();
            if let Some((name, value)) = text.split_once(':') {
                decls.push(Decl {
                    name: name.trim().to_string(),
                    value: value.trim().to_string(),
                    line,
                });
            }
            if stop != Some(';') {
                return decls;
            }
        }
    }
}

fn import_target(prelude: &str) -> String {
    let rest = prelude.trim_start_matches("@import").trim();
    let rest = rest
        .strip_prefix("url(")
        .map_or(rest, |r| r.trim_end_matches(')'));
    rest.trim_matches(|c| c == '"' || c == '\'' || c == ' ')
        .to_string()
}

/// The custom-property names a value references with `var(--name…)`.
/// Names built at run time (`var(--c-${c})` in a template string) are left out.
pub fn var_refs(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = value;
    while let Some(i) = rest.find("var(") {
        let after = rest[i + 4..].trim_start();
        let len = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .unwrap_or(after.len());
        let name = &after[..len];
        let dynamic = after[len..].starts_with('$');
        if name.starts_with("--") && name.len() > 2 && !dynamic {
            out.push(name.to_string());
        }
        rest = &after[len..];
    }
    out
}

/// Literal colours in a value: `#hex`, `rgb()`/`hsl()`-family functions and named colours.
/// `transparent` and `currentColor` are not colours of their own and are allowed everywhere.
pub fn literal_colours(value: &str) -> Vec<String> {
    const FUNCTIONS: &[&str] = &[
        "rgb(", "rgba(", "hsl(", "hsla(", "hwb(", "lab(", "lch(", "oklab(", "oklch(", "color(",
    ];
    const NAMED: &[&str] = &[
        "white", "black", "red", "green", "blue", "gray", "grey", "orange", "yellow", "purple",
        "pink", "teal", "cyan", "magenta",
    ];
    let lower = value.to_ascii_lowercase();
    let mut out = Vec::new();
    for word in words(&lower) {
        if let Some(hex) = word.strip_prefix('#')
            && matches!(hex.len(), 3 | 4 | 6 | 8)
            && hex.chars().all(|c| c.is_ascii_hexdigit())
        {
            out.push(word.to_string());
        } else if NAMED.contains(&word) {
            out.push(word.to_string());
        }
    }
    for f in FUNCTIONS {
        let mut rest = lower.as_str();
        while let Some(i) = rest.find(f) {
            let boundary = i == 0
                || !rest.as_bytes()[i - 1].is_ascii_alphanumeric()
                    && rest.as_bytes()[i - 1] != b'-';
            if boundary {
                out.push(f.trim_end_matches('(').to_string());
            }
            rest = &rest[i + f.len()..];
        }
    }
    out
}

/// Pixel lengths other than `0px` and `1px` (a hairline) in a value.
pub fn px_literals(value: &str) -> Vec<String> {
    words(value)
        .filter_map(|w| {
            w.strip_suffix("px")
                .map(|n| (w, n.trim_start_matches(['-', '+'])))
        })
        .filter(|(_, n)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit() || c == '.'))
        .filter(|(_, n)| n.parse::<f64>().is_ok_and(|v| v != 0.0 && v != 1.0))
        .map(|(w, _)| w.to_string())
        .collect()
}

/// The `kh-*` class names a selector styles.
pub fn selector_classes(selector: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in selector.split('.').skip(1) {
        let name: String = part
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if name.starts_with("kh-") || name == "kh" {
            out.push(name);
        }
    }
    out
}

/// `kh-*` names written anywhere in a page or script (class attributes, template strings).
pub fn kh_names(text: &str) -> Vec<String> {
    let is_name = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("kh-") {
        let starts_word = rest[..i].chars().next_back().is_none_or(|c| !is_name(c));
        let len = rest[i..]
            .find(|c: char| !is_name(c))
            .unwrap_or(rest.len() - i);
        let name = rest[i..i + len].trim_end_matches('-');
        if starts_word && name.len() > 3 {
            out.push(name.to_string());
        }
        rest = &rest[i + len.max(3)..];
    }
    out
}

fn words(value: &str) -> impl Iterator<Item = &str> {
    value
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | '(' | ')' | '/' | '*'))
        .filter(|w| !w.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_blocks_inside_media_and_imports() {
        let sheet = parse_sheet(
            "@import url(\"a.css\");\n/* c */\n:root { --x: 1px; --y: var(--x); }\n@media (prefers-color-scheme: dark) {\n  :root:not([data-theme=\"light\"]) { --x: 2px; }\n}\n",
        );
        assert_eq!(sheet.imports, ["a.css"]);
        assert_eq!(sheet.blocks.len(), 2);
        assert_eq!(sheet.blocks[0].selector, ":root");
        assert_eq!(sheet.blocks[0].decls[1].value, "var(--x)");
        assert_eq!(
            sheet.blocks[1].at_rules,
            ["@media (prefers-color-scheme: dark)"]
        );
        assert_eq!(sheet.blocks[1].decls[0].line, 5);
    }

    #[test]
    fn keeps_semicolons_inside_parentheses() {
        let sheet = parse_sheet(".a { background: url(\"x;y.svg\"); color: red }");
        assert_eq!(sheet.blocks[0].decls.len(), 2);
    }

    #[test]
    fn finds_var_references_but_not_template_names() {
        assert_eq!(
            var_refs("0 0 0 var(--ring, 3px) var( --c-a)"),
            ["--ring", "--c-a"]
        );
        assert!(var_refs("var(--c-${c})").is_empty());
    }

    #[test]
    fn finds_literal_colours() {
        assert_eq!(literal_colours("1px solid #fff"), ["#fff"]);
        assert_eq!(literal_colours("0 1px rgba(0,0,0,.3)"), ["rgba"]);
        assert_eq!(literal_colours("white"), ["white"]);
        assert!(literal_colours("var(--c-ink) transparent currentColor").is_empty());
        assert!(literal_colours("var(--c-dom-green)").is_empty());
    }

    #[test]
    fn finds_px_other_than_hairlines() {
        assert_eq!(px_literals("0 1px 2px calc(-1px + 12px)"), ["2px", "12px"]);
        assert!(px_literals("0px 1px var(--space-2)").is_empty());
    }

    #[test]
    fn finds_kh_names_in_pages() {
        assert_eq!(
            kh_names(r#"<b class="kh-btn ${v}"> x-kh-no "kh-" kh-row"#),
            ["kh-btn", "kh-row"]
        );
    }

    #[test]
    fn finds_kh_classes_in_selectors() {
        assert_eq!(
            selector_classes(".kh-btn.primary:hover, .kh :focus-visible"),
            ["kh-btn", "kh"]
        );
    }
}
