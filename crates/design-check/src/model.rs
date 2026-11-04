//! `DesignSet`: everything the rules look at, read once. Stylesheets with their layer,
//! the pages and scripts that use the system ("consumers"), and the icons.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::layers::{self, Layer, LayerSpec};
use crate::scanner::{self, Sheet};
use crate::{DESIGN_ROOT, ICONS_DIR};

#[derive(Debug, Clone)]
pub struct SheetFile {
    /// Path inside design-system/.
    pub path: String,
    /// The row of the layer table this stylesheet falls under.
    pub spec: &'static LayerSpec,
    pub sheet: Sheet,
}

impl SheetFile {
    pub fn layer(&self) -> Layer {
        self.spec.layer
    }
}

#[derive(Debug, Clone)]
pub struct TextFile {
    /// Path from the repository root.
    pub path: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Icon {
    pub name: String,
    pub svg: String,
}

/// Who owns a custom property: the layer and file that define it first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owner {
    pub layer: Layer,
    pub file: String,
}

#[derive(Debug, Clone, Default)]
pub struct DesignSet {
    pub sheets: Vec<SheetFile>,
    pub consumers: Vec<TextFile>,
    pub icons: Vec<Icon>,
    pub has_icon_license: bool,
    /// Stylesheets found under design-system/ that no layer claims.
    pub unlayered: Vec<String>,
}

/// Where consumers live, from the repository root: (folder, extensions).
const CONSUMERS: &[(&str, &[&str])] = &[
    (concat!(design_root!(), "/index.html"), &[]),
    (concat!(design_root!(), "/showcase"), &["js"]),
    ("integrations/browser-extension/src", &["ts", "html"]),
];

/// A file whose name holds this is a test (`view.test.ts`), not a consumer.
const TEST_MARKER: &str = ".test.";

impl DesignSet {
    /// Reads design-system/ and its consumers from a repository checkout.
    pub fn load(repo: &Path) -> io::Result<Self> {
        let mut set = DesignSet::default();
        set.load_sheets(&repo.join(DESIGN_ROOT))?;
        set.load_consumers(repo)?;
        set.load_icons(&repo.join(ICONS_DIR))?;
        Ok(set)
    }

    /// Every stylesheet under `design`, into its layer or into `unlayered`.
    fn load_sheets(&mut self, design: &Path) -> io::Result<()> {
        for path in walk(design)? {
            let rel = relative(design, &path);
            if !rel.ends_with(".css") {
                continue;
            }
            match layers::layer_of(&rel) {
                Some(spec) => self.sheets.push(SheetFile {
                    spec,
                    sheet: scanner::parse_sheet(&fs::read_to_string(&path)?),
                    path: rel,
                }),
                None => self.unlayered.push(rel),
            }
        }
        Ok(())
    }

    /// The pages and scripts listed in `CONSUMERS`, test files left out.
    fn load_consumers(&mut self, repo: &Path) -> io::Result<()> {
        for (place, exts) in CONSUMERS {
            let base = repo.join(place);
            let files = if base.is_file() {
                vec![base]
            } else {
                walk(&base)?
            };
            for path in files {
                let rel = relative(repo, &path);
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                let wanted = exts.is_empty() || exts.contains(&ext);
                if wanted && !rel.contains(TEST_MARKER) {
                    self.consumers.push(TextFile {
                        text: fs::read_to_string(&path)?,
                        path: rel,
                    });
                }
            }
        }
        Ok(())
    }

    /// The SVGs in `icons` and whether their licence is there.
    fn load_icons(&mut self, icons: &Path) -> io::Result<()> {
        for path in walk(icons)? {
            if path.extension().is_some_and(|e| e == "svg") {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                self.icons.push(Icon {
                    name,
                    svg: fs::read_to_string(&path)?,
                });
            }
        }
        self.has_icon_license = icons.join("LICENSE").is_file();
        Ok(())
    }

    /// Builds a set from strings, for rule tests: sheets by design-system/ path, consumers by repo path.
    #[cfg(test)]
    pub(crate) fn from_parts(
        sheets: &[(&str, &str)],
        consumers: &[(&str, &str)],
        icons: &[(&str, &str)],
    ) -> Self {
        DesignSet {
            sheets: sheets
                .iter()
                .map(|(path, css)| SheetFile {
                    path: path.to_string(),
                    spec: layers::layer_of(path).expect("test sheet path must be layered"),
                    sheet: scanner::parse_sheet(css),
                })
                .collect(),
            consumers: consumers
                .iter()
                .map(|(p, t)| TextFile {
                    path: p.to_string(),
                    text: t.to_string(),
                })
                .collect(),
            icons: icons
                .iter()
                .map(|(n, s)| Icon {
                    name: n.to_string(),
                    svg: s.to_string(),
                })
                .collect(),
            has_icon_license: true,
            unlayered: vec![],
        }
    }

    pub fn sheets_in(&self, layer: Layer) -> impl Iterator<Item = &SheetFile> {
        self.sheets.iter().filter(move |s| s.layer() == layer)
    }

    pub fn sheet(&self, path: &str) -> Option<&SheetFile> {
        self.sheets.iter().find(|s| s.path == path)
    }

    /// `kh-*` names the consumers use, with the file that uses each first.
    pub fn used_kh_names(&self) -> BTreeMap<String, String> {
        let mut used = BTreeMap::new();
        for file in &self.consumers {
            for name in scanner::kh_names(&file.text) {
                used.entry(name).or_insert_with(|| file.path.clone());
            }
        }
        used
    }

    /// `kh-*` classes a stylesheet styles.
    pub fn styled_classes(file: &SheetFile) -> BTreeSet<String> {
        file.sheet
            .blocks
            .iter()
            .flat_map(|b| scanner::selector_classes(&b.selector))
            .collect()
    }

    /// Who owns each custom property. Tokens belong to the token file that defines them;
    /// a hook belongs to the component whose file name it starts with.
    pub fn owners(&self) -> BTreeMap<String, Owner> {
        let mut owners = BTreeMap::new();
        for file in &self.sheets {
            let prefix = layers::hook_prefix(&file.path);
            for decl in file.sheet.custom_properties() {
                let owns = match file.layer() {
                    Layer::Primitives | Layer::Semantic => true,
                    Layer::Components => layers::owns_hook(&prefix, &decl.name),
                    _ => false,
                };
                if owns {
                    owners.entry(decl.name.clone()).or_insert(Owner {
                        layer: file.layer(),
                        file: file.path.clone(),
                    });
                }
            }
        }
        owners
    }
}

fn walk(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return Ok(out);
    }
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path)?);
        } else {
            out.push(path);
        }
    }
    Ok(out)
}

fn relative(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
