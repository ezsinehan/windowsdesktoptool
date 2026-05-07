use anyhow::{Context, Result};
use log::{info, warn};
use std::collections::HashMap;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, SetWindowPlacement, WINDOWPLACEMENT, SW_SHOWNORMAL, SW_SHOWMINIMIZED,
    SW_SHOWMAXIMIZED,
};
use windows::Win32::Foundation::RECT;

use crate::session::{Session, ShowState, WindowInfo};
use crate::vdesktop;
use crate::winapi_helpers;

/// Restore a saved session: relaunch apps, reposition windows, restore Brave tabs.
pub fn restore(session: &Session) -> Result<()> {
    let running = get_running_processes()?;
    info!("Found {} running processes on the system", running.len());

    for (idx, window_info) in session.windows.iter().enumerate() {
        let exe_lower = window_info.exe_path.to_lowercase();
        let exe_name = std::path::Path::new(&exe_lower)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        info!(
            "[{}/{}] Restoring: {} ({})",
            idx + 1,
            session.windows.len(),
            window_info.title,
            exe_name
        );

        let already_running = running.contains_key(&exe_name);

        if already_running {
            info!("  Already running — looking up existing window...");
            if let Some(hwnd) = find_window_by_exe(&window_info.exe_path) {
                info!("  Found existing window. Repositioning to ({}, {}) {}x{}",
                    window_info.x, window_info.y, window_info.width, window_info.height);
                if let Err(e) = apply_placement(hwnd, window_info) {
                    warn!("  Failed to reposition: {}", e);
                }
                continue;
            } else {
                info!("  No window found despite running process. Launching anyway.");
            }
        }

        info!("  Launching: {}", window_info.exe_path);
        match std::process::Command::new(&window_info.exe_path).spawn() {
            Ok(child) => {
                let pid = child.id();
                info!("  Spawned PID {}. Waiting for window...", pid);
                if let Some(hwnd) = wait_for_window(pid, 15_000) {
                    info!("  Window detected. Repositioning to ({}, {}) {}x{}",
                        window_info.x, window_info.y, window_info.width, window_info.height);
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    if let Err(e) = apply_placement(hwnd, window_info) {
                        warn!("  Failed to reposition: {}", e);
                    }

                    if let Some(vd_index) = window_info.virtual_desktop_index {
                        info!("  Moving to virtual desktop {}", vd_index + 1);
                        match vdesktop::get_desktop_guid_by_index(vd_index) {
                            Ok(guid) => match vdesktop::VirtualDesktopManager::new() {
                                Ok(vdm) => {
                                    if let Err(e) = vdm.move_to_desktop(hwnd, &guid) {
                                        warn!("  Failed to move to desktop {}: {}", vd_index + 1, e);
                                    }
                                }
                                Err(e) => warn!("  Could not init VirtualDesktopManager: {}", e),
                            },
                            Err(e) => warn!("  Desktop {} not available: {}", vd_index + 1, e),
                        }
                    }

                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let _ = apply_placement(hwnd, window_info);
                } else {
                    warn!("  Timed out waiting for window from PID {}", pid);
                }
            }
            Err(e) => {
                warn!("  Failed to launch {}: {}", window_info.exe_path, e);
            }
        }
    }

    if !session.brave_tabs.is_empty() {
        info!("Restoring {} Brave tabs...", session.brave_tabs.len());
        if let Err(e) = crate::brave::restore_tabs(&session.brave_tabs) {
            warn!("Failed to restore Brave tabs: {}", e);
        } else {
            info!("Brave tabs restored");
        }
    }

    Ok(())
}

fn apply_placement(hwnd: HWND, info: &WindowInfo) -> Result<()> {
    let show_cmd = match info.show_state {
        ShowState::Minimized => SW_SHOWMINIMIZED.0 as u32,
        ShowState::Maximized => SW_SHOWMAXIMIZED.0 as u32,
        ShowState::Normal => SW_SHOWNORMAL.0 as u32,
    };

    let placement = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        showCmd: show_cmd,
        rcNormalPosition: RECT {
            left: info.x,
            top: info.y,
            right: info.x + info.width,
            bottom: info.y + info.height,
        },
        ..Default::default()
    };

    unsafe {
        SetWindowPlacement(hwnd, &placement).context("Failed to set window placement")?;
    }

    Ok(())
}

/// Get a map of running process exe names (lowercase) to their PIDs.
fn get_running_processes() -> Result<HashMap<String, u32>> {
    let mut map = HashMap::new();

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
            .context("Failed to create process snapshot")?;

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let name = String::from_utf16_lossy(
                    &entry.szExeFile[..entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len())],
                );
                map.insert(name.to_lowercase(), entry.th32ProcessID);

                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = windows::Win32::Foundation::CloseHandle(snapshot);
    }

    Ok(map)
}

/// Find an existing window by its exe path.
fn find_window_by_exe(exe_path: &str) -> Option<HWND> {
    struct FindCtx {
        target_exe: String,
        found: Option<HWND>,
    }

    let mut ctx = FindCtx {
        target_exe: exe_path.to_lowercase(),
        found: None,
    };

    unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam.0 as *mut FindCtx);

        if !winapi_helpers::is_window_visible(hwnd) {
            return BOOL(1);
        }

        let pid = winapi_helpers::get_window_pid(hwnd);
        if pid == 0 {
            return BOOL(1);
        }

        if let Ok(path) = winapi_helpers::get_exe_path_from_pid(pid) {
            if path.to_lowercase() == ctx.target_exe {
                ctx.found = Some(hwnd);
                return BOOL(0); // Stop enumeration
            }
        }

        BOOL(1)
    }

    unsafe {
        let _ = EnumWindows(
            Some(callback),
            LPARAM(&mut ctx as *mut FindCtx as isize),
        );
    }

    ctx.found
}

/// Poll for a window owned by the given PID, waiting up to timeout_ms.
fn wait_for_window(pid: u32, timeout_ms: u64) -> Option<HWND> {
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_millis(timeout_ms);

    while start.elapsed() < timeout {
        struct PidCtx {
            target_pid: u32,
            found: Option<HWND>,
        }

        let mut ctx = PidCtx {
            target_pid: pid,
            found: None,
        };

        unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let ctx = &mut *(lparam.0 as *mut PidCtx);

            if !winapi_helpers::is_window_visible(hwnd) {
                return BOOL(1);
            }

            let window_pid = winapi_helpers::get_window_pid(hwnd);
            if window_pid == ctx.target_pid {
                let title = winapi_helpers::get_window_title(hwnd);
                if !title.is_empty() {
                    ctx.found = Some(hwnd);
                    return BOOL(0);
                }
            }

            BOOL(1)
        }

        unsafe {
            let _ = EnumWindows(
                Some(callback),
                LPARAM(&mut ctx as *mut PidCtx as isize),
            );
        }

        if ctx.found.is_some() {
            return ctx.found;
        }

        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    None
}
