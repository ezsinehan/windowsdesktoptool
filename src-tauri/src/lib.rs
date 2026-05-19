mod capture;
mod restore;
mod session;
mod brave;
mod vdesktop;
mod winapi_helpers;

use log::{error, info};
use session::{RestoreReport, Session, SessionSummary};

#[tauri::command]
fn save_session(name: String) -> Result<Session, String> {
    info!("=== SAVE SESSION: '{}' ===", name);
    let windows = capture::capture_windows().map_err(|e| {
        error!("Window capture failed: {}", e);
        e.to_string()
    })?;
    info!("Captured {} windows", windows.len());

    let brave_tabs = match brave::capture_tabs() {
        Ok(tabs) => {
            info!("Captured {} Brave tabs", tabs.len());
            tabs
        }
        Err(e) => {
            info!("Brave tabs not captured ({}). Skipping.", e);
            Vec::new()
        }
    };

    let session = Session::new(name.clone(), windows, brave_tabs);
    session::save(&session).map_err(|e| {
        error!("Save failed: {}", e);
        e.to_string()
    })?;
    info!("Session '{}' saved successfully", name);
    Ok(session)
}

#[tauri::command]
fn list_sessions() -> Result<Vec<SessionSummary>, String> {
    session::list().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_session(name: String) -> Result<Session, String> {
    session::load(&name).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_session(name: String) -> Result<(), String> {
    info!("Deleting session: '{}'", name);
    session::delete(&name).map_err(|e| e.to_string())
}

#[tauri::command]
async fn restore_session(name: String) -> Result<RestoreReport, String> {
    info!("=== RESTORE SESSION: '{}' ===", name);
    let session = session::load(&name).map_err(|e| {
        error!("Failed to load session: {}", e);
        e.to_string()
    })?;
    info!(
        "Loaded session with {} windows and {} Brave tabs",
        session.windows.len(),
        session.brave_tabs.len()
    );
    let report = restore::restore(&session).map_err(|e| {
        error!("Restore failed: {}", e);
        e.to_string()
    })?;
    info!(
        "=== RESTORE COMPLETE in {}ms — {} window outcomes ===",
        report.duration_ms,
        report.windows.len()
    );
    Ok(report)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logger. Default to INFO level; can be overridden with RUST_LOG env var.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    info!("Desktop Session Manager starting up");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            save_session,
            list_sessions,
            get_session,
            delete_session,
            restore_session,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
