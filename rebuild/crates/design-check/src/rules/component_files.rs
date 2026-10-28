//! Every stylesheet is in a layer, keyharbor.css imports each one that exists (and nothing that
//! does not), and every component or pattern file styles a class some page actually uses.

use super::Rule;
use crate::layers::{ENTRY, Layer};
use crate::model::DesignSet;
use crate::report::Violation;

pub struct ComponentFiles;

const IMPORTED: &[Layer] = &[
    Layer::Primitives,
    Layer::Semantic,
    Layer::Components,
    Layer::Patterns,
];

impl Rule for ComponentFiles {
    fn name(&self) -> &'static str {
        "component-files"
    }

    fn check(&self, ds: &DesignSet) -> Vec<Violation> {
        let mut out: Vec<Violation> = ds
            .unlayered
            .iter()
            .map(|p| Violation::new(self.name(), p, "stylesheet outside every layer folder"))
            .collect();
        let imports = ds
            .sheet(ENTRY)
            .map(|e| e.sheet.imports.clone())
            .unwrap_or_default();
        for import in &imports {
            if ds.sheet(import).is_none() {
                out.push(Violation::new(
                    self.name(),
                    ENTRY,
                    format!("imports {import}, which does not exist"),
                ));
            }
        }
        let mut used = ds.used_kh_names();
        if ds
            .consumers
            .iter()
            .any(|f| f.text.contains("class=\"kh ") || f.text.contains("class=\"kh\""))
        {
            used.insert("kh".into(), String::new());
        }
        for file in ds.sheets.iter().filter(|f| IMPORTED.contains(&f.layer)) {
            if !imports.contains(&file.path) {
                out.push(Violation::new(
                    self.name(),
                    &file.path,
                    format!("not imported by {ENTRY}"),
                ));
            }
            if matches!(file.layer, Layer::Components | Layer::Patterns)
                && !DesignSet::styled_classes(file)
                    .iter()
                    .any(|c| used.contains_key(c))
            {
                out.push(Violation::new(
                    self.name(),
                    &file.path,
                    "no page uses any class this file styles; remove it or show it in the showcase",
                ));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unused_and_unimported_files_fail() {
        let ds = DesignSet::from_parts(
            &[
                (
                    "keyharbor.css",
                    "@import url(\"components/row.css\"); @import url(\"components/gone.css\");",
                ),
                ("components/row.css", ".kh-row { color: red; }"),
                ("components/ghost.css", ".kh-ghost { color: red; }"),
            ],
            &[("design/index.html", "<div class=\"kh-row\"></div>")],
            &[],
        );
        let v: Vec<String> = ComponentFiles
            .check(&ds)
            .into_iter()
            .map(|v| v.to_string())
            .collect();
        assert_eq!(v.len(), 3, "{v:?}");
        assert!(v.iter().any(|m| m.contains("gone.css")));
        assert!(
            v.iter()
                .any(|m| m.contains("ghost.css") && m.contains("not imported"))
        );
        assert!(
            v.iter()
                .any(|m| m.contains("ghost.css") && m.contains("no page uses"))
        );
    }
}
