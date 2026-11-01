//! Game-memory reading is opt-in (docs/ARCHITECTURE.md, "Security"): the OS read APIs live only in
//! the Windows process module, and its opener is reached only through `game_memory::open_game`,
//! which checks `memory_gate`.

use std::path::{Path, PathBuf};

fn scan(dir: &Path, needle: &str, hits: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan(&path, needle, hits);
        } else if std::fs::read_to_string(&path).is_ok_and(|t| t.contains(needle)) {
            hits.push(path);
        }
    }
}

fn files_with(needle: &str) -> Vec<String> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut hits = Vec::new();
    scan(&src, needle, &mut hits);
    let mut out: Vec<String> = hits
        .iter()
        .map(|p| {
            p.strip_prefix(&src)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    out.sort();
    out
}

#[test]
fn memory_read_apis_appear_only_in_the_gated_module() {
    // Built apart so this file does not trip itself.
    for needle in [
        ["ReadProcess", "Memory"].concat(),
        ["PROCESS_VM", "_READ"].concat(),
    ] {
        assert_eq!(
            files_with(&needle),
            ["platform/windows/process.rs"],
            "{needle}"
        );
    }
}

#[test]
fn the_process_opener_is_called_only_behind_the_gate() {
    assert_eq!(
        files_with(&["open_", "warframe("].concat()),
        ["platform/windows/process.rs", "services/game_memory.rs"]
    );
    let game_memory = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/services/game_memory.rs"),
    )
    .unwrap();
    let opener = &game_memory[game_memory.find("fn open_game()").expect("open_game")..];
    let gate = opener.find("memory_gate::allowed()").expect("gate check");
    let call = opener.find("open_warframe(").expect("platform call");
    assert!(gate < call, "open_game must check the gate before opening");
}
