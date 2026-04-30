mod capture;
mod restore;
mod session;
mod brave;
mod vdesktop;
mod winapi_helpers;

use session::{Session, SessionSummary};

#[tauri::command]
fn save_session(name: String) -> Result<Session, String> {
    let windows = capture::capture_windows().map_err(|e| e.to_string())?;
    let brave_tabs = brave::capture_tabs().unwrap_or_default();
    let session = Session::new(name.clone(), windows, brave_tabs);
    session::save(&session).map_err(|e| e.to_string())?;
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
    session::delete(&name).map_err(|e| e.to_string())
}

#[tauri::command]
async fn restore_session(name: String) -> Result<(), String> {
    let session = session::load(&name).map_err(|e| e.to_string())?;
    restore::restore(&session).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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
