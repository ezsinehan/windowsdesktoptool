<script lang="ts">
  import type { Session, RestoreReport, OutcomeStatus } from "./api";

  interface Props {
    session: Session | null;
    onRestore: (name: string) => void;
    onDelete: (name: string) => void;
    restoring: boolean;
    report: RestoreReport | null;
  }

  let { session, onRestore, onDelete, restoring, report }: Props = $props();

  let expanded = $state<Record<number, boolean>>({});

  function getExeName(path: string): string {
    const parts = path.replace(/\\/g, "/").split("/");
    return parts[parts.length - 1] || path;
  }

  function statusColor(status: OutcomeStatus): string {
    switch (status) {
      case "Success": return "#4ade80";
      case "Partial": return "#facc15";
      case "Failed":  return "#f87171";
      case "Skipped": return "#888";
    }
  }

  function statusLabel(status: OutcomeStatus): string {
    return status.toUpperCase();
  }

  let summary = $derived.by(() => {
    if (!report) return null;
    const counts = { Success: 0, Partial: 0, Failed: 0, Skipped: 0 };
    for (const w of report.windows) counts[w.status]++;
    return counts;
  });
</script>

<div class="detail">
  {#if !session}
    <div class="placeholder">
      <p>Select a session to view details</p>
    </div>
  {:else}
    <div class="detail-header">
      <h2>{session.name}</h2>
      <div class="actions">
        <button class="btn btn-primary" onclick={() => onRestore(session!.name)} disabled={restoring}>
          {#if restoring}
            Restoring...
          {:else}
            Restore
          {/if}
        </button>
        <button class="btn btn-danger" onclick={() => onDelete(session!.name)}>
          Delete
        </button>
      </div>
    </div>

    {#if report && summary}
      <div class="report">
        <div class="report-header">
          <h3>Last Restore Report</h3>
          <span class="report-meta">
            {(report.duration_ms / 1000).toFixed(1)}s &middot;
            <span style="color: {statusColor('Success')}">{summary.Success} ok</span>
            {#if summary.Partial > 0} &middot; <span style="color: {statusColor('Partial')}">{summary.Partial} partial</span>{/if}
            {#if summary.Failed > 0} &middot; <span style="color: {statusColor('Failed')}">{summary.Failed} failed</span>{/if}
            {#if summary.Skipped > 0} &middot; <span style="color: {statusColor('Skipped')}">{summary.Skipped} skipped</span>{/if}
          </span>
        </div>

        <div class="outcome-list">
          {#each report.windows as outcome, i}
            <div class="outcome">
              <button
                class="outcome-row"
                onclick={() => (expanded[i] = !expanded[i])}
                title="Click to {expanded[i] ? 'hide' : 'show'} step trace"
              >
                <span class="status-badge" style="background: {statusColor(outcome.status)}1a; color: {statusColor(outcome.status)};">
                  {statusLabel(outcome.status)}
                </span>
                <span class="outcome-name">{getExeName(outcome.exe_path)}</span>
                <span class="outcome-message">{outcome.message}</span>
                <span class="chevron">{expanded[i] ? "▾" : "▸"}</span>
              </button>
              {#if expanded[i]}
                <ol class="steps">
                  {#each outcome.steps as step}
                    <li>{step}</li>
                  {/each}
                </ol>
              {/if}
            </div>
          {/each}

          {#if report.brave}
            <div class="outcome">
              <div class="outcome-row outcome-row-static">
                <span class="status-badge" style="background: {statusColor(report.brave.status)}1a; color: {statusColor(report.brave.status)};">
                  {statusLabel(report.brave.status)}
                </span>
                <span class="outcome-name">Brave ({report.brave.tab_count} tabs)</span>
                <span class="outcome-message">{report.brave.message}</span>
              </div>
            </div>
          {/if}
        </div>
      </div>
    {/if}

    <div class="section">
      <h3>Applications ({session.windows.length})</h3>
      {#if session.windows.length === 0}
        <p class="empty">No applications captured.</p>
      {:else}
        <div class="app-list">
          {#each session.windows as win}
            <div class="app-item">
              <span class="app-name">{getExeName(win.exe_path)}</span>
              <span class="app-title">{win.title}</span>
              <span class="app-meta">
                {win.width}x{win.height} at ({win.x}, {win.y})
                {#if win.show_state !== "Normal"} &middot; {win.show_state}{/if}
                {#if win.virtual_desktop_index != null} &middot; Desktop {win.virtual_desktop_index + 1}{/if}
              </span>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    {#if session.brave_tabs.length > 0}
      <div class="section">
        <h3>Brave Tabs ({session.brave_tabs.length})</h3>
        <div class="tab-list">
          {#each session.brave_tabs as tab}
            <div class="tab-item">
              <span class="tab-title">{tab.title || "Untitled"}</span>
              <span class="tab-url">{tab.url}</span>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .detail {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: #666;
  }
  .detail-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1rem;
  }
  .detail-header h2 {
    margin: 0;
    font-size: 1.25rem;
    color: #fff;
  }
  .actions {
    display: flex;
    gap: 0.5rem;
  }
  .btn {
    padding: 0.4rem 1rem;
    border: none;
    border-radius: 5px;
    cursor: pointer;
    font-size: 0.85rem;
    font-weight: 500;
  }
  .btn-primary {
    background: #4a9eff;
    color: #fff;
  }
  .btn-primary:hover {
    background: #3a8eef;
  }
  .btn-primary:disabled {
    background: #555;
    cursor: not-allowed;
  }
  .btn-danger {
    background: #444;
    color: #ff6b6b;
  }
  .btn-danger:hover {
    background: #553333;
  }
  .section {
    margin-bottom: 1rem;
  }
  .section h3 {
    font-size: 0.85rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #888;
    margin: 0 0 0.5rem 0;
  }
  .app-list, .tab-list {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    max-height: 300px;
    overflow-y: auto;
  }
  .app-item, .tab-item {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    padding: 0.4rem 0.6rem;
    background: #252525;
    border-radius: 4px;
    border: 1px solid #333;
  }
  .app-name {
    font-weight: 600;
    font-size: 0.9rem;
    color: #e0e0e0;
  }
  .app-title {
    font-size: 0.8rem;
    color: #aaa;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .app-meta {
    font-size: 0.75rem;
    color: #666;
  }
  .tab-title {
    font-size: 0.85rem;
    color: #e0e0e0;
  }
  .tab-url {
    font-size: 0.75rem;
    color: #4a9eff;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .empty {
    color: #666;
    font-size: 0.85rem;
  }

  .report {
    background: #1c1f26;
    border: 1px solid #2d3340;
    border-radius: 6px;
    padding: 0.75rem;
    margin-bottom: 1rem;
  }
  .report-header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin-bottom: 0.5rem;
  }
  .report-header h3 {
    margin: 0;
    font-size: 0.85rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #aaa;
  }
  .report-meta {
    font-size: 0.75rem;
    color: #888;
  }
  .outcome-list {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    max-height: 320px;
    overflow-y: auto;
  }
  .outcome {
    background: #232730;
    border: 1px solid #2f3543;
    border-radius: 4px;
  }
  .outcome-row {
    display: grid;
    grid-template-columns: auto 1fr 2fr auto;
    gap: 0.5rem;
    align-items: center;
    width: 100%;
    padding: 0.4rem 0.6rem;
    background: none;
    border: none;
    color: inherit;
    text-align: left;
    cursor: pointer;
    font: inherit;
  }
  .outcome-row:hover {
    background: #2a2f3a;
  }
  .outcome-row-static {
    cursor: default;
  }
  .outcome-row-static:hover {
    background: none;
  }
  .status-badge {
    font-size: 0.65rem;
    font-weight: 700;
    letter-spacing: 0.05em;
    padding: 0.15rem 0.4rem;
    border-radius: 3px;
  }
  .outcome-name {
    font-size: 0.85rem;
    color: #e0e0e0;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .outcome-message {
    font-size: 0.78rem;
    color: #aaa;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chevron {
    color: #666;
    font-size: 0.8rem;
  }
  .steps {
    margin: 0;
    padding: 0.25rem 0.75rem 0.5rem 2.25rem;
    font-size: 0.75rem;
    color: #999;
    line-height: 1.5;
  }
  .steps li {
    padding: 0.1rem 0;
  }
</style>
