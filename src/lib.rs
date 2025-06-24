use std::{num::ParseIntError, time::Duration};

use crossbeam::channel::{bounded, Receiver, Sender};
use inline_colorization::{color_bright_black, color_green, color_red, color_reset, color_white, color_yellow};
use log::{debug, error, info, Level};
use pubsub::{cli, PubSub};
use serde::Serialize;
use tauri::{async_runtime::RwLock, AppHandle, Emitter, Manager, WindowEvent};
use thiserror::Error;

use pubsub::{
    gitlab::{self, User},
    Approval,
};

mod pubsub;
mod store;
mod tray;

// todo: update redis host from gitlab repo file?

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            // - call frontend from rust
            push_log,
            count_online_users,
            // - call rust from frontend
            init_modules,
            request_merge_approval,
            store::filename,
        ])
        .on_window_event(|window, event| {
            if let ("main", WindowEvent::CloseRequested { api, .. }) = (window.label(), event) {
                window.hide().unwrap();
                api.prevent_close();
            }
        })
        .setup(|app| {
            // 1. Tray menu
            #[cfg(target_os = "macos")]
            {
                tray::init_tray(app.handle()).unwrap();
                app.set_activation_policy(tauri::ActivationPolicy::Accessory); // hide app from docker
            }

            // 2. State management - store log channels.
            let (tx_log, rx_log) = bounded(10);

            // 3. Logs and log stream
            if cfg!(debug_assertions) {
                let tx_log = tx_log.clone();
                app.handle().plugin(
                    tauri_plugin_log::Builder::new()
                        .level(log::LevelFilter::Debug)
                        .filter(|meta| {
                            if meta.target().starts_with("reqwest") || meta.target().starts_with("gitlab") {
                                return meta.level().le(&log::Level::Info);
                            }

                            true
                        })
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

            // 4. init store
            store::init(app).unwrap();
            info!("[store] config path is {}", store::filename());

            // 5. State management - store log channels and current user.
            let app_data = AppData::new(tx_log, rx_log);
            app.manage(RwLock::const_new(app_data));

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
    me: Option<&'static User>,
}

impl AppData {
    fn new(tx: Sender<String>, rx: Receiver<String>) -> Self {
        AppData {
            _tx: tx,
            rx,
            once_push_log: false,
            me: None,
        }
    }

    async fn get_me_or_set(app: &AppHandle) -> &'static User {
        let state = app.state::<RwLock<AppData>>();
        let state_r = state.read().await;
        match state_r.me {
            Some(me) => me,
            None => {
                drop(state_r);

                let me = Box::leak(Box::new(
                    gitlab::cli().current_user().await.expect("get current gitlab user"),
                ));
                let mut state = state.write().await;
                state.me = Some(me);

                me
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{source}")]
struct AppError {
    #[from]
    source: anyhow::Error,
}

impl AppError {
    fn new<E>(err: E) -> Self
    where
        E: Into<anyhow::Error>,
    {
        Self { source: err.into() }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.source.to_string())
    }
}

trait LogResult<T> {
    fn log_error(self, context: Option<&str>) -> Result<T, AppError>;
}

impl<T, E> LogResult<T> for Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn log_error(self, context: Option<&str>) -> Result<T, AppError> {
        self.map_err(|err| {
            match context {
                Some(v) => error!("{}: {}", v, err),
                None => error!("{}", err),
            }
            AppError::new(err)
        })
    }
}

// === Call Frontend from Rust

// push logs to frontend.
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
    let rx = state.rx.clone();
    drop(state);

    loop {
        let text = rx.recv().unwrap();
        app.emit("rust_log_stream", text).unwrap();
    }
}

#[tauri::command]
async fn count_online_users(app: AppHandle) {
    let mut ticker = tokio::time::interval(Duration::from_secs(60));
    loop {
        ticker.tick().await;
        match cli().online_users().await {
            Ok(n) => app.emit("update_online_user_count", n).unwrap(),
            Err(err) => error!("failed to get online users: {}", err),
        }
    }
}

// === Call Rust from the Frontend

// The workflow is,
// as a requester,
// 1. parse url, get pid, mid -> approval request
// 2. publish approval request
// 3. await approval response and send notification if applied.

// as an approver,
// 1. decode approval request
// 2. gitlab: get merge request details, approve merge request if possible
// 3. publish approval resonse

// Init modules(pubsub, gitlab) with user inputs.
#[tauri::command(rename_all = "snake_case")]
fn init_modules(app: AppHandle, gitlab_domain: String, access_token: String, redis_endpoint: String) {
    // 1. Gitlab agent init.
    gitlab::init(gitlab_domain, access_token).expect("failed to init gitlab agent");

    // 2. Pubsub redis init.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let me = tauri::async_runtime::handle().block_on(async { AppData::get_me_or_set(app.app_handle()).await });
    let app_pubsub = app.clone();

    pubsub::init(&redis_endpoint, tx, me.clone()).expect("failed to init pubsub agent");

    // 2.1 pubsub - block read approval response from others
    tauri::async_runtime::spawn(async move {
        while let Some(approval) = rx.recv().await {
            approval.show_response_details(app.app_handle());
        }
    });

    // 2.2 pubsub - subscribe channels and patterns; count online users
    tauri::async_runtime::spawn(async move {
        pubsub::cli().sub_approval_request().await.unwrap();
        pubsub::cli().sub_approval_response(me).await.unwrap();
        count_online_users(app_pubsub).await;
    });
}

#[tauri::command]
async fn request_merge_approval(app: AppHandle, url: String) -> Result<(), AppError> {
    debug!("parse url: [{}]", url);

    let gitlab_domain = store::get_gitlab_domain(app.app_handle());

    let (pid, mid) =
        extract_pid_and_mid(&url, &gitlab_domain).log_error(Some(format!("failed to parse {}", &url).as_str()))?;

    debug!("publish approval request, pid: {}, mid: {}", pid, mid);

    // 1. request gitlab merge request details.
    let details = gitlab::cli()
        .get_mr_details(&pid, mid)
        .await
        .log_error(None)?
        .info(&pid);

    // 2. publish approval request.
    let me = AppData::get_me_or_set(app.app_handle()).await;
    let approval_request = Approval::new_request(me.to_owned(), pid, mid, details);

    pubsub::cli()
        .pub_approval_request(&approval_request)
        .await
        .log_error(Some("failed to publish approval request"))
}

// === Utilities

// You should try regex which needs additional crate.
fn extract_pid_and_mid(url: &str, expected_domain: &str) -> Result<(String, u64), ExtractionErr> {
    let schema_trimmed = url.strip_prefix("https://").unwrap_or(url);

    if !schema_trimmed.starts_with(expected_domain) {
        return Err(ExtractionErr::DomainNotMatch {
            domain: expected_domain.to_string(),
        });
    }

    let path = &schema_trimmed[format!("{expected_domain}/").len()..];
    let marker = "/-/merge_requests/";
    let marker_index = path.find(marker).ok_or(ExtractionErr::PatternNotRecognized {
        pattern: marker.to_string(),
    })?;

    let (pid, rest) = path.split_at(marker_index);
    let mid = &rest[marker.len()..];
    let mid = mid
        .split('?')
        .next()
        .ok_or(ExtractionErr::MissingPidOrMid)?
        .parse::<u64>()
        .map_err(|error| ExtractionErr::InvalidMid {
            mid: mid.to_string(),
            error,
        })?;

    Ok((pid.to_string(), mid))
}

#[derive(Debug, Error)]
enum ExtractionErr {
    #[error("invalid url, expect domain is {domain}")]
    DomainNotMatch { domain: String },

    #[error("pattern not recognized: {pattern}")]
    PatternNotRecognized { pattern: String },

    #[error("project or merge request id not found")]
    MissingPidOrMid,

    #[error("could not parse {mid} into a numeric mid: {error}")]
    InvalidMid { mid: String, error: ParseIntError },
}
