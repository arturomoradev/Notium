<script lang="ts">
  import { call } from "../../lib/ipc/commands";
  import { invalidateEmbedCache } from "../editors/noteEngine";
  import LinkRepairModal from "../modals/LinkRepairModal.svelte";
  import {
    folderOf,
    formatError,
    type TreeActionModal,
    type TreeContextTarget,
  } from "../../lib/vault/tree";
  import type { NoteSummaryDto, VaultProfileDto } from "../../lib/ipc/types";

  interface Props {
    treeContext: TreeContextTarget | null;
    treeActionModal: TreeActionModal | null;
    notes: NoteSummaryDto[];
    folders: string[];
    activeId: string | null;
    activeFolder: string;
    onCloseContext: () => void;
    onSetActionModal: (modal: TreeActionModal | null) => void;
    onSetActiveFolder: (folder: string) => void;
    onClearActiveNote: () => void;
    onSaveBeforeAction: () => Promise<void>;
    onRefresh: () => Promise<void>;
    onOpenNote: (id: string) => Promise<void>;
    onError: (msg: string) => void;
  }

  let {
    treeContext,
    treeActionModal,
    notes,
    folders,
    activeId,
    activeFolder,
    onCloseContext,
    onSetActionModal,
    onSetActiveFolder,
    onClearActiveNote,
    onSaveBeforeAction,
    onRefresh,
    onOpenNote,
    onError,
  }: Props = $props();

  let modalInputValue = $state("");
  let modalSelectedFolder = $state("");
  let modalSelectedVaultPath = $state("");
  let availableVaults = $state<VaultProfileDto[]>([]);
  let treeContextOpenedAt = $state(0);

  $effect(() => {
    if (treeContext) {
      treeContextOpenedAt = Date.now();
    }
  });

  $effect(() => {
    const action = treeActionModal;
    if (!action) return;
    if (action.kind === "create-folder") {
      modalInputValue = "";
    } else if (action.kind === "rename-note") {
      modalInputValue = action.note.title;
    } else if (action.kind === "move-note") {
      modalSelectedFolder = folderOf(action.note.path);
    } else if (action.kind === "rename-folder") {
      modalInputValue = action.folderName;
    } else if (action.kind === "move-folder-vault") {
      void call("vault_list_profiles")
        .then((list) => {
          availableVaults = list.filter((v) => !v.active);
          modalSelectedVaultPath = availableVaults[0]?.path ?? "";
        })
        .catch((e) => {
          onError(formatError(e));
          onSetActionModal(null);
        });
    }
  });

  function closeTreeContextFromBackdrop(e: Event): void {
    e.preventDefault();
    e.stopPropagation();
    if (Date.now() - treeContextOpenedAt < 200) return;
    onCloseContext();
  }

  function triggerAction(action: TreeActionModal): void {
    onCloseContext();
    onError("");
    onSetActionModal(action);
  }

  async function submitTreeActionModal(keepContentOnFolderDelete?: boolean): Promise<void> {
    const modal = treeActionModal;
    if (!modal) return;
    try {
      await onSaveBeforeAction();
      if (modal.kind === "create-folder") {
        const folderName = modalInputValue.trim();
        if (!folderName) return;
        const fullPath = modal.parentFolder ? `${modal.parentFolder}/${folderName}` : folderName;
        await call("vault_create_folder", { name: fullPath });
        onSetActionModal(null);
        await onRefresh();
        onSetActiveFolder(fullPath.replace(/\\/g, "/").replace(/^\/+|\/+$/g, ""));
      } else if (modal.kind === "rename-note") {
        const nextTitle = modalInputValue.trim();
        if (!nextTitle || nextTitle === modal.note.title) {
          onSetActionModal(null);
          return;
        }
        const refs = await call("note_check_link_references", {
          oldTitle: modal.note.title,
        });
        if (refs.length > 0) {
          onSetActionModal({
            kind: "repair-links",
            note: modal.note,
            newTitle: nextTitle,
            references: refs,
          });
          return;
        }
        await call("note_rename", { noteId: modal.note.note_id, newTitle: nextTitle });
        onSetActionModal(null);
        await onRefresh();
        if (activeId === modal.note.note_id) {
          await onOpenNote(modal.note.note_id);
        }
      } else if (modal.kind === "move-note") {
        await call("note_move", { noteId: modal.note.note_id, folder: modalSelectedFolder });
        onSetActionModal(null);
        await onRefresh();
        if (activeId === modal.note.note_id) {
          await onOpenNote(modal.note.note_id);
        }
      } else if (modal.kind === "delete-note") {
        const targetId = modal.note.note_id;
        await call("note_delete", { noteId: targetId });
        onSetActionModal(null);
        if (activeId === targetId) {
          onClearActiveNote();
        }
        await onRefresh();
      } else if (modal.kind === "rename-folder") {
        const nextName = modalInputValue.trim();
        if (!nextName) return;
        const newPath = await call("vault_rename_folder", {
          folder: modal.folderPath,
          newName: nextName,
        });
        if (activeFolder === modal.folderPath) onSetActiveFolder(newPath);
        onSetActionModal(null);
        await onRefresh();
        if (activeId) await onOpenNote(activeId);
      } else if (modal.kind === "delete-folder") {
        const keep = Boolean(keepContentOnFolderDelete);
        await call("vault_delete_folder", {
          folder: modal.folderPath,
          keepContent: keep,
        });
        if (activeFolder === modal.folderPath || activeFolder.startsWith(`${modal.folderPath}/`)) {
          onSetActiveFolder("");
        }
        onSetActionModal(null);
        await onRefresh();
        if (activeId) {
          const stillExists = notes.some((n) => n.note_id === activeId);
          if (stillExists) {
            await onOpenNote(activeId);
          } else {
            onClearActiveNote();
          }
        }
      } else if (modal.kind === "move-folder-vault") {
        const dest = modalSelectedVaultPath.trim();
        if (!dest) return;
        await call("vault_move_folder_to_vault", {
          folder: modal.folderPath,
          targetVaultPath: dest,
        });
        if (activeFolder === modal.folderPath || activeFolder.startsWith(`${modal.folderPath}/`)) {
          onSetActiveFolder("");
        }
        onSetActionModal(null);
        await onRefresh();
        if (activeId && !notes.some((n) => n.note_id === activeId)) {
          onClearActiveNote();
        }
      }
    } catch (e) {
      onError(formatError(e));
    }
  }

  async function confirmRepairLinks(updateReferences: boolean): Promise<void> {
    const modal = treeActionModal;
    if (!modal || modal.kind !== "repair-links") return;
    try {
      if (updateReferences) {
        await call("note_repair_links", {
          oldTitle: modal.note.title,
          newTitle: modal.newTitle,
        });
      }
      await call("note_rename", { noteId: modal.note.note_id, newTitle: modal.newTitle });
      onSetActionModal(null);
      invalidateEmbedCache();
      await onRefresh();
      if (activeId === modal.note.note_id) {
        await onOpenNote(modal.note.note_id);
      }
    } catch (e) {
      onError(formatError(e));
    }
  }
</script>

{#if treeContext}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div
    class="context-menu-backdrop"
    onclick={closeTreeContextFromBackdrop}
    oncontextmenu={closeTreeContextFromBackdrop}
  >
    <div
      class="tree-context-menu"
      style="left: {treeContext.x}px; top: {treeContext.y}px"
      onclick={(e) => e.stopPropagation()}
      role="menu"
      tabindex="-1"
    >
      {#if treeContext.kind === "note"}
        {@const targetNote = treeContext.note}
        <div class="tree-context-header">📄 {targetNote.title}</div>
        <button
          type="button"
          class="more-item"
          onclick={() => triggerAction({ kind: "rename-note", note: targetNote })}
        >
          Renombrar nota
        </button>
        <button
          type="button"
          class="more-item"
          onclick={() => triggerAction({ kind: "move-note", note: targetNote })}
        >
          Mover de carpeta…
        </button>
        <div class="menu-sep"></div>
        <button
          type="button"
          class="more-item danger-text"
          onclick={() => triggerAction({ kind: "delete-note", note: targetNote })}
        >
          Eliminar nota
        </button>
      {:else}
        {@const targetFolder = { path: treeContext.folderPath, name: treeContext.folderName }}
        <div class="tree-context-header">📁 {targetFolder.name}</div>
        <button
          type="button"
          class="more-item"
          onclick={() => triggerAction({ kind: "create-folder", parentFolder: targetFolder.path })}
        >
          Nueva subcarpeta…
        </button>
        <button
          type="button"
          class="more-item"
          onclick={() =>
            triggerAction({
              kind: "rename-folder",
              folderPath: targetFolder.path,
              folderName: targetFolder.name,
            })}
        >
          Renombrar carpeta
        </button>
        <button
          type="button"
          class="more-item"
          onclick={() =>
            triggerAction({
              kind: "move-folder-vault",
              folderPath: targetFolder.path,
              folderName: targetFolder.name,
            })}
        >
          Mover a otro Vault…
        </button>
        <div class="menu-sep"></div>
        <button
          type="button"
          class="more-item danger-text"
          onclick={() =>
            triggerAction({
              kind: "delete-folder",
              folderPath: targetFolder.path,
              folderName: targetFolder.name,
            })}
        >
          Eliminar carpeta…
        </button>
      {/if}
    </div>
  </div>
{/if}

{#if treeActionModal}
  {#if treeActionModal.kind === "repair-links"}
    <LinkRepairModal
      note={treeActionModal.note}
      newTitle={treeActionModal.newTitle}
      references={treeActionModal.references}
      onUpdateLinks={() => void confirmRepairLinks(true)}
      onKeepAlias={() => void confirmRepairLinks(false)}
      onCancel={() => onSetActionModal(null)}
    />
  {:else}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="modal-overlay" onclick={() => onSetActionModal(null)}>
      <div
        class="modal tree-action-modal"
        onclick={(e) => e.stopPropagation()}
        role="dialog"
        tabindex="-1"
      >
        <div class="modal-header">
          <h2>
            {#if treeActionModal.kind === "create-folder"}
              {treeActionModal.parentFolder
                ? `Nueva subcarpeta en «${treeActionModal.parentFolder}»`
                : "Nueva carpeta"}
            {:else if treeActionModal.kind === "rename-note"}
              Renombrar nota
            {:else if treeActionModal.kind === "move-note"}
              Mover nota de carpeta
            {:else if treeActionModal.kind === "delete-note"}
              Eliminar nota
            {:else if treeActionModal.kind === "rename-folder"}
              Renombrar carpeta
            {:else if treeActionModal.kind === "delete-folder"}
              Eliminar carpeta «{treeActionModal.folderName}»
            {:else}
              Mover carpeta a otro Vault
            {/if}
          </h2>
          <button class="icon-btn" onclick={() => onSetActionModal(null)} aria-label="Cerrar"
            >✕</button
          >
        </div>

        <div class="modal-body tree-modal-body">
          {#if treeActionModal.kind === "create-folder"}
            <label class="setting-label" for="create-folder-input">
              <strong>Nombre de la {treeActionModal.parentFolder ? "subcarpeta" : "carpeta"}</strong
              >
            </label>
            <input
              id="create-folder-input"
              class="tree-modal-input"
              type="text"
              placeholder="Ej. Proyectos"
              bind:value={modalInputValue}
              onkeydown={(e) => e.key === "Enter" && void submitTreeActionModal()}
            />
          {:else if treeActionModal.kind === "rename-note"}
            <label class="setting-label" for="rename-note-input">
              <strong>Nuevo título para «{treeActionModal.note.title}»</strong>
            </label>
            <input
              id="rename-note-input"
              class="tree-modal-input"
              type="text"
              bind:value={modalInputValue}
              onkeydown={(e) => e.key === "Enter" && void submitTreeActionModal()}
            />
          {:else if treeActionModal.kind === "move-note"}
            <label class="setting-label" for="move-note-select">
              <strong>Selecciona la carpeta de destino para «{treeActionModal.note.title}»</strong>
            </label>
            <select
              id="move-note-select"
              class="tree-modal-select"
              bind:value={modalSelectedFolder}
            >
              <option value="">/ (Raíz del Vault)</option>
              {#each folders as f (f)}
                <option value={f}>📁 {f}</option>
              {/each}
            </select>
          {:else if treeActionModal.kind === "delete-note"}
            <p class="tree-modal-text">
              ¿Enviar la nota <strong>«{treeActionModal.note.title}»</strong> a la papelera? Podrás restaurarla
              en su misma carpeta durante 30 días.
            </p>
          {:else if treeActionModal.kind === "rename-folder"}
            <label class="setting-label" for="rename-folder-input">
              <strong>Nuevo nombre para la carpeta «{treeActionModal.folderName}»</strong>
            </label>
            <input
              id="rename-folder-input"
              class="tree-modal-input"
              type="text"
              bind:value={modalInputValue}
              onkeydown={(e) => e.key === "Enter" && void submitTreeActionModal()}
            />
          {:else if treeActionModal.kind === "delete-folder"}
            <p class="tree-modal-text">
              ¿Qué deseas hacer con las notas y subcarpetas dentro de
              <strong>«{treeActionModal.folderName}»</strong>?
            </p>
            <div class="folder-delete-choices">
              <button
                type="button"
                class="trash-btn restore"
                onclick={() => void submitTreeActionModal(true)}
              >
                Conservar contenido (mover notas fuera de la carpeta)
              </button>
              <button
                type="button"
                class="danger-btn"
                onclick={() => void submitTreeActionModal(false)}
              >
                Eliminar carpeta y enviar contenido a la papelera
              </button>
            </div>
          {:else if treeActionModal.kind === "move-folder-vault"}
            {#if availableVaults.length === 0}
              <p class="tree-modal-text">
                No tienes otros Vaults creados actualmente. Crea primero otro Vault desde «Gestionar
                Vaults» para poder mover carpetas hacia él.
              </p>
            {:else}
              <label class="setting-label" for="move-folder-vault-select">
                <strong
                  >Selecciona el Vault al que deseas mover la carpeta «{treeActionModal.folderName}»
                  y todas sus notas:</strong
                >
              </label>
              <select
                id="move-folder-vault-select"
                class="tree-modal-select"
                bind:value={modalSelectedVaultPath}
              >
                {#each availableVaults as v (v.path)}
                  <option value={v.path}>🗄️ {v.name} ({v.path})</option>
                {/each}
              </select>
            {/if}
          {/if}
        </div>

        <div class="modal-footer">
          <button type="button" class="trash-btn" onclick={() => onSetActionModal(null)}>
            Cancelar
          </button>
          {#if treeActionModal.kind === "delete-note"}
            <button type="button" class="danger-btn" onclick={() => void submitTreeActionModal()}>
              Eliminar
            </button>
          {:else if treeActionModal.kind === "delete-folder"}
            <!-- Botones de elección directa arriba -->
          {:else if treeActionModal.kind === "move-folder-vault"}
            {#if availableVaults.length > 0}
              <button type="button" class="primary" onclick={() => void submitTreeActionModal()}>
                Mover al Vault
              </button>
            {/if}
          {:else if treeActionModal.kind === "move-note"}
            <button type="button" class="primary" onclick={() => void submitTreeActionModal()}>
              Mover nota
            </button>
          {:else if treeActionModal.kind === "create-folder"}
            <button type="button" class="primary" onclick={() => void submitTreeActionModal()}>
              Crear carpeta
            </button>
          {:else}
            <button type="button" class="primary" onclick={() => void submitTreeActionModal()}>
              Guardar
            </button>
          {/if}
        </div>
      </div>
    </div>
  {/if}
{/if}
