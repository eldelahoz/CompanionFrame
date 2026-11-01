use tauri::menu::{MenuBuilder, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Manager, Wry};

/// Menu labels (show/hide, quit) for an app language; unknown codes read English.
fn labels(language: &str) -> (&'static str, &'static str) {
    match language {
        "es" => ("Mostrar/Ocultar", "Salir"),
        "fr" => ("Afficher/Masquer", "Quitter"),
        "de" => ("Anzeigen/Verbergen", "Beenden"),
        "zh" => ("显示/隐藏", "退出"),
        _ => ("Show/Hide", "Quit"),
    }
}

/// The menu items, kept so a language change can retitle them.
struct TrayItems {
    show: MenuItem<Wry>,
    quit: MenuItem<Wry>,
}

/// Retitles the tray menu in the app `language`.
pub fn set_language(app: &AppHandle, language: &str) {
    if let Some(items) = app.try_state::<TrayItems>() {
        let (show, quit) = labels(language);
        let _ = items.show.set_text(show);
        let _ = items.quit.set_text(quit);
    }
}

fn toggle_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
}

pub fn setup(app: &App) -> tauri::Result<()> {
    let language = app.state::<crate::store::Store>().get().language;
    let (show_label, quit_label) = labels(&language);
    let show = MenuItem::with_id(app, "show", show_label, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", quit_label, true, None::<&str>)?;
    let menu = MenuBuilder::new(app).items(&[&show, &quit]).build()?;
    app.manage(TrayItems { show, quit });
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("CompanionFrame")
        .menu(&menu)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "show" => toggle_main(app),
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::labels;

    #[test]
    fn labels_follow_the_language_and_fall_back_to_english() {
        assert_eq!(labels("es"), ("Mostrar/Ocultar", "Salir"));
        assert_eq!(labels("fr").1, "Quitter");
        assert_eq!(labels("de").1, "Beenden");
        assert_eq!(labels("zh").1, "退出");
        assert_eq!(labels("en"), ("Show/Hide", "Quit"));
        assert_eq!(labels("xx"), labels("en"));
    }
}
