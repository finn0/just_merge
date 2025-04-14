use std::time::{Duration, Instant};

use crossbeam::channel::{bounded, Receiver, Sender};
use inline_colorization::{color_bright_black, color_green, color_red, color_reset, color_white, color_yellow};
use log::{debug, error, info, warn, Level};
use pubsub::{cli, PubSub};
use tauri::{async_runtime::RwLock, AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_notification::NotificationExt;

mod pubsub;
mod tray;

// todo: ask to add to auto startup for mac os
// todo: update user info from gitlab daily(eg: common user -> security plus memory)

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        // - Static Plugins
        // - Invoke Handler
        .invoke_handler(tauri::generate_handler![
            // - call frontend from rust
            push_log,
            // -- show online user count at footer
            show_online_user_count,
            // on_mr_done,
            // - call rust from frontend
            request_merge_approval,
        ])
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

            // 2. State management - store log channels.
            let (tx_log, rx_log) = bounded(10);
            let app_data = AppData::new(tx_log.clone(), rx_log);
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

                            tx_log
                                .send(format!(
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

            // 4. notification

            // 4.1 pubsub - init agent.
            let me = pubsub::User {
                id: "user001".to_string(),
                name: "foo".to_string(),
            };
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

            // role base
            // 1. sub mr, sub mr.res.*, monitor online users
            // 2. process approval request
            // 3. process approval response

            // role foo
            // 1. publish aproval request

            // role bar
            // 1. publish approval response

            // todo: sub rx
            // 1. parse approval_response, if it gets the approval response, show the notification
            // 2. parse approval_request - app_handler not needed.
            // 2.1 get distribute lock
            // 2.2 try to approve(gitlab)
            // 2.3 release distribute lock
            // 2.4 publish approval_response

            // todo: handle approva_request from other users.
            // todo: save rx into state?
            // todo: why would log prints twice to stdout

            pubsub::init("127.0.0.1", 6379, tx, &me).expect("failed to init pubsub agent");

            // 4.2 pubsub - block read approval response from others
            tauri::async_runtime::spawn(async move {
                while let Some(approval) = rx.recv().await {
                    info!(">> approval for app: {:?}", approval);
                }
            });

            // 4.2 pubsub - subscribe channels and patterns; count online users
            let handler = app.handle().to_owned();
            tauri::async_runtime::spawn(async move {
                pubsub::cli().sub_approval_request().await.unwrap(); // sub approval requests from other users
                pubsub::cli().sub_approval_response(&me.clone()).await.unwrap(); // sub approval responses from other users
                count_online_users(handler).await; // block reading online user count
            });

            // 4.3 pubsub - block reading approval response

            // 5. State

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

// push tauri logs to frontend.
#[tauri::command]
async fn push_log(app: AppHandle) {
    // block read from app.rx and emit frontend function
    let state = app.state::<RwLock<AppData>>();
    let state = state.read().await;
    if state.once_push_log {
        return;
    }
    drop(state);

    let state = app.state::<RwLock<AppData>>();
    let mut state = state.write().await;
    state.once_push_log = true;
    loop {
        let text = state.rx.recv().unwrap();
        app.emit("rust_log_stream", text).unwrap();
    }
}

async fn count_online_users(app: AppHandle) {
    // todo: configurable ticker
    let mut ticker = tokio::time::interval(Duration::from_secs(15));
    loop {
        ticker.tick().await;
        match cli().oneline_users().await {
            Ok(n) => {
                // todo: call frontend from rust, show the number at bottom
                info!("online users: {}", n);
                show_online_user_count(app.clone(), n);

                app.notification()
                    .builder()
                    .title("online users")
                    .body(format!("count is {}", n))
                    .show()
                    .unwrap();
            }
            Err(err) => error!("failed to get online users: {}", err),
        }
    }
}

#[tauri::command]
fn show_online_user_count(_app: AppHandle, _count: u32) {
    debug!("[show_online_user_count] is called to show online user number from Frontend");
    // app.emit("show_online_user_count", count);
}

#[tauri::command]
async fn on_mr_done(app: AppHandle) {
    let then = Instant::now();
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    for _ in 0..10 {
        ticker.tick().await;
        let pass = Instant::now().duration_since(then);
        info!("time passed {:?}", pass);
    }
    // let result = pubsub::on_sub_result().await;
    // app.emit("on_get_approval_request_result", result).unwrap();
}

// === Call Rust from the Frontend

#[tauri::command]
async fn request_merge_approval(app: AppHandle, url: String) {
    debug!("{}", url);
    info!("{}", url);
    warn!("{}", url);
    error!("{}", url);
}
