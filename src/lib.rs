use crossbeam::channel::{bounded, Receiver, Sender};
use inline_colorization::{color_bright_black, color_green, color_red, color_reset, color_white, color_yellow};
use log::{debug, error, info, warn, Level};
use tauri::{async_runtime::RwLock, AppHandle, Emitter, Manager, WindowEvent};

mod pubsub;
mod tray;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // - Static Plugins
        // - Invoke Handler
        .invoke_handler(tauri::generate_handler![push_log, request_merge_approval])
        // - Window Event Override
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
        // - Dynamic Setups
        .setup(|app| {
            // 1. Tray menu
            #[cfg(target_os = "macos")]
            {
                tray::init_tray(app.handle()).unwrap();
                // app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            // 2. State management
            let (tx, rx) = bounded(10);
            let app_data = AppData::new(tx.clone(), rx);
            app.manage(RwLock::const_new(app_data));

            // 3. Logs and log stream
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::new()
                        .level(log::LevelFilter::Debug)
                        .target(tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout))
                        .format(move |out, msg, record| {
                            let now = chrono::Local::now();
                            let date_str = now.date_naive();
                            let time_str = now.time().format("%H:%M:%S");

                            let the_level = record.level();
                            let color = match the_level {
                                Level::Debug => color_bright_black,
                                Level::Info => color_white,
                                Level::Warn => color_yellow,
                                Level::Error => color_red,
                                _ => color_green,
                            };

                            tx.send(format!(
                                "{color}[{}][{}][{}] {}{color_reset}",
                                date_str, time_str, the_level, msg
                            ))
                            .unwrap();

                            out.finish(format_args!(
                                "{color}[{}][{}][{}] {}{color_reset}",
                                date_str, time_str, the_level, msg
                            ));
                        })
                        .build(),
                )?;
            }

            // 3. State

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// === State Management
struct AppData {
    _tx: Sender<String>,
    rx: Receiver<String>,
    once_push_log: bool,
}

impl AppData {
    fn new(tx: Sender<String>, rx: Receiver<String>) -> Self {
        AppData {
            _tx: tx,
            rx,
            once_push_log: false,
        }
    }
}

// === Call Frontend from Rust

#[tauri::command]
async fn push_log(app: AppHandle) {
    // block read from app.rx and emit frontend function
    let state = app.state::<RwLock<AppData>>();
    let state = state.read().await;
    if state.once_push_log {
        warn!("already start pushing logs");
        return;
    }
    drop(state);

    let state = app.state::<RwLock<AppData>>();
    let mut state = state.write().await;
    state.once_push_log = true;
    println!(">>> let's push log");
    loop {
        println!(">>> before recv");
        let text = state.rx.recv().unwrap();
        app.emit("rust_log_stream", text).unwrap();
        println!(">>> after recv");
    }
}

// === Call Rust from the Frontend

#[tauri::command]
fn request_merge_approval(url: String) {
    debug!("{}", url);
    info!("{}", url);
    warn!("{}", url);
    error!("{}", url);
}
