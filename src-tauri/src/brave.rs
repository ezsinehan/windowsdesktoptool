use anyhow::{Context, Result};
use log::{info, warn};
use serde::Deserialize;

use crate::session::BraveTab;

const CDP_URL: &str = "http://localhost:9222/json";

#[derive(Deserialize)]
struct CdpTarget {
    url: Option<String>,
    title: Option<String>,
    #[serde(rename = "type")]
    target_type: Option<String>,
}

/// Capture open Brave tabs via Chrome DevTools Protocol.
/// Returns an empty vec if Brave is not running with --remote-debugging-port.
pub fn capture_tabs() -> Result<Vec<BraveTab>> {
    info!("Querying Brave CDP at {}", CDP_URL);
    let body: String = ureq::get(CDP_URL)
        .call()
        .context("Brave is not running with remote debugging enabled (start brave with --remote-debugging-port=9222)")?
        .body_mut()
        .read_to_string()
        .context("Failed to read CDP response")?;

    let targets: Vec<CdpTarget> =
        serde_json::from_str(&body).context("Failed to parse CDP response")?;

    let tabs: Vec<BraveTab> = targets
        .into_iter()
        .filter(|t| t.target_type.as_deref() == Some("page"))
        .filter_map(|t| {
            let url = t.url?;
            // Skip internal browser pages
            if url.starts_with("chrome://") || url.starts_with("brave://") || url.starts_with("devtools://") {
                return None;
            }
            Some(BraveTab {
                url,
                title: t.title,
            })
        })
        .collect();

    Ok(tabs)
}

/// Restore Brave tabs by launching Brave with URLs as arguments.
pub fn restore_tabs(tabs: &[BraveTab]) -> Result<()> {
    if tabs.is_empty() {
        return Ok(());
    }

    let brave_path = find_brave_exe()?;
    info!("Launching Brave at {} with {} tabs", brave_path, tabs.len());
    for t in tabs {
        info!("  -> {}", t.url);
    }
    let urls: Vec<&str> = tabs.iter().map(|t| t.url.as_str()).collect();

    std::process::Command::new(&brave_path)
        .arg("--new-window")
        .args(&urls)
        .spawn()
        .context("Failed to launch Brave browser")?;

    Ok(())
}

fn find_brave_exe() -> Result<String> {
    let candidates = [
        r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
        r"C:\Program Files (x86)\BraveSoftware\Brave-Browser\Application\brave.exe",
    ];

    // Also check user-local install
    let local_path = dirs::data_local_dir().map(|d| {
        d.join("BraveSoftware")
            .join("Brave-Browser")
            .join("Application")
            .join("brave.exe")
    });

    if let Some(ref p) = local_path {
        if p.exists() {
            return Ok(p.to_string_lossy().to_string());
        }
    }

    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return Ok(path.to_string());
        }
    }

    warn!("Brave browser not found in any standard location");
    anyhow::bail!("Brave browser not found. Please ensure Brave is installed.")
}
