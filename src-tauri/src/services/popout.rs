//! "Open in window": a view or one layout section in its own webview window.
//! The window loads the main page with `?popout=<key>`; the frontend renders only that target.

use serde::Deserialize;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

const MAX_ID: usize = 64;
const MIN_SIZE: (f64, f64) = (360.0, 280.0);
const SIZE: (f64, f64) = (720.0, 600.0);

#[derive(Debug, Clone, PartialEq, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PopoutTarget {
    View {
        view: String,
    },
    Section {
        #[serde(rename = "sectionId")]
        section_id: String,
    },
}

fn is_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= MAX_ID && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// `(window label, popout key)`; the key is what the frontend parses back out of the URL.
fn identify(target: &PopoutTarget) -> Result<(String, String), String> {
    match target {
        PopoutTarget::View { view } if is_name(view) => {
            Ok((format!("popout-view-{view}"), format!("view:{view}")))
        }
        PopoutTarget::Section { section_id }
            if section_id
                .split_once('.')
                .is_some_and(|(v, n)| is_name(v) && is_name(n)) =>
        {
            Ok((
                format!("popout-section-{}", section_id.replace('.', "_")),
                format!("section:{section_id}"),
            ))
        }
        _ => Err("invalid popout target".into()),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn popout_open(app: AppHandle, target: PopoutTarget) -> Result<(), String> {
    let (label, key) = identify(&target)?;
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.unminimize();
        return win.set_focus().map_err(|e| e.to_string());
    }
    WebviewWindowBuilder::new(
        &app,
        label,
        WebviewUrl::App(format!("index.html?popout={key}").into()),
    )
    .title("CompanionFrame")
    .inner_size(SIZE.0, SIZE.1)
    .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
    .build()
    .map(|_| ())
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(v: &str) -> PopoutTarget {
        PopoutTarget::View { view: v.into() }
    }
    fn section(s: &str) -> PopoutTarget {
        PopoutTarget::Section {
            section_id: s.into(),
        }
    }

    #[test]
    fn identifies_valid_targets() {
        assert_eq!(
            identify(&view("world")).unwrap(),
            ("popout-view-world".into(), "view:world".into())
        );
        assert_eq!(
            identify(&section("world.fissures")).unwrap(),
            (
                "popout-section-world_fissures".into(),
                "section:world.fissures".into()
            )
        );
    }

    #[test]
    fn rejects_malformed_targets() {
        for t in [
            view(""),
            view("a b"),
            view("../x"),
            section("world"),
            section("world."),
            section("world.a.b"),
            section("world.a?b=1"),
            section(&format!("world.{}", "x".repeat(MAX_ID + 1))),
        ] {
            assert!(identify(&t).is_err(), "{t:?}");
        }
    }

    #[test]
    fn deserializes_tagged_json() {
        let t: PopoutTarget =
            serde_json::from_str(r#"{"kind":"section","sectionId":"world.baro"}"#).unwrap();
        assert_eq!(t, section("world.baro"));
    }
}
