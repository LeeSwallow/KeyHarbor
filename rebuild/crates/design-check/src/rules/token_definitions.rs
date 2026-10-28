//! Who may define custom properties: token files define tokens; a component defines hooks named
//! after its file (and may set another component's hooks); patterns and pages only set hooks a
//! component opened; the entry file defines nothing.

use super::Rule;
use crate::layers::{self, Defines, Layer};
use crate::model::DesignSet;
use crate::report::Violation;

pub struct TokenDefinitions;

impl Rule for TokenDefinitions {
    fn name(&self) -> &'static str {
        "token-definitions"
    }

    fn check(&self, ds: &DesignSet) -> Vec<Violation> {
        let owners = ds.owners();
        let is_hook = |name: &str| {
            owners
                .get(name)
                .is_some_and(|o| o.layer == Layer::Components)
        };
        let mut out = Vec::new();
        for file in &ds.sheets {
            let Some(spec) = layers::layer_of(&file.path) else {
                continue;
            };
            let prefix = layers::hook_prefix(&file.path);
            for decl in file.sheet.blocks.iter().flat_map(|b| b.custom_properties()) {
                let allowed = match spec.defines {
                    Defines::Tokens => true,
                    Defines::OwnHooks => {
                        layers::owns_hook(&prefix, &decl.name) || is_hook(&decl.name)
                    }
                    Defines::ComponentHooks => is_hook(&decl.name),
                    Defines::Nothing => false,
                };
                if !allowed {
                    let hint = match spec.defines {
                        Defines::OwnHooks => format!("a component's own hooks start with {prefix}"),
                        _ => "only a component hook may be set here; tokens belong in tokens/"
                            .to_string(),
                    };
                    out.push(
                        Violation::new(
                            self.name(),
                            &file.path,
                            format!("defines {}: {hint}", decl.name),
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

    const PANEL: (&str, &str) = (
        "components/panel.css",
        ".kh-panel { --panel-pad: 4px; padding: var(--panel-pad); }",
    );

    #[test]
    fn components_own_hooks_named_after_the_file() {
        let ds = DesignSet::from_parts(&[PANEL], &[], &[]);
        assert!(TokenDefinitions.check(&ds).is_empty());
        let bad = DesignSet::from_parts(
            &[("components/panel.css", ".kh-panel { --pad: 4px; }")],
            &[],
            &[],
        );
        assert_eq!(TokenDefinitions.check(&bad).len(), 1);
    }

    #[test]
    fn patterns_set_hooks_but_define_no_tokens() {
        let ok = DesignSet::from_parts(
            &[
                PANEL,
                (
                    "patterns/extension-popup.css",
                    ".kh-popup .kh-panel { --panel-pad: 2px; }",
                ),
            ],
            &[],
            &[],
        );
        assert!(TokenDefinitions.check(&ok).is_empty());
        let bad = DesignSet::from_parts(
            &[
                PANEL,
                (
                    "patterns/extension-popup.css",
                    ".kh-popup { --popup-gap: 2px; }",
                ),
            ],
            &[],
            &[],
        );
        assert_eq!(TokenDefinitions.check(&bad).len(), 1);
    }
}
