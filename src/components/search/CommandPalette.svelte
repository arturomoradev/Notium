<script lang="ts">
  import { onMount } from "svelte";
  import { pushBackHandler } from "../../lib/backStack";
  import { call } from "../../lib/ipc/commands";
  import type { SearchHitDto } from "../../lib/ipc/types";

  interface Props {
    onSelect: (noteId: string) => void;
    onClose: () => void;
  }

  let { onSelect, onClose }: Props = $props();

  let query = $state("");
  let hits = $state<SearchHitDto[]>([]);
  let selected = $state(0);
  let inputEl: HTMLInputElement;
  let root: HTMLDivElement;
  let debounce: number | undefined;

  function search(q: string): void {
    query = q;
    if (debounce !== undefined) window.clearTimeout(debounce);
    debounce = window.setTimeout(async () => {
      const value = q.trim();
      if (!value) {
        hits = [];
        return;
      }
      const isTagOnly = /^#[\p{L}\p{N}_/-]+$/u.test(value);
      try {
        hits = await call("note_search_fts", {
          query: isTagOnly ? "" : value,
          tag: isTagOnly ? value.slice(1) : null,
          format: null,
        });
        selected = 0;
      } catch {
        hits = [];
      }
    }, 120);
  }

  function pick(hit: SearchHitDto): void {
    onSelect(hit.note_id);
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      selected = Math.min(selected + 1, hits.length - 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      selected = Math.max(selected - 1, 0);
    } else if (e.key === "Enter") {
      e.preventDefault();
      const hit = hits[selected];
      if (hit) pick(hit);
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }

  function onWindowMousedown(e: MouseEvent): void {
    if (root && !root.contains(e.target as Node)) onClose();
  }

  onMount(() => {
    inputEl.focus();
    window.addEventListener("mousedown", onWindowMousedown);
    return () => window.removeEventListener("mousedown", onWindowMousedown);
  });

  onMount(() => pushBackHandler(() => (onClose(), true)));
</script>

<div class="palette-overlay">
  <div class="palette" bind:this={root} role="dialog" tabindex="-1" aria-label="Buscar notas">
    <input
      class="palette-input"
      bind:this={inputEl}
      placeholder="Buscar notas (FTS)…"
      oninput={(e) => search(e.currentTarget.value)}
      onkeydown={onKeydown}
    />
    <div class="palette-results">
      {#each hits as hit, i (hit.note_id)}
        <div
          class="palette-item"
          class:selected={i === selected}
          role="button"
          tabindex="0"
          onmousedown={(e) => {
            e.preventDefault();
            pick(hit);
          }}
          onmouseenter={() => (selected = i)}
        >
          <span class="badge md">md</span>
          <span>{hit.title}</span>
        </div>
      {/each}
      {#if query.trim() && hits.length === 0}
        <div class="palette-empty">Sin resultados</div>
      {/if}
    </div>
  </div>
</div>
