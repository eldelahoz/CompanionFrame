use tauri::menu::{MenuBuilder, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, Manager};

pub fn setup(app: &App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show/Hide", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = MenuBuilder::new(app).items(&[&show, &quit]).build()?;
    let mut tray = TrayIconBuilder::new()
        .tooltip("CompanionFrame")
        .menu(&menu)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    if w.is_visible().unwrap_or(false) {
                        let _ = w.hide();
                    } else {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
            }
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}
