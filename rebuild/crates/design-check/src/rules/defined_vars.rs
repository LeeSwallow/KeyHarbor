//! Every `var(--x)` — in a stylesheet, an inline style or a script — names a property some
//! stylesheet defines. A typo in a token name otherwise fails silently to the initial value.

use std::collections::BTreeSet;

use super::Rule;
use crate::model::DesignSet;
use crate::report::Violation;
use crate::scanner;

pub struct DefinedVars;

impl Rule for DefinedVars {
    fn name(&self) -> &'static str {
        "defined-vars"
    }

    fn check(&self, ds: &DesignSet) -> Vec<Violation> {
        let defined: BTreeSet<&str> = ds
            .sheets
            .iter()
            .flat_map(|f| f.sheet.blocks.iter().flat_map(|b| b.custom_properties()))
            .map(|d| d.name.as_str())
            .collect();
        let mut out = Vec::new();
        for file in &ds.sheets {
            for decl in file.sheet.blocks.iter().flat_map(|b| &b.decls) {
                for name in scanner::var_refs(&decl.value) {
                    if !defined.contains(name.as_str()) {
                        out.push(
                            Violation::new(
                                self.name(),
                                &file.path,
                                format!("{name} is not defined"),
                            )
                            .at(decl.line),
                        );
                    }
                }
            }
        }
        for file in &ds.consumers {
            for (i, line) in file.text.lines().enumerate() {
                for name in scanner::var_refs(line) {
                    if !defined.contains(name.as_str()) {
                        out.push(
                            Violation::new(
                                self.name(),
                                &file.path,
                                format!("{name} is not defined"),
                            )
                            .at(i + 1),
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

    #[test]
    fn reports_undefined_names_in_css_and_pages() {
        let ds = DesignSet::from_parts(
            &[
                ("tokens/semantic.css", ":root { --c-ink: x; }"),
                (
                    "components/row.css",
                    ".kh-row { color: var(--c-ink); background: var(--c-nope); }",
                ),
            ],
            &[(
                "design/index.html",
                "<i style=\"color:var(--c-ink2)\"></i> <b style=\"font:var(--text-${n})\"></b>",
            )],
            &[],
        );
        let v = DefinedVars.check(&ds);
        assert_eq!(v.len(), 2, "{v:?}");
    }
}
