use tauri::WindowEvent;

mod tray;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Plugins
        .plugin(tauri_plugin_log::Builder::new().level(log::LevelFilter::Debug).build())
        // Window Event Override
        .on_window_event(|window, event| {
            // > 1. Only hide `main` window.
            // todo: will panic if `cmd+w`
            // >> thread 'main' panicked at /Users/finn/.cargo/registry/src/index.crates.io-6f17d22bba15001f/tao-0.32.7/src/platform_impl/macos/app.rs:43:19:
            // >> messsaging sendEvent: to nil
            if let ("main", WindowEvent::CloseRequested { api, .. }) = (window.label(), event) {
                window.hide().unwrap();
                api.prevent_close();
            }
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                tray::init_tray(app.handle()).unwrap();
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
