import { invoke } from "@tauri-apps/api/core";

export interface SessionSummary {
  name: string;
  created_at: string;
  window_count: number;
  tab_count: number;
}

export interface WindowInfo {
  exe_path: string;
  command_line: string | null;
  title: string;
  x: number;
  y: number;
  width: number;
  height: number;
  show_state: "Normal" | "Minimized" | "Maximized";
  virtual_desktop_index: number | null;
  hwnd: number | null;
}

export interface BraveTab {
  url: string;
  title: string | null;
}

export interface Session {
  name: string;
  created_at: string;
  windows: WindowInfo[];
  brave_tabs: BraveTab[];
}

export type OutcomeStatus = "Success" | "Partial" | "Failed" | "Skipped";

export interface WindowOutcome {
  exe_path: string;
  title: string;
  status: OutcomeStatus;
  message: string;
  steps: string[];
}

export interface BraveOutcome {
  status: OutcomeStatus;
  message: string;
  tab_count: number;
}

export interface RestoreReport {
  session_name: string;
  started_at: string;
  duration_ms: number;
  windows: WindowOutcome[];
  brave: BraveOutcome | null;
}

export async function saveSession(name: string): Promise<Session> {
  return invoke<Session>("save_session", { name });
}

export async function listSessions(): Promise<SessionSummary[]> {
  return invoke<SessionSummary[]>("list_sessions");
}

export async function getSession(name: string): Promise<Session> {
  return invoke<Session>("get_session", { name });
}

export async function deleteSession(name: string): Promise<void> {
  return invoke<void>("delete_session", { name });
}

export async function restoreSession(name: string): Promise<RestoreReport> {
  return invoke<RestoreReport>("restore_session", { name });
}
