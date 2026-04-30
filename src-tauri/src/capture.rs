use anyhow::Result;
use std::sync::Mutex;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::EnumWindows;

use crate::session::WindowInfo;
use crate::vdesktop;
use crate::winapi_helpers;

/// List of exe filenames to exclude from capture (system/shell processes).
const EXCLUDED_EXES: &[&str] = &[
    "explorer.exe",
    "shellexperiencehost.exe",
    "searchhost.exe",
    "startmenuexperiencehost.exe",
    "textinputhost.exe",
    "applicationframehost.exe",
    "systemsettings.exe",
    "lockapp.exe",
];

fn is_excluded_exe(exe_path: &str) -> bool {
    let lower = exe_path.to_lowercase();
    EXCLUDED_EXES.iter().any(|exc| lower.ends_with(exc))
}

pub fn capture_windows() -> Result<Vec<WindowInfo>> {
    let results: Mutex<Vec<WindowInfo>> = Mutex::new(Vec::new());

    unsafe {
        EnumWindows(
            Some(enum_window_proc),
            LPARAM(&results as *const Mutex<Vec<WindowInfo>> as isize),
        )?;
    }

    let windows = results.into_inner().unwrap();
    Ok(windows)
}

unsafe extern "system" fn enum_window_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let results = &*(lparam.0 as *const Mutex<Vec<WindowInfo>>);

    // Skip invisible windows
    if !winapi_helpers::is_window_visible(hwnd) {
        return BOOL(1);
    }

    // Skip cloaked windows (hidden UWP, etc.)
    if winapi_helpers::is_window_cloaked(hwnd) {
        return BOOL(1);
    }

    // Skip windows with no title
    let title = winapi_helpers::get_window_title(hwnd);
    if title.is_empty() {
        return BOOL(1);
    }

    // Get process info
    let pid = winapi_helpers::get_window_pid(hwnd);
    if pid == 0 {
        return BOOL(1);
    }

    let exe_path = match winapi_helpers::get_exe_path_from_pid(pid) {
        Ok(path) => path,
        Err(_) => return BOOL(1),
    };

    // Skip excluded system processes
    if is_excluded_exe(&exe_path) {
        return BOOL(1);
    }

    // Skip our own window
    if title.contains("Desktop Session Manager") {
        return BOOL(1);
    }

    // Get window placement
    let placement = match winapi_helpers::get_window_placement(hwnd) {
        Ok(p) => p,
        Err(_) => return BOOL(1),
    };

    // Get virtual desktop index
    let vd_index = vdesktop::get_window_desktop_index(hwnd).ok();

    let info = WindowInfo {
        exe_path,
        command_line: None,
        title,
        x: placement.x,
        y: placement.y,
        width: placement.width,
        height: placement.height,
        show_state: placement.show_state,
        virtual_desktop_index: vd_index,
    };

    if let Ok(mut guard) = results.lock() {
        guard.push(info);
    }

    BOOL(1) // Continue enumeration
}
