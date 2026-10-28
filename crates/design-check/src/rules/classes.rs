//! Every `kh-*` class a page or script uses (showcase, extension popup) is styled by a
//! component or pattern. A misspelt class otherwise renders unstyled without a word.

use std::collections::BTreeSet;

use super::Rule;
use crate::layers::Layer;
use crate::model::DesignSet;
use crate::report::Violation;

pub struct Classes;

/// `kh-*` names that are not classes: keyframes and the like.
const NOT_CLASSES: &[&str] = &["kh-spin"];

impl Rule for Classes {
    fn name(&self) -> &'static str {
        "classes"
    }

    fn check(&self, ds: &DesignSet) -> Vec<Violation> {
        let styled: BTreeSet<String> = ds
            .sheets
            .iter()
            .filter(|f| matches!(f.layer, Layer::Components | Layer::Patterns))
            .flat_map(DesignSet::styled_classes)
            .collect();
        ds.used_kh_names()
            .into_iter()
            .filter(|(name, _)| !styled.contains(name) && !NOT_CLASSES.contains(&name.as_str()))
            .map(|(name, file)| {
                Violation::new(
                    self.name(),
                    file,
                    format!("class {name} is not styled by any component or pattern"),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_classes_fail() {
        let ds = DesignSet::from_parts(
            &[("components/row.css", ".kh-row .title { color: red; }")],
            &[(
                "design/index.html",
                "<div class=\"kh-row\"><b class=\"kh-rwo\"></b></div>",
            )],
            &[],
        );
        let v = Classes.check(&ds);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].message.contains("kh-rwo"));
    }
}
