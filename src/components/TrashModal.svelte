<script lang="ts">
  import { onMount } from "svelte";
  import { pushBackHandler } from "../lib/backStack";
  import { call } from "../lib/ipc/commands";
  import Icon from "./editors/Icon.svelte";
  import type { NoteSummaryDto, TrashItemDto } from "../lib/ipc/types";

  interface Props {
    onRestored: (note: NoteSummaryDto) => void;
    onClose: () => void;
    embedded?: boolean;
  }

  let { onRestored, onClose, embedded = false }: Props = $props();

  let root: HTMLDivElement;
  let items = $state<TrashItemDto[]>([]);
  let loading = $state(true);
  let error = $state("");
  let pendingDelete = $state<{ trashId: string; title: string } | null>(null);
  let pendingEmptyTrash = $state(false);

  async function loadTrash(): Promise<void> {
    loading = true;
    error = "";
    try {
      items = await call("trash_list");
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function restoreItem(trashId: string): Promise<void> {
    error = "";
    try {
      const restored = await call("trash_restore", { trashId });
      await loadTrash();
      onRestored(restored);
    } catch (e) {
      error = String(e);
    }
  }

  async function confirmDeleteItem(): Promise<void> {
    if (!pendingDelete) return;
    const { trashId } = pendingDelete;
    error = "";
    try {
      await call("trash_delete_item", { trashId });
      pendingDelete = null;
      await loadTrash();
    } catch (e) {
      error = String(e);
    }
  }

  async function confirmEmptyTrash(): Promise<void> {
    if (items.length === 0) return;
    error = "";
    try {
      await call("trash_empty");
      pendingEmptyTrash = false;
      items = [];
    } catch (e) {
      error = String(e);
    }
  }

  function formatDate(ts: number): string {
    if (!ts) return "";
    try {
      return new Date(ts * 1000).toLocaleString("es-ES", {
        dateStyle: "short",
        timeStyle: "short",
      });
    } catch {
      return "";
    }
  }

  function onDocMousedown(e: MouseEvent): void {
    if (!embedded && root && !root.contains(e.target as Node)) onClose();
  }

  function onKeydown(e: KeyboardEvent): void {
    if (!embedded && e.key === "Escape") onClose();
  }

  onMount(() => {
    void loadTrash();
  });

  onMount(() => {
    if (embedded) return;
    return pushBackHandler(() => (onClose(), true));
  });
</script>

<svelte:window onmousedown={onDocMousedown} onkeydown={onKeydown} />

<div class:modal-overlay={!embedded} class:settings-embedded-host={embedded}>
  <div
    class="modal trash-modal"
    class:settings-embedded-modal={embedded}
    bind:this={root}
    role={embedded ? "region" : "dialog"}
    aria-label="Papelera"
  >
    {#if !embedded}
      <div class="modal-header">
        <h2><Icon name="trash" size={16} /> Papelera ({items.length})</h2>
        <button class="icon-btn" onclick={onClose} aria-label="Cerrar">✕</button>
      </div>
    {/if}

    <div class="modal-body trash-body">
      <div class="setting-hint">
        Cada nota permanece en la papelera durante 30 días desde su eliminación individual antes de
        borrarse permanentemente de forma automática.
      </div>
      {#if error}
        <div class="error-banner">{error}</div>
      {/if}

      {#if pendingDelete}
        <div class="vault-delete-confirm">
          <strong>⚠️ ¿Eliminar permanentemente «{pendingDelete.title}»?</strong>
          <p>
            Esta acción borrará la nota de la papelera para siempre. <strong
              >No se puede deshacer.</strong
            >
          </p>
          <div class="vault-delete-actions">
            <button type="button" class="trash-btn" onclick={() => (pendingDelete = null)}>
              Cancelar
            </button>
            <button type="button" class="danger-btn" onclick={() => void confirmDeleteItem()}>
              Sí, eliminar nota
            </button>
          </div>
        </div>
      {/if}

      {#if pendingEmptyTrash}
        <div class="vault-delete-confirm">
          <strong>⚠️ ¿Vaciar toda la papelera ({items.length} notas)?</strong>
          <p>
            Se borrarán permanentemente todas las notas de la papelera. <strong
              >Esta acción NO se puede deshacer.</strong
            >
          </p>
          <div class="vault-delete-actions">
            <button type="button" class="trash-btn" onclick={() => (pendingEmptyTrash = false)}>
              Cancelar
            </button>
            <button type="button" class="danger-btn" onclick={() => void confirmEmptyTrash()}>
              Sí, vaciar papelera
            </button>
          </div>
        </div>
      {/if}

      {#if loading}
        <div class="list-caption">Cargando papelera…</div>
      {:else if items.length === 0}
        <div class="trash-empty">
          <Icon name="trash" size={28} />
          <span>La papelera está vacía.</span>
        </div>
      {:else}
        <div class="trash-list">
          {#each items as item (item.trash_id)}
            <div class="trash-item">
              <div class="trash-info">
                <strong>{item.title}</strong>
                <span class="trash-meta">
                  {formatDate(item.deleted_at)} · {(item.size / 1024).toFixed(1)} KB ·
                  <span class="badge md">
                    {item.days_remaining === 1
                      ? "Queda 1 día"
                      : `Quedan ${item.days_remaining} días`}
                  </span>
                </span>
              </div>
              <div class="trash-actions">
                <button
                  type="button"
                  class="trash-btn restore"
                  title="Restaurar nota"
                  onclick={() => void restoreItem(item.trash_id)}
                >
                  <Icon name="restore" size={15} /> Restaurar
                </button>
                <button
                  type="button"
                  class="trash-btn danger"
                  title="Eliminar permanentemente"
                  onclick={() => (pendingDelete = { trashId: item.trash_id, title: item.title })}
                >
                  <Icon name="trash" size={15} />
                </button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <div class="modal-footer trash-footer">
      <button
        type="button"
        class="danger-btn"
        disabled={items.length === 0 || pendingEmptyTrash}
        onclick={() => (pendingEmptyTrash = true)}
      >
        Vaciar papelera
      </button>
      {#if !embedded}
        <button type="button" class="primary" onclick={onClose}>Cerrar</button>
      {/if}
    </div>
  </div>
</div>
