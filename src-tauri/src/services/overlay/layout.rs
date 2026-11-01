//! Overlay layouts the frontend saves, kept as JSON text in the settings `Store`. The frontend owns the
//! shape (`config/shared/overlayLayout`); here only the key and a sane size/JSON-object check.
use tauri::{AppHandle, Emitter, State};

use crate::store::Store;

const MAX_BYTES: usize = 64 * 1024;

/// `reward` is the card layout; `offset:<kind>` is the drag offset of a whole overlay window.
fn valid_key(key: &str) -> bool {
    let kinds = ["reward", "riven", "arbi", "trade"];
    key == "reward"
        || key
            .strip_prefix("offset:")
            .is_some_and(|k| kinds.contains(&k))
}

fn check(key: &str, json: &str) -> Result<(), String> {
    if !valid_key(key) {
        return Err(format!("unknown layout key {key}"));
    }
    if json.len() > MAX_BYTES {
        return Err("layout too large".into());
    }
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(v) if v.is_object() => Ok(()),
        _ => Err("layout must be a JSON object".into()),
    }
}

fn save(store: &Store, key: &str, json: Option<String>) -> Result<(), String> {
    if !valid_key(key) {
        return Err(format!("unknown layout key {key}"));
    }
    let mut s = store.get();
    match json {
        Some(j) => {
            check(key, &j)?;
            s.overlay_layouts.insert(key.into(), j);
        }
        None => {
            s.overlay_layouts.remove(key);
        }
    }
    store.set(s).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn overlay_layout_get(store: State<Store>, key: String) -> Option<String> {
    store.get().overlay_layouts.get(&key).cloned()
}

#[tauri::command]
#[specta::specta]
pub fn overlay_layout_save(store: State<Store>, key: String, json: String) -> Result<(), String> {
    save(&store, &key, Some(json))
}

/// Payload is the layout key that was reset.
pub const RESET_EVENT: &str = "overlay-layout-reset";

/// "Reset" forgets the saved layout so the defaults apply again.
#[tauri::command]
#[specta::specta]
pub fn overlay_layout_reset(
    app: AppHandle,
    store: State<Store>,
    key: String,
) -> Result<(), String> {
    save(&store, &key, None)?;
    // Open overlays hold their offset in memory; tell them to drop it.
    let _ = app.emit(RESET_EVENT, key);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (Store, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "cf-layout-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        (Store::load(dir.join("settings.json")), dir)
    }

    #[test]
    fn saves_survive_a_restart_and_reset_forgets() {
        let (st, dir) = store();
        save(&st, "reward", Some(r#"{"version":1,"fields":{}}"#.into())).unwrap();
        save(&st, "offset:arbi", Some(r#"{"x":3,"y":4}"#.into())).unwrap();
        let again = Store::load(dir.join("settings.json"));
        assert_eq!(again.get().overlay_layouts.len(), 2);
        save(&again, "reward", None).unwrap();
        assert!(
            !Store::load(dir.join("settings.json"))
                .get()
                .overlay_layouts
                .contains_key("reward")
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_bad_keys_and_payloads() {
        let (st, dir) = store();
        assert!(save(&st, "planner", Some("{}".into())).is_err());
        assert!(save(&st, "offset:nope", None).is_err());
        assert!(save(&st, "reward", Some("[1]".into())).is_err());
        assert!(save(&st, "reward", Some("not json".into())).is_err());
        assert!(
            save(
                &st,
                "reward",
                Some(format!(r#"{{"a":"{}"}}"#, "x".repeat(MAX_BYTES)))
            )
            .is_err()
        );
        assert!(st.get().overlay_layouts.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
