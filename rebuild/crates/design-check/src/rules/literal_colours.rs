//! Literal colours (`#hex`, `rgb()`, named colours) appear only in tokens/primitives.css.

use super::Rule;
use crate::layers;
use crate::model::DesignSet;
use crate::report::Violation;
use crate::scanner;

pub struct LiteralColours;

impl Rule for LiteralColours {
    fn name(&self) -> &'static str {
        "literal-colours"
    }

    fn check(&self, ds: &DesignSet) -> Vec<Violation> {
        let mut out = Vec::new();
        for file in &ds.sheets {
            if layers::layer_of(&file.path).is_none_or(|s| s.literal_colours) {
                continue;
            }
            for decl in file.sheet.blocks.iter().flat_map(|b| &b.decls) {
                for colour in scanner::literal_colours(&decl.value) {
                    out.push(
                        Violation::new(
                            self.name(),
                            &file.path,
                            format!(
                                "{}: literal colour {colour}; add a primitive and a semantic token",
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
    fn primitives_may_hold_colours() {
        let ds = DesignSet::from_parts(
            &[(
                "tokens/primitives.css",
                ":root { --a: #fff; --b: rgba(0,0,0,.3); }",
            )],
            &[],
            &[],
        );
        assert!(LiteralColours.check(&ds).is_empty());
    }

    #[test]
    fn components_and_semantic_may_not() {
        let ds = DesignSet::from_parts(
            &[
                ("tokens/semantic.css", ":root { --c-x: #fff; }"),
                ("components/toggle.css", ".kh-toggle { background: white; }"),
            ],
            &[],
            &[],
        );
        assert_eq!(LiteralColours.check(&ds).len(), 2);
    }
}
