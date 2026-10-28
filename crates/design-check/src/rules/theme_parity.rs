//! Every themed token (`--c-*`, `--shadow-*`, `--scrim`) of the light theme has a dark value, in
//! both dark blocks, and the two dark blocks (system setting and explicit `data-theme="dark"`)
//! are identical. CSS cannot share one block between a media query and an attribute selector,
//! so it is written twice and kept equal here.

use super::Rule;
use crate::model::DesignSet;
use crate::report::Violation;
use crate::theme::{self, SEMANTIC};

pub struct ThemeParity;

impl Rule for ThemeParity {
    fn name(&self) -> &'static str {
        "theme-parity"
    }

    fn check(&self, ds: &DesignSet) -> Vec<Violation> {
        let blocks = theme::theme_blocks(ds);
        let mut out = Vec::new();
        let mut miss = |msg: String| out.push(Violation::new(self.name(), SEMANTIC, msg));
        for name in blocks.light.keys().filter(|n| theme::is_themed(n)) {
            for (label, dark) in [
                ("system dark", &blocks.dark_system),
                ("explicit dark", &blocks.dark_explicit),
            ] {
                if !dark.contains_key(name) {
                    miss(format!("{name} has no value in the {label} block"));
                }
            }
        }
        for (name, value) in &blocks.dark_system {
            match blocks.dark_explicit.get(name) {
                Some(other) if other == value => {}
                Some(other) => miss(format!(
                    "{name}: system dark is {value}, explicit dark is {other}"
                )),
                None => miss(format!("{name} is in the system dark block only")),
            }
        }
        for name in blocks
            .dark_explicit
            .keys()
            .filter(|n| !blocks.dark_system.contains_key(*n))
        {
            miss(format!("{name} is in the explicit dark block only"));
        }
        if blocks.light.is_empty() || blocks.dark_system.is_empty() {
            miss("no light or dark theme block found".into());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn semantic(dark_system: &str, dark_explicit: &str) -> DesignSet {
        let css = format!(
            ":root {{ --c-ink: a; --c-bg: b; --space-1: 4px; }}\n@media (prefers-color-scheme: dark) {{ :root:not([data-theme=\"light\"]) {{ {dark_system} }} }}\n:root[data-theme=\"dark\"] {{ {dark_explicit} }}"
        );
        DesignSet::from_parts(&[(SEMANTIC, css.as_str())], &[], &[])
    }

    #[test]
    fn matching_dark_blocks_pass() {
        assert!(
            ThemeParity
                .check(&semantic(
                    "--c-ink: x; --c-bg: y;",
                    "--c-ink: x; --c-bg: y;"
                ))
                .is_empty()
        );
    }

    #[test]
    fn a_missing_dark_value_fails() {
        let v = ThemeParity.check(&semantic("--c-ink: x;", "--c-ink: x;"));
        assert_eq!(v.len(), 2, "{v:?}");
    }

    #[test]
    fn diverging_dark_blocks_fail() {
        let v = ThemeParity.check(&semantic(
            "--c-ink: x; --c-bg: y;",
            "--c-ink: x; --c-bg: z;",
        ));
        assert_eq!(v.len(), 1, "{v:?}");
    }
}
