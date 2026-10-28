//! Components take every size from a token. Only `0` and a `1px` hairline may be written as is.

use super::Rule;
use crate::layers;
use crate::model::DesignSet;
use crate::report::Violation;
use crate::scanner;

pub struct PxLiterals;

impl Rule for PxLiterals {
    fn name(&self) -> &'static str {
        "px-literals"
    }

    fn check(&self, ds: &DesignSet) -> Vec<Violation> {
        let mut out = Vec::new();
        for file in &ds.sheets {
            if layers::layer_of(&file.path).is_none_or(|s| s.px_literals) {
                continue;
            }
            for decl in file.sheet.blocks.iter().flat_map(|b| &b.decls) {
                for px in scanner::px_literals(&decl.value) {
                    out.push(
                        Violation::new(
                            self.name(),
                            &file.path,
                            format!(
                                "{}: {px}; use a size token from tokens/semantic.css",
                                decl.name
                            ),
                        )
                        .at(decl.line),
                    );
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hairlines_and_tokens_pass() {
        let ds = DesignSet::from_parts(
            &[(
                "components/panel.css",
                ".kh-panel { border: 1px solid var(--c-line); padding: var(--space-3); margin: 0; }",
            )],
            &[],
            &[],
        );
        assert!(PxLiterals.check(&ds).is_empty());
    }

    #[test]
    fn other_pixel_values_fail() {
        let ds = DesignSet::from_parts(
            &[("components/panel.css", ".kh-panel { width: 380px; }")],
            &[],
            &[],
        );
        assert_eq!(PxLiterals.check(&ds).len(), 1);
    }

    #[test]
    fn patterns_may_set_layout_sizes() {
        let ds = DesignSet::from_parts(
            &[("patterns/app-shell.css", ".kh-shell { height: 460px; }")],
            &[],
            &[],
        );
        assert!(PxLiterals.check(&ds).is_empty());
    }
}
