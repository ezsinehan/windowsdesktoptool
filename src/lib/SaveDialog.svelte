<script lang="ts">
  interface Props {
    open: boolean;
    onSave: (name: string) => void;
    onCancel: () => void;
    saving: boolean;
  }

  let { open, onSave, onCancel, saving }: Props = $props();
  let name = $state("");

  function handleSubmit(e: Event) {
    e.preventDefault();
    const trimmed = name.trim();
    if (trimmed) {
      onSave(trimmed);
      name = "";
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      onCancel();
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="overlay" onkeydown={handleKeydown} onclick={onCancel}>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="dialog" onclick={(e) => e.stopPropagation()}>
      <h2>Save Current Session</h2>
      <form onsubmit={handleSubmit}>
        <input
          type="text"
          placeholder="Session name..."
          bind:value={name}
          disabled={saving}
          autofocus
        />
        <div class="dialog-actions">
          <button type="button" class="btn btn-cancel" onclick={onCancel} disabled={saving}>
            Cancel
          </button>
          <button type="submit" class="btn btn-save" disabled={saving || !name.trim()}>
            {#if saving}Saving...{:else}Save{/if}
          </button>
        </div>
      </form>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .dialog {
    background: #2a2a2a;
    border: 1px solid #444;
    border-radius: 8px;
    padding: 1.5rem;
    width: 350px;
  }
  .dialog h2 {
    margin: 0 0 1rem 0;
    font-size: 1.1rem;
    color: #fff;
  }
  input {
    width: 100%;
    padding: 0.5rem 0.75rem;
    border: 1px solid #555;
    border-radius: 5px;
    background: #1e1e1e;
    color: #e0e0e0;
    font-size: 0.95rem;
    margin-bottom: 1rem;
    box-sizing: border-box;
  }
  input:focus {
    outline: none;
    border-color: #4a9eff;
  }
  .dialog-actions {
    display: flex;
    justify-content: flex-end;
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
  .btn-cancel {
    background: #444;
    color: #ccc;
  }
  .btn-cancel:hover {
    background: #555;
  }
  .btn-save {
    background: #4a9eff;
    color: #fff;
  }
  .btn-save:hover {
    background: #3a8eef;
  }
  .btn-save:disabled {
    background: #555;
    cursor: not-allowed;
  }
</style>
