//! The themes as tokens/semantic.css writes them: the light blocks, the system dark block
//! (`@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) }`) and the explicit
//! dark block (`:root[data-theme="dark"]`), and resolving a token to its final value.

use std::collections::BTreeMap;

use crate::layers::Layer;
use crate::model::DesignSet;
use crate::scanner::{self, Block};

/// Token names that change with the theme.
pub const THEMED_PREFIXES: &[&str] = &["--c-", "--shadow-", "--scrim"];

pub const SEMANTIC: &str = "tokens/semantic.css";
const SYSTEM_DARK_SELECTOR: &str = ":root:not([data-theme=\"light\"])";
const EXPLICIT_DARK_SELECTOR: &str = ":root[data-theme=\"dark\"]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    pub const ALL: [Theme; 2] = [Theme::Light, Theme::Dark];

    pub fn name(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }
}

pub type Tokens = BTreeMap<String, String>;

#[derive(Debug, Clone, Default)]
pub struct ThemeBlocks {
    pub light: Tokens,
    pub dark_system: Tokens,
    pub dark_explicit: Tokens,
}

pub fn is_themed(name: &str) -> bool {
    THEMED_PREFIXES.iter().any(|p| name.starts_with(p))
}

fn is_dark_media(block: &Block) -> bool {
    block
        .at_rules
        .iter()
        .any(|a| a.replace(' ', "").contains("prefers-color-scheme:dark"))
}

fn collect(into: &mut Tokens, block: &Block) {
    for d in block.custom_properties() {
        into.insert(d.name.clone(), d.value.clone());
    }
}

/// Reads the three theme blocks of tokens/semantic.css.
pub fn theme_blocks(ds: &DesignSet) -> ThemeBlocks {
    let mut out = ThemeBlocks::default();
    let Some(sheet) = ds.sheet(SEMANTIC) else {
        return out;
    };
    for block in &sheet.sheet.blocks {
        let selector = block.selector.as_str();
        if block.at_rules.is_empty() && selector == ":root" {
            collect(&mut out.light, block);
        } else if is_dark_media(block) && selector == SYSTEM_DARK_SELECTOR {
            collect(&mut out.dark_system, block);
        } else if block.at_rules.is_empty() && selector == EXPLICIT_DARK_SELECTOR {
            collect(&mut out.dark_explicit, block);
        }
    }
    out
}

/// Every primitive and the semantic tokens of one theme, unresolved.
pub fn tokens_for(ds: &DesignSet, theme: Theme) -> Tokens {
    let mut tokens = Tokens::new();
    for file in ds.sheets_in(Layer::Primitives) {
        for block in &file.sheet.blocks {
            collect(&mut tokens, block);
        }
    }
    let blocks = theme_blocks(ds);
    tokens.extend(blocks.light);
    if theme == Theme::Dark {
        tokens.extend(blocks.dark_explicit);
    }
    tokens
}

/// Replaces every `var(--x)` in a token's value until only literals are left.
pub fn resolve(tokens: &Tokens, name: &str) -> Option<String> {
    resolve_depth(tokens, name, 0)
}

fn resolve_depth(tokens: &Tokens, name: &str, depth: usize) -> Option<String> {
    const MAX_DEPTH: usize = 16; // a cycle, not a real chain
    if depth > MAX_DEPTH {
        return None;
    }
    let mut value = tokens.get(name)?.clone();
    for r in scanner::var_refs(&value) {
        let inner = resolve_depth(tokens, &r, depth + 1)?;
        value = value.replace(&format!("var({r})"), &inner);
    }
    Some(value)
}
