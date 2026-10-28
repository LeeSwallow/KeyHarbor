//! The layer table: which folder of design-system/ is which layer, and what each layer may do.
//! Rules read this table; changing what a layer may do is a change here, not in a rule.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Layer {
    Primitives,
    Semantic,
    Components,
    Patterns,
    Page,
    Entry,
}

/// What a layer may define as custom properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Defines {
    /// New design tokens.
    Tokens,
    /// Its own hooks (named after the file: `button.css` → `--button-*`), and other components' hooks.
    OwnHooks,
    /// Only hooks that a component already opened.
    ComponentHooks,
    /// Nothing.
    Nothing,
}

#[derive(Debug, Clone, Copy)]
pub struct LayerSpec {
    pub layer: Layer,
    /// Path inside design-system/: a file, or a folder ending in `/`.
    pub path: &'static str,
    /// Layers whose custom properties this layer may read with `var()`.
    pub may_reference: &'static [Layer],
    pub literal_colours: bool,
    /// Pixel lengths other than 0 and 1px.
    pub px_literals: bool,
    pub defines: Defines,
}

use Layer::*;

pub const LAYERS: &[LayerSpec] = &[
    LayerSpec {
        layer: Primitives,
        path: "tokens/primitives.css",
        may_reference: &[],
        literal_colours: true,
        px_literals: true,
        defines: Defines::Tokens,
    },
    LayerSpec {
        layer: Semantic,
        path: "tokens/semantic.css",
        may_reference: &[Primitives, Semantic],
        literal_colours: false,
        px_literals: true,
        defines: Defines::Tokens,
    },
    LayerSpec {
        layer: Components,
        path: "components/",
        may_reference: &[Semantic, Components],
        literal_colours: false,
        px_literals: false,
        defines: Defines::OwnHooks,
    },
    LayerSpec {
        layer: Patterns,
        path: "patterns/",
        may_reference: &[Semantic, Components],
        literal_colours: false,
        px_literals: true,
        defines: Defines::ComponentHooks,
    },
    LayerSpec {
        layer: Page,
        path: "showcase/",
        may_reference: &[Semantic, Components],
        literal_colours: false,
        px_literals: true,
        defines: Defines::ComponentHooks,
    },
    LayerSpec {
        layer: Entry,
        path: "keyharbor.css",
        may_reference: &[],
        literal_colours: false,
        px_literals: false,
        defines: Defines::Nothing,
    },
];

/// The entry stylesheet every page loads.
pub const ENTRY: &str = "keyharbor.css";

/// The layer of a stylesheet, by its path inside design-system/.
pub fn layer_of(path: &str) -> Option<&'static LayerSpec> {
    LAYERS.iter().find(|s| {
        if s.path.ends_with('/') {
            path.starts_with(s.path)
        } else {
            path == s.path
        }
    })
}

/// The hook prefix a component file owns: `components/icon-button.css` → `--icon-button`.
pub fn hook_prefix(path: &str) -> String {
    let stem = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".css");
    format!("--{stem}")
}

/// Whether `name` is one of the hooks `prefix` owns (`--panel` owns `--panel-pad`).
pub fn owns_hook(prefix: &str, name: &str) -> bool {
    name == prefix
        || name
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_map_to_layers() {
        assert_eq!(
            layer_of("tokens/primitives.css").map(|s| s.layer),
            Some(Primitives)
        );
        assert_eq!(
            layer_of("components/button.css").map(|s| s.layer),
            Some(Components)
        );
        assert_eq!(
            layer_of("patterns/app-shell.css").map(|s| s.layer),
            Some(Patterns)
        );
        assert!(layer_of("elsewhere.css").is_none());
    }

    #[test]
    fn hooks_are_named_after_the_file() {
        let p = hook_prefix("components/icon-button.css");
        assert_eq!(p, "--icon-button");
        assert!(owns_hook(&p, "--icon-button-bg"));
        assert!(!owns_hook(&hook_prefix("components/icon.css"), "--iconic"));
    }
}
