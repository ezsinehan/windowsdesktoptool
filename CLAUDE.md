# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Windows desktop session manager (Tauri 2 + Svelte 5 + TypeScript on the frontend, Rust + windows-rs on the backend). Captures the set of open top-level windows (exe path, position, size, show state, virtual desktop) plus open Brave tabs into a named "session" JSON file, then can relaunch and reposition everything.

## Common commands

Run from repo root unless noted.

- `npm run tauri dev` — Launches Vite (port 1420) + the Tauri shell. This is the primary dev loop; the Rust crate rebuilds on save.
- `npm run tauri build` — Production bundle (release Rust binary + bundled installers under `src-tauri/target/release/bundle/`).
- `npm run dev` / `npm run build` — Frontend only (rarely useful in isolation; the app needs the Tauri runtime).
- `cargo check --manifest-path src-tauri/Cargo.toml` — Fast type-check the Rust side without rebuilding the whole app.
- `cargo build --manifest-path src-tauri/Cargo.toml --release` — Build the Rust binary directly (skips frontend bundling).
- `RUST_LOG=debug npm run tauri dev` — Bump log verbosity. Default is `info`; logger is initialized in `src-tauri/src/lib.rs`.

There is no test suite and no lint config beyond `svelte-check` (not wired into a script).

## Architecture

### Frontend ↔ backend bridge

The frontend never touches OS APIs directly. All five operations go through Tauri commands declared in `src-tauri/src/lib.rs` and invoked from `src/lib/api.ts`:

| Frontend (`api.ts`) | Rust handler (`lib.rs`) |
| --- | --- |
| `saveSession(name)` | `save_session` → `capture::capture_windows` + `brave::capture_tabs` → `session::save` |
| `listSessions()` | `list_sessions` → `session::list` |
| `getSession(name)` | `get_session` → `session::load` |
| `deleteSession(name)` | `delete_session` → `session::delete` |
| `restoreSession(name)` | `restore_session` → `session::load` + `restore::restore` |

When adding a new command: register it in `tauri::generate_handler!` in `lib.rs::run`, add a typed wrapper in `src/lib/api.ts`, and update the Svelte UI in `src/App.svelte` / `src/lib/*.svelte`.

### Rust module layout (`src-tauri/src/`)

- `lib.rs` — Tauri command surface and `run()` entry point. `main.rs` is a thin shim that calls `lib::run()`.
- `session.rs` — Defines `Session`, `WindowInfo`, `BraveTab`, `ShowState`, `SessionSummary`. All on-disk persistence lives here. Sessions are JSON files at `%APPDATA%/windowsdesktoptool/sessions/<sanitized-name>.json` (filenames non-alphanumerics replaced with `_`).
- `capture.rs` — Walks top-level windows via `EnumWindows`. Filters out invisible, cloaked, untitled windows, system shell processes (see `EXCLUDED_EXES`), and our own window (matched by title `"Desktop Session Manager"`).
- `restore.rs` — For each saved window: if the exe is already running, find its HWND and reposition; otherwise spawn the exe, poll for its window (`wait_for_window`, 15s timeout), then `SetWindowPlacement`. After repositioning, optionally moves it to the saved virtual desktop. Brave tabs are restored last by spawning Brave with URLs as args.
- `brave.rs` — Captures tabs via Chrome DevTools Protocol (`http://localhost:9222/json`). **Brave must be launched with `--remote-debugging-port=9222`**, otherwise tab capture silently returns empty (errors are logged but not propagated). Restore relaunches Brave with `--new-window <url1> <url2> ...`. Brave exe is searched in `Program Files`, `Program Files (x86)`, and `%LOCALAPPDATA%/BraveSoftware/...`.
- `vdesktop.rs` — Manual COM vtable for `IVirtualDesktopManager` (the public IID has only `IsWindowOnCurrentVirtualDesktop` / `GetWindowDesktopId` / `MoveWindowToDesktop`). Desktop **index ↔ GUID** translation is done by reading the binary `VirtualDesktopIDs` value under `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops` and slicing 16-byte GUIDs out of it. This is unstable Windows internals — expect breakage on Windows updates.
- `winapi_helpers.rs` — Thin safe wrappers over `IsWindowVisible`, `DwmGetWindowAttribute(DWMWA_CLOAKED)`, `GetWindowText`, `GetWindowThreadProcessId`, `QueryFullProcessImageNameW`, `GetWindowPlacement`. Touch this when adding new Win32 calls so the unsafe surface stays in one place.

### Frontend layout (`src/`)

`App.svelte` owns all state (sessions list, selected session, dialog open, error bar) using Svelte 5 runes (`$state`). Children (`SessionList`, `SessionDetail`, `SaveDialog`) are presentational and receive callbacks via props. There is no router and no global store.

## Things to know before changing code

- **Tauri v2, Svelte 5** — Use the runes API (`$state`, `$derived`, props as function args), not Svelte 4 stores or `export let`. Tauri commands use `invoke` from `@tauri-apps/api/core` (not `/tauri`).
- **Console window stays visible in release builds.** `main.rs` does NOT use `#![windows_subsystem = "windows"]` because logs are useful during save/restore. Don't add the attribute unless you also wire logs to a file.
- **`bundle.icon` in `tauri.conf.json` references files under `src-tauri/icons/` that may not all exist** — keep the list and the directory in sync or `tauri build` will fail.
- **`virtual_desktop_index` is best-effort.** If the COM call or registry read fails, the field is `None` and the window restores to the current desktop without complaint. Don't make the restore path fail-hard on virtual desktop errors.
- **Window matching on restore is by exe path, not HWND.** If the user has multiple instances of the same exe, restore will reposition whichever the enumeration finds first. `command_line` is currently captured as `None` — wiring that up is the path to per-instance disambiguation.
