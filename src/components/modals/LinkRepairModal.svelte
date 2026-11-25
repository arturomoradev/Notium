<script lang="ts">
  import type { LinkReferenceDto, NoteSummaryDto } from "../../lib/ipc/types";

  interface Props {
    note: NoteSummaryDto;
    newTitle: string;
    references: LinkReferenceDto[];
    onUpdateLinks: () => void;
    onKeepAlias: () => void;
    onCancel: () => void;
  }

  let { note: _note, newTitle, references, onUpdateLinks, onKeepAlias, onCancel }: Props = $props();
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="modal-overlay" onclick={onCancel}>
  <div
    class="modal tree-action-modal"
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    tabindex="-1"
  >
    <div class="modal-header">
      <h2>🔗 ¿Actualizar enlaces internos?</h2>
      <button class="icon-btn" onclick={onCancel} aria-label="Cerrar">✕</button>
    </div>

    <div class="modal-body tree-modal-body">
      <p class="tree-modal-text">
        Has renombrado la nota a <strong>«{newTitle}»</strong>.
      </p>
      <p class="tree-modal-text">
        Se encontraron <strong>{references.reduce((acc, r) => acc + r.count, 0)} enlace(s)</strong>
        en
        <strong>{references.length} nota(s)</strong> apuntando al nombre anterior:
      </p>
      <div class="repair-links-list">
        {#each references as ref (ref.note_id)}
          <div class="repair-link-item">
            <span class="repair-link-title">📄 {ref.title}</span>
            <span class="badge">{ref.count} {ref.count === 1 ? "enlace" : "enlaces"}</span>
          </div>
        {/each}
      </div>
      <p class="tree-modal-text muted-text" style="font-size: 12px; margin-top: 8px;">
        ¿Deseas actualizar el texto en los archivos de esas notas para que apunten a «{newTitle}»?
        Si eliges conservar alias, los enlaces seguirán resolviéndose mediante el índice.
      </p>
    </div>

    <div class="modal-footer">
      <button type="button" class="trash-btn" onclick={onKeepAlias}> Conservar alias </button>
      <button type="button" class="primary" onclick={onUpdateLinks}> Actualizar enlaces </button>
    </div>
  </div>
</div>
