<script lang="ts">
  import type { Session } from "./api";

  interface Props {
    session: Session | null;
    onRestore: (name: string) => void;
    onDelete: (name: string) => void;
    restoring: boolean;
  }

  let { session, onRestore, onDelete, restoring }: Props = $props();

  function getExeName(path: string): string {
    const parts = path.replace(/\\/g, "/").split("/");
    return parts[parts.length - 1] || path;
  }
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
</style>
