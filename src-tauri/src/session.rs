use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub windows: Vec<WindowInfo>,
    pub brave_tabs: Vec<BraveTab>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub window_count: usize,
    pub tab_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub exe_path: String,
    pub command_line: Option<String>,
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub show_state: ShowState,
    pub virtual_desktop_index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShowState {
    Normal,
    Minimized,
    Maximized,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BraveTab {
    pub url: String,
    pub title: Option<String>,
}

impl Session {
    pub fn new(name: String, windows: Vec<WindowInfo>, brave_tabs: Vec<BraveTab>) -> Self {
        Self {
            name,
            created_at: Utc::now(),
            windows,
            brave_tabs,
        }
    }
}

fn sessions_dir() -> Result<PathBuf> {
    let base = dirs::data_dir().context("Could not find AppData directory")?;
    let dir = base.join("windowsdesktoptool").join("sessions");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn session_path(name: &str) -> Result<PathBuf> {
    let sanitized = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>();
    Ok(sessions_dir()?.join(format!("{}.json", sanitized)))
}

pub fn save(session: &Session) -> Result<()> {
    let path = session_path(&session.name)?;
    let json = serde_json::to_string_pretty(session)?;
    fs::write(&path, json)?;
    Ok(())
}

pub fn load(name: &str) -> Result<Session> {
    let path = session_path(name)?;
    let json = fs::read_to_string(&path).context(format!("Session '{}' not found", name))?;
    let session: Session = serde_json::from_str(&json)?;
    Ok(session)
}

pub fn delete(name: &str) -> Result<()> {
    let path = session_path(name)?;
    if path.exists() {
        fs::remove_file(&path)?;
    }
    Ok(())
}

pub fn list() -> Result<Vec<SessionSummary>> {
    let dir = sessions_dir()?;
    let mut summaries = Vec::new();

    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            if let Ok(json) = fs::read_to_string(&path) {
                if let Ok(session) = serde_json::from_str::<Session>(&json) {
                    summaries.push(SessionSummary {
                        name: session.name,
                        created_at: session.created_at,
                        window_count: session.windows.len(),
                        tab_count: session.brave_tabs.len(),
                    });
                }
            }
        }
    }

    summaries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(summaries)
}
