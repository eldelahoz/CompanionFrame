//! Game data shipped inside the app: `warframe-public-export-plus` tables and dictionaries,
//! and `@wfcd/items` categories, copied to `resources/game-data` by `scripts/copy-game-data.mjs`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use tauri::Manager;
use tauri::path::BaseDirectory;

/// Where `tauri.conf.json` `bundle.resources` puts the data (dev: next to the exe, build: the install dir).
pub const RESOURCE_DIR: &str = "game-data";
const WFCD_COMPONENTS: &str = "Components";

pub struct BundledData {
    root: PathBuf,
}

/// The `pnpm game-data` bundle for data-bound tests: `None` locally when it was never copied
/// (the test skips), a panic in CI where skipping would pass without checking anything.
#[cfg(test)]
pub(crate) fn test_root() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/game-data");
    if root.join("pep").exists() {
        return Some(root);
    }
    assert!(
        std::env::var_os("CI").is_none(),
        "game data missing at {}: run `pnpm game-data` before `cargo test`",
        root.display()
    );
    None
}

fn plain_name(s: &str, ok: impl Fn(char) -> bool) -> bool {
    !s.is_empty() && s.chars().all(ok)
}

fn is_export_name(s: &str) -> bool {
    s.starts_with("Export") && plain_name(s, |c| c.is_ascii_alphabetic())
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn read_object(path: &Path) -> Option<Map<String, Value>> {
    match read_json(path)? {
        Value::Object(m) => Some(m),
        _ => None,
    }
}

impl BundledData {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn from_app(app: &tauri::AppHandle) -> Result<Self, String> {
        let root = app
            .path()
            .resolve(RESOURCE_DIR, BaseDirectory::Resource)
            .map_err(|e| e.to_string())?;
        Ok(Self::new(root))
    }

    /// One table such as `ExportWeapons`; `None` when not shipped or not an object.
    pub fn export(&self, name: &str) -> Option<Map<String, Value>> {
        if !is_export_name(name) {
            return None;
        }
        read_object(&self.root.join("pep").join(format!("{name}.json")))
    }

    /// DE's string table for a language code (`en`, `de`, `zh`, `fr`, `es` are shipped).
    pub fn dict(&self, locale: &str) -> Option<Map<String, Value>> {
        if locale.len() != 2 || !locale.chars().all(|c| c.is_ascii_lowercase()) {
            return None;
        }
        read_object(&self.root.join("pep").join(format!("dict.{locale}.json")))
    }

    pub fn list_exports(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.root.join("pep"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter_map(|f| f.strip_suffix(".json").map(str::to_owned))
            .filter(|n| is_export_name(n))
            .collect();
        names.sort();
        names
    }

    pub fn wfcd_version(&self) -> Option<String> {
        read_json(&self.root.join("wfcd/package.json"))?
            .get("version")?
            .as_str()
            .map(str::to_owned)
    }

    fn wfcd_category(&self, category: &str) -> Result<Vec<Value>, String> {
        if !plain_name(category, |c| c.is_ascii_alphabetic() || c == '-') {
            return Ok(vec![]);
        }
        let path = self.root.join("wfcd").join(format!("{category}.json"));
        if !path.exists() {
            return Ok(vec![]);
        }
        match read_json(&path) {
            Some(Value::Array(items)) => Ok(items),
            _ => Err(format!("@wfcd/items {category}.json is not a list")),
        }
    }

    /// Every shipped item by uniqueName, `Components` first so other categories win.
    fn wfcd_catalog(&self) -> Result<BTreeMap<String, Value>, String> {
        let mut files: Vec<String> = fs::read_dir(self.root.join("wfcd"))
            .map_err(|e| e.to_string())?
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter_map(|f| f.strip_suffix(".json").map(str::to_owned))
            .filter(|c| c != "package" && c != WFCD_COMPONENTS)
            .collect();
        files.sort();
        files.insert(0, WFCD_COMPONENTS.into());
        let mut catalog = BTreeMap::new();
        for category in files {
            for item in self.wfcd_category(&category)? {
                if let Some(u) = item.get("uniqueName").and_then(Value::as_str) {
                    catalog.insert(u.to_owned(), item.clone());
                }
            }
        }
        Ok(catalog)
    }

    /// What `new Items({ category })` yields: categories concatenated with `Components` last,
    /// component refs resolved, then sorted by name and uniqueName.
    // shortcut: sorts by byte order, JS used ICU localeCompare; revisit if ordering drift shows up.
    pub fn wfcd_items(&self, categories: &[&str]) -> Result<Vec<Value>, String> {
        let mut ordered: Vec<&str> = categories
            .iter()
            .copied()
            .filter(|c| *c != WFCD_COMPONENTS)
            .collect();
        if categories.contains(&WFCD_COMPONENTS) {
            ordered.push(WFCD_COMPONENTS);
        }
        let mut items = Vec::new();
        for c in ordered {
            items.extend(self.wfcd_category(c)?);
        }
        // Since 1.1276 components are { uniqueName, itemCount } refs; a ref to a loaded item copies it.
        let mut catalog: Option<BTreeMap<String, Value>> = None;
        let loaded: Vec<(String, Value)> = items
            .iter()
            .filter_map(|i| Some((i.get("uniqueName")?.as_str()?.to_owned(), i.clone())))
            .collect();
        for item in &mut items {
            let Some(Value::Array(components)) = item.get_mut("components") else {
                continue;
            };
            for c in components.iter_mut() {
                let is_ref = c.get("uniqueName").is_some()
                    && ["name", "imageName", "drops"]
                        .iter()
                        .all(|k| c.get(k).is_none_or(|v| v.is_null() || v == ""));
                if !is_ref {
                    continue;
                }
                if catalog.is_none() {
                    let mut cat = self.wfcd_catalog()?;
                    cat.extend(loaded.iter().cloned());
                    catalog = Some(cat);
                }
                let Some(Value::Object(entry)) = catalog
                    .as_ref()
                    .and_then(|m| m.get(c["uniqueName"].as_str().unwrap_or_default()))
                else {
                    continue;
                };
                let mut resolved = entry.clone();
                resolved.remove("parentUniqueNames");
                let count = c.get("itemCount").filter(|v| v.is_number());
                resolved.insert("itemCount".into(), count.cloned().unwrap_or(json!(1)));
                *c = Value::Object(resolved);
            }
        }
        let key = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
        items.sort_by_key(|i| (key(i, "name"), key(i, "uniqueName")));
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tiny temp dir (no tempfile dependency in this crate).
    struct Dir(PathBuf);
    impl Dir {
        fn new() -> Self {
            static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let p = std::env::temp_dir().join(format!("cf-bundled-{}-{n}", std::process::id()));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn fixture() -> (Dir, BundledData) {
        let dir = Dir::new();
        let w = |rel: &str, v: Value| {
            let p = dir.0.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, v.to_string()).unwrap();
        };
        w(
            "pep/ExportWeapons.json",
            json!({"/W/A": {"name": "/Lang/A"}}),
        );
        w("pep/ExportRelics.json", json!({}));
        w("pep/dict.de.json", json!({"/Lang/A": "Ein"}));
        w("pep/notes.json", json!({}));
        w("wfcd/package.json", json!({"version": "1.2.3"}));
        w(
            "wfcd/Components.json",
            json!([{"uniqueName": "/C/Barrel", "name": "Barrel", "parentUniqueNames": ["/P"], "drops": [{"type": "x"}]}]),
        );
        w(
            "wfcd/Primary.json",
            json!([
                {"uniqueName": "/P/Z", "name": "Zed", "components": [{"uniqueName": "/C/Barrel", "itemCount": 2}, {"uniqueName": "/C/Missing"}]},
                {"uniqueName": "/P/A", "name": "Alpha"}
            ]),
        );
        let data = BundledData::new(&dir.0);
        (dir, data)
    }

    #[test]
    fn reads_tables_dicts_and_lists_only_export_tables() {
        let (_d, b) = fixture();
        assert!(b.export("ExportWeapons").unwrap().contains_key("/W/A"));
        assert!(b.export("ExportMissing").is_none());
        assert!(b.export("../wfcd/Primary").is_none());
        assert_eq!(b.dict("de").unwrap()["/Lang/A"], "Ein");
        assert!(b.dict("en").is_none() && b.dict("../x").is_none());
        assert_eq!(b.list_exports(), ["ExportRelics", "ExportWeapons"]);
        assert_eq!(b.wfcd_version().as_deref(), Some("1.2.3"));
    }

    #[test]
    fn wfcd_items_resolve_component_refs_sorted_by_name() {
        let (_d, b) = fixture();
        let items = b.wfcd_items(&["Primary", "Components"]).unwrap();
        let names: Vec<_> = items.iter().map(|i| i["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["Alpha", "Barrel", "Zed"]);
        let c = &items[2]["components"];
        assert_eq!(c[0]["name"], "Barrel");
        assert_eq!(c[0]["itemCount"], 2);
        assert!(c[0].get("parentUniqueNames").is_none());
        assert_eq!(c[0]["drops"][0]["type"], "x");
        // An unknown ref stays as it was.
        assert_eq!(c[1], json!({"uniqueName": "/C/Missing"}));
    }

    #[test]
    fn wfcd_items_ignore_bad_category_names_and_hand_out_copies() {
        let (_d, b) = fixture();
        assert!(b.wfcd_items(&["../Primary", "Nope"]).unwrap().is_empty());
        let mut first = b.wfcd_items(&["Primary"]).unwrap();
        first[0]["name"] = json!("changed");
        assert_eq!(b.wfcd_items(&["Primary"]).unwrap()[0]["name"], "Alpha");
    }

    /// Dev and `tauri build` both take resources from `bundle.resources`; keep it pointing at what the script writes.
    #[test]
    fn tauri_conf_bundles_the_generated_dir_under_the_runtime_name() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let conf = read_json(&manifest.join("tauri.conf.json")).unwrap();
        let resources = conf["bundle"]["resources"].as_object().unwrap();
        let (src, dest) = resources.iter().next().unwrap();
        assert_eq!(dest, &format!("{RESOURCE_DIR}/"));
        assert!(manifest.join(src).is_dir(), "{src} missing");
    }

    /// Runs against the real generated data when `pnpm game-data` has been run.
    #[test]
    fn generated_data_has_only_shipped_locales() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(RESOURCE_DIR);
        if !root.join("pep").is_dir() {
            return;
        }
        let b = BundledData::new(root);
        assert!(b.export("ExportWarframes").is_some_and(|m| !m.is_empty()));
        assert!(
            ["en", "de", "zh", "fr", "es"]
                .iter()
                .all(|l| b.dict(l).is_some())
        );
        assert!(b.dict("it").is_none());
        assert!(b.wfcd_version().is_some());
        assert!(!b.wfcd_items(&["Relics"]).unwrap().is_empty());
        assert!(b.wfcd_items(&["Enemy"]).unwrap().is_empty());
    }
}
