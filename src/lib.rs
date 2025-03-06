use log::info;
use tauri::WindowEvent;

mod tray;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Plugins
        .plugin(tauri_plugin_log::Builder::new().level(log::LevelFilter::Debug).build())
        // Invoke Handler
        .invoke_handler(tauri::generate_handler![process_mr])
        // Window Event Override
        .on_window_event(|window, event| {
            // > 1. Only hide `main` window.
            // > 2. todo: will panic if `cmd+w`
            // https://github.com/tauri-apps/tauri/issues/12888
            // https://github.com/tauri-apps/tao/issues/1086
            if let ("main", WindowEvent::CloseRequested { api, .. }) = (window.label(), event) {
                window.hide().unwrap();
                api.prevent_close();
            }
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            {
                tray::init_tray(app.handle()).unwrap();
                // app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[tauri::command(rename_all = "snake_case")]
fn process_mr(url: String) -> String {
    info!("Processing Merge Request URL: {}", url);
    format!("Processing Merge Request URL: {}", url)
}
