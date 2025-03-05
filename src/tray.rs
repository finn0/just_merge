use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, Runtime,
};

pub fn init_tray<R: Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<()> {
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "window", "Show Window", true, None::<&str>)?,
            &MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?,
        ],
    )?;

    let tray_icon = Image::from_bytes(include_bytes!("../icons/Square44x44Logo.png"))?;
    let _ = TrayIconBuilder::with_id("menu_extra")
        .icon(tray_icon)
        .icon_as_template(true)
        .menu(&menu)
        .on_menu_event(move |app, event| {
            match event.id().as_ref() {
                "quit" => app.exit(0),
                "window" => {
                    // if hidden or not focused, show and focus
                    if let Some(window) = app.get_webview_window("main") {
                        window.show().unwrap();
                        window.set_focus().unwrap();
                    }
                }
                _ => {}
            }
            // todo: Click 'X' not quit, but hide.
            // todo: collect log into mpsc, and send to main window
        })
        .build(app)?;

    Ok(())
}
