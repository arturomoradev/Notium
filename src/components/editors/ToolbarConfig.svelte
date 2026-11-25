<script lang="ts">
  import { onMount } from "svelte";
  import { pushBackHandler } from "../../lib/backStack";
  import { editorStore } from "../../lib/stores/editor.svelte";
  import Icon from "./Icon.svelte";
  import { COMMANDS_BY_ID, TOOLBAR_COMMANDS } from "./toolbar";

  interface Props {
    onClose: () => void;
    embedded?: boolean;
  }

  let { onClose, embedded = false }: Props = $props();

  let root: HTMLDivElement;
  let list = $state<HTMLDivElement>();
  let dragId = $state<string | null>(null);

  const ids = $derived(editorStore.toolbarIds);
  const hidden = $derived(TOOLBAR_COMMANDS.filter((c) => !ids.includes(c.id)));

  function startDrag(e: PointerEvent, id: string): void {
    e.preventDefault();
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    dragId = id;
  }

  function moveDrag(e: PointerEvent): void {
    if (!dragId || !list) return;
    const scroller = list.closest(".modal-body");
    if (scroller) {
      const b = scroller.getBoundingClientRect();
      if (e.clientY < b.top + 40) scroller.scrollBy(0, -10);
      else if (e.clientY > b.bottom - 40) scroller.scrollBy(0, 10);
    }
    const rows = [...list.querySelectorAll<HTMLElement>("[data-id]")];
    const over = rows.findIndex((r) => {
      const b = r.getBoundingClientRect();
      return e.clientY >= b.top && e.clientY < b.bottom;
    });
    if (over < 0 || rows[over].dataset.id === dragId) return;
    const next = ids.filter((i) => i !== dragId);
    next.splice(over, 0, dragId);
    editorStore.setToolbar(next);
  }

  function onDocMousedown(e: MouseEvent): void {
    if (!embedded && root && !root.contains(e.target as Node)) onClose();
  }

  function onKeydown(e: KeyboardEvent): void {
    if (!embedded && e.key === "Escape") onClose();
  }

  onMount(() => {
    if (embedded) return;
    return pushBackHandler(() => (onClose(), true));
  });
</script>

<svelte:window onmousedown={onDocMousedown} onkeydown={onKeydown} />

<div class:modal-overlay={!embedded} class:settings-embedded-host={embedded}>
  <div
    class="modal toolbar-config"
    class:settings-embedded-modal={embedded}
    bind:this={root}
    role={embedded ? "region" : "dialog"}
    aria-label="Configurar barra de herramientas"
  >
    {#if !embedded}
      <div class="modal-header">
        <h2><Icon name="wrench" size={16} /> Configurar barra de herramientas</h2>
        <button class="icon-btn" onclick={onClose} aria-label="Cerrar">✕</button>
      </div>
    {/if}

    <div class="modal-body">
      <div class="settings-section-label">Opciones de la barra</div>
      <div class="tb-list" bind:this={list}>
        {#each ids as id (id)}
          {@const c = COMMANDS_BY_ID.get(id)!}
          <div class="tb-row" class:dragging={dragId === id} data-id={id}>
            <Icon name={c.icon} size={18} />
            <span class="tb-name">{c.title}</span>
            <button
              type="button"
              class="tb-icon-btn"
              title="Quitar de la barra"
              aria-label="Quitar {c.title} de la barra"
              onclick={() => editorStore.setToolbar(ids.filter((i) => i !== id))}
            >
              <Icon name="x" size={18} />
            </button>
            <span
              class="tb-icon-btn tb-handle"
              role="button"
              tabindex="-1"
              aria-label="Arrastrar para reordenar {c.title}"
              onpointerdown={(e) => startDrag(e, id)}
              onpointermove={moveDrag}
              onpointerup={() => (dragId = null)}
              onpointercancel={() => (dragId = null)}
            >
              <Icon name="menu" size={18} />
            </span>
          </div>
        {/each}
        {#if ids.length === 0}
          <div class="setting-hint">La barra está vacía. Añade comandos desde abajo.</div>
        {/if}
      </div>

      {#if hidden.length > 0}
        <div class="settings-section-label">Más opciones</div>
        <div class="tb-list">
          {#each hidden as c (c.id)}
            <button
              type="button"
              class="tb-row tb-add"
              onclick={() => editorStore.setToolbar([...ids, c.id])}
            >
              <Icon name={c.icon} size={18} />
              <span class="tb-name">{c.title}</span>
              <Icon name="plus" size={18} />
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>
