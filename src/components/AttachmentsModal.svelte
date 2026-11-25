<script lang="ts">
  import { onMount } from "svelte";
  import { pushBackHandler } from "../lib/backStack";
  import Icon from "./editors/Icon.svelte";
  import type { AttachmentItemDto } from "../lib/ipc/types";
  import { call, readAttachment } from "../lib/ipc/commands";

  interface Props {
    onOpenNote: (noteId: string) => void;
    onClose: () => void;
    embedded?: boolean;
  }

  let { onOpenNote, onClose, embedded = false }: Props = $props();

  let root: HTMLDivElement;
  let items = $state<AttachmentItemDto[]>([]);
  let loading = $state(true);
  let error = $state("");
  let filter = $state<"all" | "images" | "orphans">("all");
  let searchQuery = $state("");
  let pendingDelete = $state<AttachmentItemDto | null>(null);
  let pendingDeleteOrphans = $state(false);
  let copiedPath = $state("");
  let previews = $state<Record<string, string>>({});
  let failedPreviews = $state<Record<string, boolean>>({});

  function isImage(filename: string): boolean {
    const ext = filename.split(".").pop()?.toLowerCase() ?? "";
    return ["png", "jpg", "jpeg", "webp", "gif", "svg", "bmp", "ico"].includes(ext);
  }

  function getMime(filename: string): string {
    const ext = filename.split(".").pop()?.toLowerCase() ?? "";
    switch (ext) {
      case "png":
        return "image/png";
      case "jpg":
      case "jpeg":
        return "image/jpeg";
      case "webp":
        return "image/webp";
      case "gif":
        return "image/gif";
      case "svg":
        return "image/svg+xml";
      case "ico":
        return "image/x-icon";
      default:
        return "application/octet-stream";
    }
  }

  async function loadAttachments(): Promise<void> {
    loading = true;
    error = "";
    try {
      items = await call("attachment_list");
      void loadPreviews();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function loadPreviews(): Promise<void> {
    const imageItems = items.filter((it) => isImage(it.filename) && !previews[it.path]);
    for (const it of imageItems) {
      try {
        const u8 = await readAttachment(it.path);
        const mime = getMime(it.filename);
        const blob = new Blob([u8], { type: mime });
        const url = URL.createObjectURL(blob);
        previews = { ...previews, [it.path]: url };
      } catch {
        // Skip unreadable preview
      }
    }
  }

  async function deleteAttachment(item: AttachmentItemDto): Promise<void> {
    error = "";
    try {
      await call("attachment_delete", { path: item.path });
      if (previews[item.path]) {
        URL.revokeObjectURL(previews[item.path]);
        const copy = { ...previews };
        delete copy[item.path];
        previews = copy;
      }
      pendingDelete = null;
      await loadAttachments();
    } catch (e) {
      error = String(e);
    }
  }

  async function deleteOrphans(): Promise<void> {
    error = "";
    try {
      await call("attachment_delete_orphans");
      pendingDeleteOrphans = false;
      await loadAttachments();
    } catch (e) {
      error = String(e);
    }
  }

  async function copyEmbed(filename: string): Promise<void> {
    const tag = `![[${filename}]]`;
    try {
      await navigator.clipboard.writeText(tag);
      copiedPath = filename;
      setTimeout(() => {
        if (copiedPath === filename) copiedPath = "";
      }, 2000);
    } catch (e) {
      error = `No se pudo copiar: ${String(e)}`;
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  let imagesCount = $derived(items.filter((it) => isImage(it.filename)).length);
  let orphansCount = $derived(items.filter((it) => it.is_orphan).length);

  let filteredItems = $derived(
    items.filter((it) => {
      if (filter === "images" && !isImage(it.filename)) return false;
      if (filter === "orphans" && !it.is_orphan) return false;
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        const matchesName = it.filename.toLowerCase().includes(q);
        const matchesRef = it.referencing_notes.some((r) => r.title.toLowerCase().includes(q));
        if (!matchesName && !matchesRef) return false;
      }
      return true;
    }),
  );

  function onDocMousedown(e: MouseEvent): void {
    if (!embedded && root && !root.contains(e.target as Node)) onClose();
  }

  function onKeydown(e: KeyboardEvent): void {
    if (!embedded && e.key === "Escape") onClose();
  }

  onMount(() => {
    void loadAttachments();
    return () => {
      for (const url of Object.values(previews)) {
        URL.revokeObjectURL(url);
      }
    };
  });

  onMount(() => {
    if (embedded) return;
    return pushBackHandler(() => (onClose(), true));
  });
</script>

<svelte:window onmousedown={onDocMousedown} onkeydown={onKeydown} />

<div class:modal-overlay={!embedded} class:settings-embedded-host={embedded}>
  <div
    class="modal attachments-modal"
    class:settings-embedded-modal={embedded}
    bind:this={root}
    role={embedded ? "region" : "dialog"}
    aria-label="Gestor de Adjuntos"
  >
    {#if !embedded}
      <div class="modal-header">
        <h2>📎 Archivos Adjuntos ({items.length})</h2>
        <button class="icon-btn" onclick={onClose} aria-label="Cerrar">✕</button>
      </div>
    {/if}

    <div class="modal-body attachments-body">
      <div class="attachments-toolbar">
        <div class="attachments-tabs">
          <button
            type="button"
            class="att-tab-btn"
            class:active={filter === "all"}
            onclick={() => (filter = "all")}
          >
            Todos ({items.length})
          </button>
          <button
            type="button"
            class="att-tab-btn"
            class:active={filter === "images"}
            onclick={() => (filter = "images")}
          >
            Imágenes ({imagesCount})
          </button>
          <button
            type="button"
            class="att-tab-btn"
            class:active={filter === "orphans"}
            onclick={() => (filter = "orphans")}
          >
            Huérfanos ({orphansCount})
          </button>
        </div>

        {#if orphansCount > 0}
          <button
            type="button"
            class="danger-btn small"
            onclick={() => (pendingDeleteOrphans = true)}
          >
            Eliminar huérfanos ({orphansCount})
          </button>
        {/if}
      </div>

      <div class="attachments-search">
        <input
          type="text"
          placeholder="Buscar por nombre o nota que lo usa…"
          bind:value={searchQuery}
        />
      </div>

      {#if error}
        <div class="error-banner">{error}</div>
      {/if}

      {#if pendingDelete}
        <div class="vault-delete-confirm">
          <strong>⚠️ ¿Eliminar adjunto «{pendingDelete.filename}»?</strong>
          {#if !pendingDelete.is_orphan}
            <p>
              Este archivo está en uso por <strong
                >{pendingDelete.referencing_notes.length} nota(s)</strong
              >. Si lo eliminas, los enlaces e incrustaciones dejarán de mostrarse.
            </p>
          {:else}
            <p>Este archivo es huérfano (no está referenciado por ninguna nota activa).</p>
          {/if}
          <div class="vault-delete-actions">
            <button type="button" class="trash-btn" onclick={() => (pendingDelete = null)}>
              Cancelar
            </button>
            <button
              type="button"
              class="danger-btn"
              onclick={() => void deleteAttachment(pendingDelete!)}
            >
              Sí, eliminar adjunto
            </button>
          </div>
        </div>
      {/if}

      {#if pendingDeleteOrphans}
        <div class="vault-delete-confirm">
          <strong>⚠️ ¿Eliminar todos los {orphansCount} adjuntos huérfanos?</strong>
          <p>
            Esta acción borrará del Vault los archivos que no están enlazados por ninguna nota. <strong
              >No se puede deshacer.</strong
            >
          </p>
          <div class="vault-delete-actions">
            <button type="button" class="trash-btn" onclick={() => (pendingDeleteOrphans = false)}>
              Cancelar
            </button>
            <button type="button" class="danger-btn" onclick={() => void deleteOrphans()}>
              Sí, eliminar todos los huérfanos
            </button>
          </div>
        </div>
      {/if}

      {#if loading}
        <div class="list-caption">Cargando adjuntos…</div>
      {:else if items.length === 0}
        <div class="trash-empty">
          <Icon name="attachment" size={28} />
          <span>El Vault no tiene archivos adjuntos en <code>attachments/</code>.</span>
        </div>
      {:else if filteredItems.length === 0}
        <div class="trash-empty">
          <span>No hay adjuntos que coincidan con el filtro actual.</span>
        </div>
      {:else}
        <div class="attachments-grid">
          {#each filteredItems as item (item.att_id)}
            <div class="attachment-card" class:is-orphan={item.is_orphan}>
              <div class="att-preview">
                {#if previews[item.path] && !failedPreviews[item.path]}
                  <img
                    src={previews[item.path]}
                    alt={item.filename}
                    loading="lazy"
                    onerror={() => {
                      failedPreviews = { ...failedPreviews, [item.path]: true };
                    }}
                  />
                {:else if isImage(item.filename)}
                  <div class="att-placeholder">🖼️</div>
                {:else}
                  <div class="att-placeholder">📄</div>
                {/if}
              </div>

              <div class="att-meta">
                <span class="att-filename" title={item.filename}>{item.filename}</span>
                <span class="att-size">{formatBytes(item.file_size)}</span>

                <div class="att-status">
                  {#if item.is_orphan}
                    <span class="badge orphan-badge">⚠️ Huérfano</span>
                  {:else}
                    <span class="att-refs-label">Usado en:</span>
                    <div class="att-refs-list">
                      {#each item.referencing_notes as ref (ref.note_id)}
                        <button
                          type="button"
                          class="att-ref-pill"
                          title="Abrir nota «{ref.title}»"
                          onclick={() => {
                            onOpenNote(ref.note_id);
                            onClose();
                          }}
                        >
                          📄 {ref.title}
                        </button>
                      {/each}
                    </div>
                  {/if}
                </div>
              </div>

              <div class="att-actions">
                <button
                  type="button"
                  class="trash-btn small"
                  title="Copiar sintaxis ![[{item.filename}]]"
                  onclick={() => void copyEmbed(item.filename)}
                >
                  {#if copiedPath === item.filename}
                    ✓ Copiado
                  {:else}
                    Copiar ![[…]]
                  {/if}
                </button>
                <button
                  type="button"
                  class="trash-btn danger small"
                  title="Eliminar adjunto"
                  onclick={() => (pendingDelete = item)}
                  aria-label="Eliminar"
                >
                  <Icon name="trash" size={14} />
                </button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    {#if !embedded}
      <div class="modal-footer">
        <button type="button" class="primary" onclick={onClose}>Cerrar</button>
      </div>
    {/if}
  </div>
</div>
