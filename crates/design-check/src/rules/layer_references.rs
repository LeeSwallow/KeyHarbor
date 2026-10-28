//! A layer reads only the layers the table allows: primitives read nothing, semantic reads
//! primitives, components and patterns read semantic tokens and component hooks — never primitives.

use super::Rule;
use crate::layers;
use crate::model::DesignSet;
use crate::report::Violation;
use crate::scanner;

pub struct LayerReferences;

impl Rule for LayerReferences {
    fn name(&self) -> &'static str {
        "layer-references"
    }

    fn check(&self, ds: &DesignSet) -> Vec<Violation> {
        let owners = ds.owners();
        let mut out = Vec::new();
        for file in &ds.sheets {
            let Some(spec) = layers::layer_of(&file.path) else {
                continue;
            };
            for decl in file.sheet.blocks.iter().flat_map(|b| &b.decls) {
                for name in scanner::var_refs(&decl.value) {
                    // Undefined names are the defined-vars rule's business.
                    let Some(owner) = owners.get(&name) else {
                        continue;
                    };
                    if !spec.may_reference.contains(&owner.layer) {
                        out.push(
                            Violation::new(
                                self.name(),
                                &file.path,
                                format!(
                                    "{name} belongs to {:?} ({}); {:?} may read {:?}",
                                    owner.layer, owner.file, spec.layer, spec.may_reference
                                ),
                            )
                            .at(decl.line),
                        );
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIM: (&str, &str) = ("tokens/primitives.css", ":root { --teal-500: #05a8a8; }");
    const SEM: (&str, &str) = (
        "tokens/semantic.css",
        ":root { --c-accent: var(--teal-500); }",
    );

    #[test]
    fn components_may_read_semantic_tokens() {
        let ds = DesignSet::from_parts(
            &[
                PRIM,
                SEM,
                (
                    "components/button.css",
                    ".kh-btn { color: var(--c-accent); }",
                ),
            ],
            &[],
            &[],
        );
        assert!(LayerReferences.check(&ds).is_empty());
    }

    #[test]
    fn components_may_not_read_primitives() {
        let ds = DesignSet::from_parts(
            &[
                PRIM,
                SEM,
                (
                    "components/button.css",
                    ".kh-btn { color: var(--teal-500); }",
                ),
            ],
            &[],
            &[],
        );
        let v = LayerReferences.check(&ds);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].message.contains("--teal-500"));
    }

    #[test]
    fn primitives_read_nothing() {
        let ds = DesignSet::from_parts(
            &[(
                "tokens/primitives.css",
                ":root { --a: #fff; --b: var(--a); }",
            )],
            &[],
            &[],
        );
        assert_eq!(LayerReferences.check(&ds).len(), 1);
    }
}
