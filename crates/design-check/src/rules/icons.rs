//! Icons: every icon a page names exists, every SVG is a 24×24 currentColor stroke icon (so the
//! kh-i mask can paint it), the Lucide licence is there, and the showcase lists the whole folder.

use std::collections::BTreeSet;

use super::Rule;
use crate::model::DesignSet;
use crate::report::Violation;

pub struct Icons;

/// The ways a page names an icon: a path, or the `icon("name")` helper of the showcase and popup.
const REFERENCE_FORMS: &[(&str, &str)] = &[("icons/", ".svg"), ("icon(\"", "\""), ("icon('", "'")];
/// The showcase file whose `ICONS` list must cover the folder.
const ICON_LIST: &str = "design/showcase/sections/icons.js";

fn is_icon_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Icon names a text refers to.
pub fn referenced_icons(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (open, close) in REFERENCE_FORMS {
        let mut rest = text;
        while let Some(i) = rest.find(open) {
            let after = &rest[i + open.len()..];
            if let Some(end) = after.find(close)
                && is_icon_name(&after[..end])
            {
                out.insert(after[..end].to_string());
            }
            rest = after;
        }
    }
    out
}

/// The quoted names inside `ICONS = [ … ]`.
fn listed_icons(text: &str) -> BTreeSet<String> {
    let Some(start) = text.find("ICONS = [") else {
        return BTreeSet::new();
    };
    let list = &text[start..start + text[start..].find(']').unwrap_or(0)];
    list.split('"')
        .skip(1)
        .step_by(2)
        .filter(|s| is_icon_name(s))
        .map(str::to_string)
        .collect()
}

impl Rule for Icons {
    fn name(&self) -> &'static str {
        "icons"
    }

    fn check(&self, ds: &DesignSet) -> Vec<Violation> {
        let mut out = Vec::new();
        let have: BTreeSet<&str> = ds.icons.iter().map(|i| i.name.as_str()).collect();
        if !ds.has_icon_license {
            out.push(Violation::new(
                self.name(),
                "design/icons/LICENSE",
                "missing: the icons are Lucide (ISC) and ship with their licence",
            ));
        }
        for icon in &ds.icons {
            let path = format!("design/icons/{}.svg", icon.name);
            if !icon.svg.contains("viewBox=\"0 0 24 24\"") {
                out.push(Violation::new(
                    self.name(),
                    &path,
                    "viewBox must be 0 0 24 24",
                ));
            }
            if !icon.svg.contains("stroke=\"currentColor\"") {
                out.push(Violation::new(
                    self.name(),
                    &path,
                    "stroke must be currentColor",
                ));
            }
        }
        let css = ds.sheets.iter().flat_map(|f| {
            f.sheet
                .blocks
                .iter()
                .flat_map(|b| &b.decls)
                .map(move |d| (format!("design/{}", f.path), d.value.as_str()))
        });
        let texts = ds
            .consumers
            .iter()
            .map(|f| (f.path.clone(), f.text.as_str()))
            .chain(css);
        for (path, text) in texts {
            for name in referenced_icons(text) {
                if !have.contains(name.as_str()) {
                    out.push(Violation::new(
                        self.name(),
                        &path,
                        format!("icon {name} is not in design/icons/"),
                    ));
                }
            }
        }
        if let Some(list) = ds
            .consumers
            .iter()
            .find(|f| f.path == ICON_LIST)
            .map(|f| listed_icons(&f.text))
        {
            for name in have.iter().filter(|n| !list.contains(**n)) {
                out.push(Violation::new(
                    self.name(),
                    ICON_LIST,
                    format!("ICONS does not list {name}"),
                ));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCK: (&str, &str) = (
        "lock",
        r#"<svg viewBox="0 0 24 24" stroke="currentColor"></svg>"#,
    );

    #[test]
    fn finds_icon_references_in_all_forms() {
        let names = referenced_icons(
            r#"mask-image:url(icons/lock.svg) ${icon("key-round", "lg")} icon('x') icons/${n}.svg"#,
        );
        assert_eq!(
            names.into_iter().collect::<Vec<_>>(),
            ["key-round", "lock", "x"]
        );
    }

    #[test]
    fn missing_icons_and_bad_svgs_fail() {
        let ds = DesignSet::from_parts(
            &[],
            &[("design/index.html", "icons/nope.svg icons/lock.svg")],
            &[
                LOCK,
                ("bad", "<svg viewBox=\"0 0 16 16\" stroke=\"red\"></svg>"),
            ],
        );
        let v = Icons.check(&ds);
        assert_eq!(v.len(), 3, "{v:?}");
    }

    #[test]
    fn the_showcase_lists_every_icon() {
        let ds = DesignSet::from_parts(
            &[],
            &[(ICON_LIST, r#"export const ICONS = ["lock"];"#)],
            &[LOCK, ("key", LOCK.1)],
        );
        let v = Icons.check(&ds);
        assert_eq!(v.len(), 1, "{v:?}");
        assert!(v[0].message.contains("key"));
    }
}
