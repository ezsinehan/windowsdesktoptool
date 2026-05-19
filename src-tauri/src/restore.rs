use anyhow::{Context, Result};
use chrono::Utc;
use log::{info, warn};
use std::collections::HashMap;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{BOOL, CloseHandle, HWND, LPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::GetProcessId;
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, IsWindow, SetWindowPlacement, SW_SHOW, WINDOWPLACEMENT, SW_SHOWNORMAL,
    SW_SHOWMINIMIZED, SW_SHOWMAXIMIZED,
};
use windows::Win32::Foundation::RECT;

use crate::session::{
    BraveOutcome, OutcomeStatus, RestoreReport, Session, ShowState, WindowInfo, WindowOutcome,
};
use crate::vdesktop;
use crate::winapi_helpers;

/// Restore a saved session: relaunch apps, reposition windows, restore Brave tabs.
/// Returns a structured report of every per-window step so the UI can show what
/// worked and what didn't.
pub fn restore(session: &Session) -> Result<RestoreReport> {
    let started_at = Utc::now();
    let start_instant = std::time::Instant::now();

    let running = get_running_processes()?;
    info!("Found {} running processes on the system", running.len());

    let mut outcomes: Vec<WindowOutcome> = Vec::with_capacity(session.windows.len());
    // Track HWNDs we've already operated on in this restore call so the same
    // window doesn't get matched twice when the session has multiple entries
    // for one exe (e.g. two Brave windows).
    let mut touched_hwnds: std::collections::HashSet<isize> =
        std::collections::HashSet::new();

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

        let outcome = restore_window(window_info, &exe_name, &running, &mut touched_hwnds);
        outcomes.push(outcome);
    }

    let brave_outcome = if session.brave_tabs.is_empty() {
        None
    } else {
        info!("Restoring {} Brave tabs...", session.brave_tabs.len());
        Some(match crate::brave::restore_tabs(&session.brave_tabs) {
            Ok(()) => {
                info!("Brave tabs restored");
                BraveOutcome {
                    status: OutcomeStatus::Success,
                    message: format!("Launched Brave with {} tab(s)", session.brave_tabs.len()),
                    tab_count: session.brave_tabs.len(),
                }
            }
            Err(e) => {
                warn!("Failed to restore Brave tabs: {}", e);
                BraveOutcome {
                    status: OutcomeStatus::Failed,
                    message: format!("Failed to launch Brave: {}", e),
                    tab_count: session.brave_tabs.len(),
                }
            }
        })
    };

    let duration_ms = start_instant.elapsed().as_millis() as u64;

    Ok(RestoreReport {
        session_name: session.name.clone(),
        started_at,
        duration_ms,
        windows: outcomes,
        brave: brave_outcome,
    })
}

enum LaunchResult {
    Plain(u32),
    Elevated(u32),
}

enum LaunchError {
    UacDeclined,
    Other(String),
}

/// `ERROR_ELEVATION_REQUIRED` — std::process::Command::spawn returns this in
/// `raw_os_error()` when the target exe demands UAC elevation.
const ERROR_ELEVATION_REQUIRED: i32 = 740;
/// `ERROR_CANCELLED` — ShellExecuteExW with "runas" returns this when the
/// user clicks "No" on the UAC prompt.
const ERROR_CANCELLED: i32 = 1223;

/// Spawn an exe. If it demands elevation, retry via ShellExecuteEx with
/// the "runas" verb so the user can grant UAC. Returns the PID on success.
fn launch_exe(exe_path: &str) -> Result<LaunchResult, LaunchError> {
    match std::process::Command::new(exe_path).spawn() {
        Ok(child) => Ok(LaunchResult::Plain(child.id())),
        Err(e) if e.raw_os_error() == Some(ERROR_ELEVATION_REQUIRED) => {
            launch_elevated(exe_path).map(LaunchResult::Elevated)
        }
        Err(e) => Err(LaunchError::Other(e.to_string())),
    }
}

/// Spawn an exe via ShellExecuteEx with the "runas" verb. Triggers a UAC
/// prompt. Returns the PID of the spawned (elevated) process.
fn launch_elevated(exe_path: &str) -> Result<u32, LaunchError> {
    let wide_verb: Vec<u16> = "runas\0".encode_utf16().collect();
    let wide_file: Vec<u16> = exe_path
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(wide_verb.as_ptr()),
        lpFile: PCWSTR(wide_file.as_ptr()),
        nShow: SW_SHOW.0,
        ..Default::default()
    };

    let result = unsafe { ShellExecuteExW(&mut info) };
    if let Err(e) = result {
        // `windows::core::Error` wraps the underlying GetLastError code.
        let code = e.code().0 & 0xFFFF;
        if code as i32 == ERROR_CANCELLED {
            return Err(LaunchError::UacDeclined);
        }
        return Err(LaunchError::Other(format!(
            "ShellExecuteEx (runas) failed: {} (code 0x{:08X})",
            e.message(),
            e.code().0 as u32
        )));
    }

    if info.hProcess.is_invalid() {
        return Err(LaunchError::Other(
            "ShellExecuteEx returned no process handle".to_string(),
        ));
    }

    let pid = unsafe { GetProcessId(info.hProcess) };
    unsafe {
        let _ = CloseHandle(info.hProcess);
    }
    if pid == 0 {
        return Err(LaunchError::Other(
            "GetProcessId returned 0 after elevated spawn".to_string(),
        ));
    }

    Ok(pid)
}

fn restore_window(
    window_info: &WindowInfo,
    exe_name: &str,
    running: &HashMap<String, u32>,
    touched_hwnds: &mut std::collections::HashSet<isize>,
) -> WindowOutcome {
    let mut steps: Vec<String> = Vec::new();
    let already_running = running.contains_key(exe_name);

    // Try the saved HWND first. If still valid and belongs to the same exe,
    // we're guaranteed to operate on the right window even when the session
    // has multiple entries for one exe (e.g. two Brave windows).
    let matched_hwnd: Option<HWND> = if let Some(saved) = window_info.hwnd {
        let saved_isize = saved as isize;
        if touched_hwnds.contains(&saved_isize) {
            None
        } else {
            let hwnd = HWND(saved_isize as *mut std::ffi::c_void);
            if hwnd_matches_exe(hwnd, &window_info.exe_path) {
                steps.push(format!("Matched saved HWND 0x{:X}", saved));
                Some(hwnd)
            } else {
                steps.push(format!(
                    "Saved HWND 0x{:X} is stale (window closed or exe changed)",
                    saved
                ));
                None
            }
        }
    } else {
        None
    };

    let matched_hwnd = matched_hwnd.or_else(|| {
        if !already_running {
            steps.push("Process not running; launching".to_string());
            return None;
        }
        steps.push(
            "Process already running — scanning for an unmatched window".to_string(),
        );
        find_window_by_exe_excluding(&window_info.exe_path, touched_hwnds)
    });

    if let Some(hwnd) = matched_hwnd {
        touched_hwnds.insert(hwnd.0 as isize);
        steps.push(format!(
            "Repositioning to ({}, {}) {}x{}",
            window_info.x, window_info.y, window_info.width, window_info.height
        ));
        match apply_placement(hwnd, window_info) {
            Ok(()) => {
                let (vd_status, vd_message) =
                    move_to_saved_desktop(hwnd, window_info, &mut steps);
                return WindowOutcome {
                    exe_path: window_info.exe_path.clone(),
                    title: window_info.title.clone(),
                    status: vd_status,
                    message: vd_message
                        .unwrap_or_else(|| "Repositioned existing window".to_string()),
                    steps,
                };
            }
            Err(e) => {
                warn!("  Failed to reposition: {}", e);
                steps.push(format!("SetWindowPlacement failed: {}", e));
                return WindowOutcome {
                    exe_path: window_info.exe_path.clone(),
                    title: window_info.title.clone(),
                    status: OutcomeStatus::Failed,
                    message: format!("Could not reposition existing window: {}", e),
                    steps,
                };
            }
        }
    }

    if already_running {
        steps.push(
            "No unmatched window found for this exe; spawning fresh instance".to_string(),
        );
    }

    info!("  Launching: {}", window_info.exe_path);
    let pid = match launch_exe(&window_info.exe_path) {
        Ok(LaunchResult::Plain(pid)) => {
            steps.push(format!("Spawned PID {}; waiting up to 15s for window", pid));
            info!("  Spawned PID {}. Waiting for window...", pid);
            pid
        }
        Ok(LaunchResult::Elevated(pid)) => {
            steps.push(format!(
                "Elevation required; relaunched via UAC. PID {}; waiting up to 15s for window",
                pid
            ));
            info!("  Elevated via UAC, PID {}. Waiting for window...", pid);
            pid
        }
        Err(LaunchError::UacDeclined) => {
            warn!("  User declined UAC prompt for {}", window_info.exe_path);
            steps.push("User declined the UAC prompt".to_string());
            return WindowOutcome {
                exe_path: window_info.exe_path.clone(),
                title: window_info.title.clone(),
                status: OutcomeStatus::Failed,
                message: "User declined the UAC elevation prompt".to_string(),
                steps,
            };
        }
        Err(LaunchError::Other(e)) => {
            warn!("  Failed to launch {}: {}", window_info.exe_path, e);
            steps.push(format!("Failed to spawn process: {}", e));
            return WindowOutcome {
                exe_path: window_info.exe_path.clone(),
                title: window_info.title.clone(),
                status: OutcomeStatus::Failed,
                message: format!("Could not launch executable: {}", e),
                steps,
            };
        }
    };

    let hwnd = match wait_for_window(pid, 15_000) {
        Some(h) => h,
        None => {
            warn!("  Timed out waiting for window from PID {}", pid);
            steps.push("Timed out waiting for window to appear".to_string());
            return WindowOutcome {
                exe_path: window_info.exe_path.clone(),
                title: window_info.title.clone(),
                status: OutcomeStatus::Failed,
                message: "Process launched but no window appeared within 15s".to_string(),
                steps,
            };
        }
    };
    touched_hwnds.insert(hwnd.0 as isize);

    steps.push(format!(
        "Window detected; repositioning to ({}, {}) {}x{}",
        window_info.x, window_info.y, window_info.width, window_info.height
    ));
    std::thread::sleep(std::time::Duration::from_millis(500));

    let reposition_status = match apply_placement(hwnd, window_info) {
        Ok(()) => None,
        Err(e) => {
            warn!("  Failed to reposition: {}", e);
            steps.push(format!("SetWindowPlacement failed: {}", e));
            Some(e.to_string())
        }
    };

    let (vd_status, vd_message) = move_to_saved_desktop(hwnd, window_info, &mut steps);

    // Re-apply placement after virtual desktop move (some apps reset their window).
    std::thread::sleep(std::time::Duration::from_millis(500));
    let _ = apply_placement(hwnd, window_info);

    let status = match (&reposition_status, &vd_status) {
        (Some(_), _) => OutcomeStatus::Partial,
        (None, OutcomeStatus::Success) | (None, OutcomeStatus::Skipped) => OutcomeStatus::Success,
        (None, OutcomeStatus::Partial) => OutcomeStatus::Partial,
        (None, OutcomeStatus::Failed) => OutcomeStatus::Partial,
    };

    let message = match (&reposition_status, vd_message) {
        (Some(e), _) => format!("Launched but reposition failed: {}", e),
        (None, Some(vd_msg)) => vd_msg,
        (None, None) => "Launched and repositioned".to_string(),
    };

    WindowOutcome {
        exe_path: window_info.exe_path.clone(),
        title: window_info.title.clone(),
        status,
        message,
        steps,
    }
}

/// Move window to its saved virtual desktop. Returns (status, optional message).
/// Status is Skipped when the session has no saved desktop, Success when moved,
/// Partial/Failed for COM/registry trouble.
fn move_to_saved_desktop(
    hwnd: HWND,
    window_info: &WindowInfo,
    steps: &mut Vec<String>,
) -> (OutcomeStatus, Option<String>) {
    let Some(vd_index) = window_info.virtual_desktop_index else {
        return (OutcomeStatus::Skipped, None);
    };

    info!("  Moving to virtual desktop {}", vd_index + 1);
    steps.push(format!("Moving to virtual desktop {}", vd_index + 1));

    let guid = match vdesktop::get_desktop_guid_by_index(vd_index) {
        Ok(g) => g,
        Err(e) => {
            warn!("  Desktop {} not available: {}", vd_index + 1, e);
            steps.push(format!("Desktop {} not found in registry: {}", vd_index + 1, e));
            return (
                OutcomeStatus::Partial,
                Some(format!("Repositioned but desktop {} unavailable", vd_index + 1)),
            );
        }
    };

    // Use the undocumented IVirtualDesktopManagerInternal — the public
    // IVirtualDesktopManager only works for windows owned by *our* process,
    // which is never the case for restored apps. Falling back to the public
    // API is pointless (it always returns E_ACCESSDENIED for foreign HWNDs).
    match vdesktop::move_window_to_desktop_internal(hwnd, &guid) {
        Ok(()) => {
            steps.push(format!("Moved to desktop {}", vd_index + 1));
            (
                OutcomeStatus::Success,
                Some(format!("Repositioned on desktop {}", vd_index + 1)),
            )
        }
        Err(e) => {
            warn!("  Failed to move to desktop {}: {}", vd_index + 1, e);
            steps.push(format!("MoveViewToDesktop failed: {}", e));
            (
                OutcomeStatus::Partial,
                Some(format!("Repositioned but desktop move failed: {}", e)),
            )
        }
    }
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

/// Check that a saved HWND still refers to a visible window belonging to the
/// expected exe. Stale HWNDs (window closed) or recycled HWNDs (now belong to
/// a different process) are rejected.
fn hwnd_matches_exe(hwnd: HWND, expected_exe: &str) -> bool {
    unsafe {
        if !IsWindow(hwnd).as_bool() {
            return false;
        }
    }
    if !winapi_helpers::is_window_visible(hwnd) {
        return false;
    }
    let pid = winapi_helpers::get_window_pid(hwnd);
    if pid == 0 {
        return false;
    }
    match winapi_helpers::get_exe_path_from_pid(pid) {
        Ok(path) => path.to_lowercase() == expected_exe.to_lowercase(),
        Err(_) => false,
    }
}

/// Find a visible top-level window owned by the given exe, skipping any HWNDs
/// already consumed by an earlier restore-step in this session.
fn find_window_by_exe_excluding(
    exe_path: &str,
    exclude: &std::collections::HashSet<isize>,
) -> Option<HWND> {
    struct FindCtx<'a> {
        target_exe: String,
        exclude: &'a std::collections::HashSet<isize>,
        found: Option<HWND>,
    }

    let mut ctx = FindCtx {
        target_exe: exe_path.to_lowercase(),
        exclude,
        found: None,
    };

    unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam.0 as *mut FindCtx);

        if ctx.exclude.contains(&(hwnd.0 as isize)) {
            return BOOL(1);
        }
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
                return BOOL(0);
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

#[cfg(test)]
mod integration_tests {
    //! Live integration tests for the restore flow.
    //!
    //! These spawn a real GUI process (mspaint.exe), capture its window via
    //! `capture::capture_windows`, kill it, then call `restore` and assert
    //! both the structured `RestoreReport` and the post-restore window state.
    //!
    //! Marked `#[ignore]` because they touch the live desktop. Run with:
    //!   cargo test --manifest-path src-tauri/Cargo.toml -- --ignored --test-threads=1
    //!
    //! Pre-condition: no instances of the test target may be open. The tests
    //! abort early if one is detected so we don't trash a user's open work.
    use super::*;
    use crate::capture;
    use crate::session::{OutcomeStatus, Session, WindowInfo};
    use std::process::Command;
    use std::thread;
    use std::time::{Duration, Instant};

    const TEST_TARGET_EXE: &str = "mspaint.exe";

    /// RAII guard: on drop, kills any lingering instances of the test target.
    /// Runs even on panic so tests don't pollute the user's desktop.
    struct CleanupGuard;
    impl Drop for CleanupGuard {
        fn drop(&mut self) {
            let _ = Command::new("taskkill")
                .args(["/F", "/IM", TEST_TARGET_EXE])
                .output();
        }
    }

    fn is_test_target(window: &WindowInfo) -> bool {
        window.exe_path.to_lowercase().ends_with(TEST_TARGET_EXE)
    }

    fn count_test_targets() -> usize {
        capture::capture_windows()
            .map(|ws| ws.into_iter().filter(is_test_target).count())
            .unwrap_or(0)
    }

    fn wait_for_target_window(timeout: Duration) -> Option<WindowInfo> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Ok(windows) = capture::capture_windows() {
                if let Some(w) = windows.into_iter().find(is_test_target) {
                    return Some(w);
                }
            }
            thread::sleep(Duration::from_millis(300));
        }
        None
    }

    fn wait_until_target_gone(timeout: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if count_test_targets() == 0 {
                return true;
            }
            thread::sleep(Duration::from_millis(200));
        }
        false
    }

    /// End-to-end: spawn → capture → kill → restore. The restored window must
    /// exist and land at roughly the captured coordinates.
    #[test]
    #[ignore]
    fn restore_relaunches_killed_target() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _cleanup = CleanupGuard;

        assert_eq!(
            count_test_targets(),
            0,
            "Test requires no {} instances open. Please close any before running.",
            TEST_TARGET_EXE
        );

        Command::new(TEST_TARGET_EXE)
            .spawn()
            .expect("failed to spawn test target");

        let captured = wait_for_target_window(Duration::from_secs(10))
            .expect("test target window did not appear within 10s");

        let session = Session::new(
            "integration_test_relaunch".into(),
            vec![captured.clone()],
            vec![],
        );

        let _ = Command::new("taskkill")
            .args(["/F", "/IM", TEST_TARGET_EXE])
            .output();
        assert!(
            wait_until_target_gone(Duration::from_secs(4)),
            "failed to kill test target before restore"
        );

        let report = restore(&session).expect("restore returned Err");

        assert_eq!(report.windows.len(), 1, "expected exactly one window outcome");
        let outcome = &report.windows[0];
        assert!(
            matches!(outcome.status, OutcomeStatus::Success | OutcomeStatus::Partial),
            "expected Success/Partial, got {:?}. message={}, steps={:?}",
            outcome.status,
            outcome.message,
            outcome.steps
        );

        let restored = wait_for_target_window(Duration::from_secs(8))
            .expect("test target window did not reappear after restore");

        let dx = (restored.x - captured.x).abs();
        let dy = (restored.y - captured.y).abs();
        assert!(
            dx < 60 && dy < 60,
            "restored position ({}, {}) too far from captured ({}, {}). delta=({}, {})",
            restored.x,
            restored.y,
            captured.x,
            captured.y,
            dx,
            dy
        );
    }

    /// When the target is already running, restore should find the existing
    /// HWND and reposition it rather than spawning a new instance.
    #[test]
    #[ignore]
    fn restore_repositions_already_running_target() {
        let _ = env_logger::builder().is_test(true).try_init();
        let _cleanup = CleanupGuard;

        assert_eq!(
            count_test_targets(),
            0,
            "Test requires no {} instances open. Please close any before running.",
            TEST_TARGET_EXE
        );

        Command::new(TEST_TARGET_EXE)
            .spawn()
            .expect("failed to spawn test target");

        let original = wait_for_target_window(Duration::from_secs(10))
            .expect("test target window did not appear within 10s");

        let target_info = WindowInfo {
            x: 120,
            y: 120,
            width: 640,
            height: 480,
            ..original.clone()
        };
        let session = Session::new(
            "integration_test_reposition".into(),
            vec![target_info.clone()],
            vec![],
        );

        let report = restore(&session).expect("restore returned Err");

        let outcome = &report.windows[0];
        assert!(
            matches!(outcome.status, OutcomeStatus::Success | OutcomeStatus::Partial),
            "expected Success/Partial, got {:?}. message={}, steps={:?}",
            outcome.status,
            outcome.message,
            outcome.steps
        );

        let trace = outcome.steps.join(" | ").to_lowercase();
        assert!(
            trace.contains("matched saved hwnd") || trace.contains("already running"),
            "expected existing-window path (matched saved HWND or already-running scan); got: {}",
            trace
        );

        assert_eq!(
            count_test_targets(),
            1,
            "expected exactly one target window; restore should not have spawned a second"
        );

        thread::sleep(Duration::from_millis(1000));
        let restored = wait_for_target_window(Duration::from_secs(3))
            .expect("test target window not found after reposition");

        let dx = (restored.x - target_info.x).abs();
        let dy = (restored.y - target_info.y).abs();
        assert!(
            dx < 60 && dy < 60,
            "repositioned to ({}, {}), expected ({}, {}). delta=({}, {})",
            restored.x,
            restored.y,
            target_info.x,
            target_info.y,
            dx,
            dy
        );
    }
}
