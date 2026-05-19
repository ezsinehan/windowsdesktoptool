<script lang="ts">
  import { onMount } from "svelte";
  import SessionList from "./lib/SessionList.svelte";
  import SessionDetail from "./lib/SessionDetail.svelte";
  import SaveDialog from "./lib/SaveDialog.svelte";
  import {
    listSessions,
    getSession,
    saveSession,
    deleteSession,
    restoreSession,
    type SessionSummary,
    type Session,
    type RestoreReport,
  } from "./lib/api";

  let sessions = $state<SessionSummary[]>([]);
  let selectedName = $state<string | null>(null);
  let selectedSession = $state<Session | null>(null);
  let showSaveDialog = $state(false);
  let saving = $state(false);
  let restoring = $state(false);
  let error = $state<string | null>(null);
  let lastReport = $state<RestoreReport | null>(null);

  onMount(() => {
    refreshSessions();
  });

  async function refreshSessions() {
    try {
      sessions = await listSessions();
    } catch (e) {
      error = `Failed to load sessions: ${e}`;
    }
  }

  async function handleSelect(name: string) {
    selectedName = name;
    lastReport = null;
    try {
      selectedSession = await getSession(name);
      error = null;
    } catch (e) {
      error = `Failed to load session: ${e}`;
    }
  }

  async function handleSave(name: string) {
    saving = true;
    error = null;
    try {
      await saveSession(name);
      showSaveDialog = false;
      await refreshSessions();
      await handleSelect(name);
    } catch (e) {
      error = `Failed to save session: ${e}`;
    } finally {
      saving = false;
    }
  }

  async function handleRestore(name: string) {
    restoring = true;
    error = null;
    lastReport = null;
    try {
      lastReport = await restoreSession(name);
    } catch (e) {
      error = `Failed to restore session: ${e}`;
    } finally {
      restoring = false;
    }
  }

  async function handleDelete(name: string) {
    error = null;
    try {
      await deleteSession(name);
      if (selectedName === name) {
        selectedName = null;
        selectedSession = null;
      }
      await refreshSessions();
    } catch (e) {
      error = `Failed to delete session: ${e}`;
    }
  }
</script>

<main>
  <header>
    <h1>Desktop Session Manager</h1>
    <button class="btn-save-session" onclick={() => (showSaveDialog = true)}>
      + Save Current Session
    </button>
  </header>

  {#if error}
    <div class="error-bar">
      <span>{error}</span>
      <button onclick={() => (error = null)}>&times;</button>
    </div>
  {/if}

  <div class="layout">
    <aside>
      <SessionList
        {sessions}
        selectedSession={selectedName}
        onSelect={handleSelect}
      />
    </aside>
    <section>
      <SessionDetail
        session={selectedSession}
        onRestore={handleRestore}
        onDelete={handleDelete}
        {restoring}
        report={lastReport}
      />
    </section>
  </div>

  <SaveDialog
    open={showSaveDialog}
    onSave={handleSave}
    onCancel={() => (showSaveDialog = false)}
    {saving}
  />
</main>

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    background: #1a1a1a;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    color: #e0e0e0;
  }

  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }

  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.75rem 1.25rem;
    background: #222;
    border-bottom: 1px solid #333;
  }

  header h1 {
    margin: 0;
    font-size: 1.1rem;
    font-weight: 600;
    color: #fff;
  }

  .btn-save-session {
    padding: 0.4rem 1rem;
    border: none;
    border-radius: 5px;
    background: #4a9eff;
    color: #fff;
    font-size: 0.85rem;
    font-weight: 500;
    cursor: pointer;
  }

  .btn-save-session:hover {
    background: #3a8eef;
  }

  .error-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.5rem 1rem;
    background: #3d1f1f;
    border-bottom: 1px solid #5a2a2a;
    color: #ff8888;
    font-size: 0.85rem;
  }

  .error-bar button {
    background: none;
    border: none;
    color: #ff8888;
    font-size: 1.1rem;
    cursor: pointer;
  }

  .layout {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  aside {
    width: 260px;
    min-width: 260px;
    padding: 1rem;
    border-right: 1px solid #333;
    overflow-y: auto;
    background: #1e1e1e;
  }

  section {
    flex: 1;
    padding: 1rem 1.25rem;
    overflow-y: auto;
  }
</style>
