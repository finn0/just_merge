use std::{env, path::PathBuf, sync::OnceLock};

use tauri::{App, AppHandle};
use tauri_plugin_store::StoreExt;

static STORE_FILE: OnceLock<String> = OnceLock::new();

const GITLAB_DOMAIN_KEY: &str = "gitlab-domain";
// const GITLAB_ACCESS_TOKEN_KEY: &str = "gitlab-access-token";
// const REDIS_ENDPOINT_KEY: &str = "redis-endpoint";

#[tauri::command]
pub fn filename() -> &'static str {
    STORE_FILE.get().expect("init store before use")
}

pub fn init(app: &App) -> Result<(), StoreError> {
    let config_path = get_config_filepath()?;
    let _ = app.store(&config_path)?;

    STORE_FILE.set(config_path).map_err(StoreError::Init)
}

pub fn get_gitlab_domain(app: &AppHandle) -> String {
    get_string(app, GITLAB_DOMAIN_KEY)
}

fn get_string(app: &AppHandle, key: &str) -> String {
    let store = app.store(filename()).expect("failed to get store");

    store
        .get(key)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default()
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Failed to get env $HOME: {0}")]
    Env(#[from] env::VarError),

    #[error("Plugin error: {0}")]
    Plugin(#[from] tauri_plugin_store::Error),

    #[error("Failed to init store: {0}")]
    Init(String),
}

// Use dirs crate for cross platform support.
fn get_config_filepath() -> Result<String, env::VarError> {
    let home_dir = env::var("HOME")?;

    let mut path = PathBuf::from(home_dir);
    path.push(".just_merge.json");

    Ok(path.display().to_string())
}
