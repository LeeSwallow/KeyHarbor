//! Text colours meet WCAG 2.x contrast on the surfaces they are drawn on, in both themes.
//! 4.5:1 for text (WCAG 1.4.3 AA). `--c-ink-3` is secondary text — labels, meta, timestamps, set
//! no smaller than 11px semibold or 12px — and is held to 3:1, the non-text and large-text floor;
//! nothing a person must read to act is drawn in ink-3 alone. `--c-ink-4` (placeholder, disabled)
//! makes no promise: WCAG exempts disabled controls and placeholder text is never the label.

use super::Rule;
use crate::color::{self, Rgba};
use crate::model::DesignSet;
use crate::report::Violation;
use crate::theme::{self, SEMANTIC, Theme};

/// A text colour on a surface, the minimum ratio, and where the pair is used.
#[derive(Debug, Clone, Copy)]
pub struct ContrastPair {
    pub fg: &'static str,
    pub bg: &'static str,
    pub min: f64,
    pub used_for: &'static str,
}

const fn pair(
    fg: &'static str,
    bg: &'static str,
    min: f64,
    used_for: &'static str,
) -> ContrastPair {
    ContrastPair {
        fg,
        bg,
        min,
        used_for,
    }
}

const TEXT: f64 = 4.5;
const SECONDARY: f64 = 3.0;

pub const CONTRAST_PAIRS: &[ContrastPair] = &[
    pair("--c-ink", "--c-bg", TEXT, "body text on the window"),
    pair("--c-ink", "--c-surface", TEXT, "body text on panels"),
    pair(
        "--c-ink",
        "--c-surface-2",
        TEXT,
        "body text in lists and inputs",
    ),
    pair("--c-ink", "--c-surface-3", TEXT, "hovered row"),
    pair("--c-ink-2", "--c-bg", TEXT, "secondary body text"),
    pair("--c-ink-2", "--c-surface", TEXT, "ghost button, nav"),
    pair("--c-ink-2", "--c-surface-2", TEXT, "sidebar nav"),
    pair("--c-ink-3", "--c-bg", SECONDARY, "labels, meta"),
    pair("--c-ink-3", "--c-surface", SECONDARY, "labels, meta"),
    pair("--c-ink-3", "--c-surface-2", SECONDARY, "labels, meta"),
    pair(
        "--c-accent-text",
        "--c-accent-fill",
        TEXT,
        "primary button, count",
    ),
    pair(
        "--c-accent-text",
        "--c-accent-fill-hover",
        TEXT,
        "primary button, hover",
    ),
    pair(
        "--c-accent-strong",
        "--c-accent-wash",
        TEXT,
        "placeholder pill, pressed icon button",
    ),
    pair("--c-accent-strong", "--c-surface", TEXT, "toast action"),
    pair(
        "--c-danger-text",
        "--c-danger",
        TEXT,
        "danger button, tampered badge",
    ),
    pair("--c-ok", "--c-ok-wash", TEXT, "active badge"),
    pair("--c-warn", "--c-warn-wash", TEXT, "expiring badge"),
    pair(
        "--c-danger",
        "--c-danger-wash",
        TEXT,
        "expired, denied badge",
    ),
    pair("--c-info", "--c-info-wash", TEXT, "draft badge"),
    pair(
        "--c-dom-session",
        "--c-dom-session-wash",
        TEXT,
        "session chip",
    ),
    pair("--c-dom-cred", "--c-dom-cred-wash", TEXT, "credential chip"),
    pair("--c-dom-dev", "--c-dom-dev-wash", TEXT, "developer chip"),
    pair("--c-dom-pay", "--c-dom-pay-wash", TEXT, "payment chip"),
    pair(
        "--c-dom-personal",
        "--c-dom-personal-wash",
        TEXT,
        "personal chip",
    ),
    pair("--c-sealed-ink", "--c-sealed", TEXT, "sealed secret value"),
];

/// One pair measured in one theme.
#[derive(Debug, Clone, Copy)]
pub struct Measured {
    pub pair: ContrastPair,
    pub theme: Theme,
    /// `None` when a token does not resolve to a colour.
    pub ratio: Option<f64>,
}

/// Measures every pair in both themes.
pub fn measure(ds: &DesignSet) -> Vec<Measured> {
    let mut out = Vec::new();
    for theme in Theme::ALL {
        let tokens = theme::tokens_for(ds, theme);
        let colour =
            |name: &str| theme::resolve(&tokens, name).and_then(|v| color::parse_colour(&v));
        for &pair in CONTRAST_PAIRS {
            let ratio = match (colour(pair.fg), colour(pair.bg)) {
                // A translucent surface is measured over the window background.
                (Some(fg), Some(bg)) => {
                    let backdrop = colour("--c-bg").unwrap_or(Rgba::opaque(255.0, 255.0, 255.0));
                    let bg = bg.over(backdrop);
                    Some(color::contrast(fg.over(bg), bg))
                }
                _ => None,
            };
            out.push(Measured { pair, theme, ratio });
        }
    }
    out
}

pub struct Contrast;

impl Rule for Contrast {
    fn name(&self) -> &'static str {
        "contrast"
    }

    fn check(&self, ds: &DesignSet) -> Vec<Violation> {
        measure(ds)
            .into_iter()
            .filter_map(|m| {
                let p = m.pair;
                let what = format!("{} on {} ({}, {})", p.fg, p.bg, m.theme.name(), p.used_for);
                match m.ratio {
                    None => Some(format!("{what}: does not resolve to a colour")),
                    Some(r) if r < p.min => Some(format!("{what}: {r:.2}:1, needs {:.1}:1", p.min)),
                    Some(_) => None,
                }
            })
            .map(|msg| Violation::new(self.name(), SEMANTIC, msg))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_on_light_teal_fails_the_primary_button() {
        // The 2026-10 primary button: white on #05a8a8, 2.93:1.
        let white = color::parse_colour("#ffffff").unwrap();
        let teal = color::parse_colour("#05a8a8").unwrap();
        let r = color::contrast(white, teal);
        assert!(r < TEXT && (r - 2.93).abs() < 0.01, "{r}");
    }

    #[test]
    fn resolves_through_semantic_to_primitives() {
        let ds = DesignSet::from_parts(
            &[
                (
                    "tokens/primitives.css",
                    ":root { --white: #ffffff; --teal: #05a8a8; --deep: #047a7a; }",
                ),
                (
                    SEMANTIC,
                    ":root { --c-bg: var(--white); --c-accent-text: var(--white); --c-accent-fill: var(--teal); }\n:root[data-theme=\"dark\"] { --c-accent-fill: var(--deep); }",
                ),
            ],
            &[],
            &[],
        );
        let fill = measure(&ds)
            .into_iter()
            .filter(|m| m.pair.bg == "--c-accent-fill")
            .collect::<Vec<_>>();
        assert!(
            fill.iter()
                .any(|m| m.theme == Theme::Light && m.ratio.is_some_and(|r| r < TEXT))
        );
        assert!(
            fill.iter()
                .any(|m| m.theme == Theme::Dark && m.ratio.is_some_and(|r| r > TEXT))
        );
    }
}
