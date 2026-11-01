//! CompanionFrame has no backend of its own and uses none of WFHelper's.

use std::path::Path;

fn scan(dir: &Path, needle: &str, hits: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan(&path, needle, hits);
        } else {
            scan_file(&path, needle, hits);
        }
    }
}

#[test]
fn no_source_file_names_a_wfhelper_host() {
    // Built apart so this file does not trip itself.
    let needle = ["wfhelper", ".com"].concat();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut hits = Vec::new();
    for dir in ["src", "../src", "../config", "capabilities"] {
        scan(&root.join(dir), &needle, &mut hits);
    }
    scan_file(&root.join("tauri.conf.json"), &needle, &mut hits);
    assert!(hits.is_empty(), "{needle} in: {hits:#?}");
}

fn scan_file(path: &Path, needle: &str, hits: &mut Vec<String>) {
    if std::fs::read_to_string(path).is_ok_and(|t| t.to_lowercase().contains(needle)) {
        hits.push(path.display().to_string());
    }
}
