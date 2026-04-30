<script lang="ts">
  import type { SessionSummary } from "./api";

  interface Props {
    sessions: SessionSummary[];
    selectedSession: string | null;
    onSelect: (name: string) => void;
  }

  let { sessions, selectedSession, onSelect }: Props = $props();

  function formatDate(iso: string): string {
    const d = new Date(iso);
    return d.toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  }
</script>

<div class="session-list">
  <h2>Sessions</h2>
  {#if sessions.length === 0}
    <p class="empty">No saved sessions yet.</p>
  {:else}
    {#each sessions as session}
      <button
        class="session-item"
        class:selected={selectedSession === session.name}
        onclick={() => onSelect(session.name)}
      >
        <span class="session-name">{session.name}</span>
        <span class="session-meta">
          {session.window_count} apps, {session.tab_count} tabs
        </span>
        <span class="session-date">{formatDate(session.created_at)}</span>
      </button>
    {/each}
  {/if}
</div>

<style>
  .session-list {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  h2 {
    font-size: 0.875rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #888;
    margin: 0 0 0.5rem 0;
  }
  .empty {
    color: #666;
    font-size: 0.85rem;
  }
  .session-item {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    padding: 0.6rem 0.75rem;
    border: 1px solid #333;
    border-radius: 6px;
    background: #252525;
    cursor: pointer;
    text-align: left;
    color: #e0e0e0;
    transition: background 0.15s, border-color 0.15s;
  }
  .session-item:hover {
    background: #2a2a2a;
    border-color: #555;
  }
  .session-item.selected {
    background: #1a3a5c;
    border-color: #4a9eff;
  }
  .session-name {
    font-weight: 600;
    font-size: 0.95rem;
  }
  .session-meta {
    font-size: 0.8rem;
    color: #999;
  }
  .session-date {
    font-size: 0.75rem;
    color: #666;
  }
</style>
