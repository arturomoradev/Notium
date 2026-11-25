<script lang="ts">
  import { call } from "../../lib/ipc/commands";
  import Icon from "../editors/Icon.svelte";
  import {
    buildTree,
    collectFolderPaths,
    folderOf,
    formatError,
    type DrawerView,
    type FolderNode,
    type TreeContextTarget,
  } from "../../lib/vault/tree";
  import type {
    NoteSummaryDto,
    SearchHitDto,
    SyncStatusDto,
    VaultInfoDto,
  } from "../../lib/ipc/types";

  type SyncIconName = "cloud" | "sync" | "alert" | "cloud-off";

  interface Props {
    isDesktop: boolean;
    sidebarOpen: boolean;
    drawerView: DrawerView;
    vault: VaultInfoDto | null;
    notes: NoteSummaryDto[];
    folders: string[];
    activeId: string | null;
    activePath: string;
    activeFolder: string;
    syncStatus: SyncStatusDto;
    syncIconName: SyncIconName;
    syncStatusLabel: string;
    syncTooltip: string;
    touchDraggingActive: boolean;
    onSetSidebarOpen: (open: boolean) => void;
    onSetDrawerView: (view: DrawerView) => void;
    onSetActiveFolder: (folder: string) => void;
    onSetTouchDragging: (active: boolean) => void;
    onShowTreeContext: (ctx: TreeContextTarget) => void;
    onOpenNote: (id: string) => Promise<void>;
    onCreateNote: () => Promise<void>;
    onCreateFolder: () => Promise<void>;
    onSaveBeforeAction: () => Promise<void>;
    onRefresh: () => Promise<void>;
    onOpenSyncSettings: () => void;
    onOpenSettings: () => void;
    onOpenVaultModal: () => void;
    onError: (msg: string) => void;
  }

  let {
    isDesktop,
    sidebarOpen,
    drawerView,
    vault,
    notes,
    folders,
    activeId,
    activePath,
    activeFolder,
    syncStatus,
    syncIconName,
    syncStatusLabel,
    syncTooltip,
    touchDraggingActive,
    onSetSidebarOpen,
    onSetDrawerView,
    onSetActiveFolder,
    onSetTouchDragging,
    onShowTreeContext,
    onOpenNote,
    onCreateNote,
    onCreateFolder,
    onSaveBeforeAction,
    onRefresh,
    onOpenSyncSettings,
    onOpenSettings,
    onOpenVaultModal,
    onError,
  }: Props = $props();

  let searchQuery = $state("");
  let searchHits = $state<SearchHitDto[]>([]);
  let tagFilter = $state("");
  let expandedTag = $state<string | null>(null);
  let sortOrder = $state<"az" | "modified">("modified");
  let collapsedFolders = $state<string[]>([]);
  let mobileViewMenuOpen = $state(false);

  const mobileViewMeta = $derived(
    drawerView === "files"
      ? { icon: "folder", label: "Files" }
      : drawerView === "search"
        ? { icon: "search", label: "Search" }
        : { icon: "hash", label: "Tags" },
  );

  let draggingNote = $state<NoteSummaryDto | null>(null);
  let dragOverFolder = $state<string | null>(null);
  let touchDragGhostX = $state(0);
  let touchDragGhostY = $state(0);
  let longPressTimer: number | undefined;
  let suppressNextTreeClick = false;
  let pointerDownX = 0;
  let pointerDownY = 0;
  let pointerPendingNote: NoteSummaryDto | null = null;
  let pointerPendingFolder: { path: string; name: string } | null = null;
  let longPressedNoteId = $state<string | null>(null);
  let longPressArmed = false;

  const LONG_PRESS_MS = 380;
  const TOUCH_DRAG_THRESHOLD = 8;
  const MOUSE_DRAG_THRESHOLD = 6;

  const folderTree = $derived(buildTree(notes, folders, sortOrder));

  const allTags = $derived(
    [...new Set(notes.flatMap((n) => n.tags))].sort((a, b) => a.localeCompare(b, "es")),
  );

  const filteredTags = $derived.by(() => {
    const q = tagFilter.trim().replace(/^#/, "").toLowerCase();
    if (!q) return allTags;
    return allTags.filter((t) => t.toLowerCase().includes(q));
  });

  function notesForTag(tag: string): NoteSummaryDto[] {
    return notes.filter((n) => n.tags.includes(tag));
  }

  function clearLongPressTimer(): void {
    if (longPressTimer !== undefined) {
      window.clearTimeout(longPressTimer);
      longPressTimer = undefined;
    }
  }

  function clampMenuCoords(x: number, y: number): { x: number; y: number } {
    return {
      x: Math.max(10, Math.min(x, window.innerWidth - 220)),
      y: Math.max(10, Math.min(y, window.innerHeight - 190)),
    };
  }

  function openNoteContextMenu(e: MouseEvent, note: NoteSummaryDto): void {
    e.preventDefault();
    e.stopPropagation();
    if (touchDraggingActive) return;
    const { x, y } = clampMenuCoords(e.clientX, e.clientY);
    onShowTreeContext({ kind: "note", note, x, y });
  }

  function openFolderContextMenu(e: MouseEvent, folderPath: string, folderName: string): void {
    e.preventDefault();
    e.stopPropagation();
    if (touchDraggingActive) return;
    const { x, y } = clampMenuCoords(e.clientX, e.clientY);
    onShowTreeContext({ kind: "folder", folderPath, folderName, x, y });
  }

  function updateDragHover(x: number, y: number): void {
    touchDragGhostX = x;
    touchDragGhostY = y;
    const hit = document.elementFromPoint(x, y);
    const dropEl = hit?.closest("[data-drop-folder]") as HTMLElement | null;
    dragOverFolder = dropEl ? (dropEl.getAttribute("data-drop-folder") ?? "") : null;
  }

  function beginDrag(note: NoteSummaryDto, x: number, y: number): void {
    clearLongPressTimer();
    longPressedNoteId = null;
    draggingNote = note;
    onSetTouchDragging(true);
    suppressNextTreeClick = true;
    updateDragHover(x, y);
  }

  async function moveNoteToFolder(noteId: string, targetFolder: string): Promise<void> {
    try {
      await onSaveBeforeAction();
      await call("note_move", { noteId, folder: targetFolder });
      await onRefresh();
      if (activeId === noteId) {
        await onOpenNote(noteId);
      }
    } catch (e) {
      onError(formatError(e));
    }
  }

  function finishDrag(): void {
    const note = draggingNote;
    const targetFolder = dragOverFolder;
    onSetTouchDragging(false);
    draggingNote = null;
    dragOverFolder = null;
    pointerPendingNote = null;
    pointerPendingFolder = null;
    longPressedNoteId = null;
    if (note && targetFolder !== null && folderOf(note.path) !== targetFolder) {
      void moveNoteToFolder(note.note_id, targetFolder);
    }
    window.setTimeout(() => (suppressNextTreeClick = false), 150);
  }

  function cancelDrag(): void {
    onSetTouchDragging(false);
    draggingNote = null;
    dragOverFolder = null;
    pointerPendingNote = null;
    pointerPendingFolder = null;
    longPressedNoteId = null;
  }

  function onNoteMouseDown(e: MouseEvent, note: NoteSummaryDto): void {
    if (e.button !== 0) return;
    pointerDownX = e.clientX;
    pointerDownY = e.clientY;
    pointerPendingNote = note;
    window.addEventListener("mousemove", onWindowMouseMove);
    window.addEventListener("mouseup", onWindowMouseUp);
  }

  function onWindowMouseMove(e: MouseEvent): void {
    if (!touchDraggingActive) {
      if (!pointerPendingNote) return;
      if (Math.hypot(e.clientX - pointerDownX, e.clientY - pointerDownY) < MOUSE_DRAG_THRESHOLD)
        return;
      beginDrag(pointerPendingNote, e.clientX, e.clientY);
    }
    e.preventDefault();
    updateDragHover(e.clientX, e.clientY);
  }

  function onWindowMouseUp(): void {
    window.removeEventListener("mousemove", onWindowMouseMove);
    window.removeEventListener("mouseup", onWindowMouseUp);
    if (touchDraggingActive) {
      finishDrag();
    } else {
      pointerPendingNote = null;
    }
  }

  function treeTouchGestures(node: HTMLElement) {
    function onStart(e: TouchEvent): void {
      clearLongPressTimer();
      longPressArmed = false;
      longPressedNoteId = null;
      pointerPendingNote = null;
      pointerPendingFolder = null;
      if (e.touches.length !== 1) return;
      const target = e.target as Element | null;
      const noteEl = target?.closest("[data-note-id]") as HTMLElement | null;
      const folderEl = noteEl
        ? null
        : (target?.closest("[data-folder-path]") as HTMLElement | null);
      if (!noteEl && !folderEl) return;
      const t = e.touches[0];
      pointerDownX = t.clientX;
      pointerDownY = t.clientY;
      const note = noteEl ? (notes.find((n) => n.note_id === noteEl.dataset.noteId) ?? null) : null;
      pointerPendingNote = note;
      const folderPath = folderEl?.dataset.folderPath ?? "";
      const folderName = folderEl?.dataset.folderName ?? "";
      if (folderEl) {
        pointerPendingFolder = { path: folderPath, name: folderName };
      }

      longPressTimer = window.setTimeout(() => {
        longPressTimer = undefined;
        longPressArmed = true;
        suppressNextTreeClick = true;
        if (note) longPressedNoteId = note.note_id;
        navigator.vibrate?.(25);
      }, LONG_PRESS_MS);
    }

    function onMove(e: TouchEvent): void {
      if (e.touches.length !== 1) return;
      const t = e.touches[0];
      const dist = Math.hypot(t.clientX - pointerDownX, t.clientY - pointerDownY);

      if (touchDraggingActive) {
        if (e.cancelable) e.preventDefault();
        updateDragHover(t.clientX, t.clientY);
        return;
      }

      if (!longPressArmed) {
        if (dist > TOUCH_DRAG_THRESHOLD) {
          clearLongPressTimer();
          longPressedNoteId = null;
          pointerPendingNote = null;
          pointerPendingFolder = null;
        }
        return;
      }

      if (pointerPendingNote && dist > TOUCH_DRAG_THRESHOLD) {
        if (e.cancelable) e.preventDefault();
        longPressedNoteId = null;
        beginDrag(pointerPendingNote, t.clientX, t.clientY);
      } else if (pointerPendingFolder && dist > TOUCH_DRAG_THRESHOLD) {
        longPressArmed = false;
        pointerPendingFolder = null;
      }
    }

    function onEnd(e: TouchEvent): void {
      clearLongPressTimer();
      const wasArmed = longPressArmed;
      const pendingNote = pointerPendingNote;
      const pendingFolder = pointerPendingFolder;
      longPressArmed = false;
      longPressedNoteId = null;
      pointerPendingNote = null;
      pointerPendingFolder = null;

      if (touchDraggingActive) {
        if (e.cancelable) e.preventDefault();
        e.stopPropagation();
        finishDrag();
      } else if (wasArmed) {
        if (e.cancelable) e.preventDefault();
        e.stopPropagation();
        suppressNextTreeClick = true;
        window.setTimeout(() => (suppressNextTreeClick = false), 350);
        const { x, y } = clampMenuCoords(pointerDownX, pointerDownY);
        if (pendingNote) {
          onShowTreeContext({ kind: "note", note: pendingNote, x, y });
        } else if (pendingFolder) {
          onShowTreeContext({
            kind: "folder",
            folderPath: pendingFolder.path,
            folderName: pendingFolder.name,
            x,
            y,
          });
        }
      }
    }

    function onCancel(): void {
      clearLongPressTimer();
      longPressArmed = false;
      longPressedNoteId = null;
      if (touchDraggingActive) cancelDrag();
      pointerPendingNote = null;
      pointerPendingFolder = null;
    }

    node.addEventListener("touchstart", onStart, { passive: true });
    node.addEventListener("touchmove", onMove, { passive: false });
    node.addEventListener("touchend", onEnd, { passive: false });
    node.addEventListener("touchcancel", onCancel);
    return {
      destroy() {
        node.removeEventListener("touchstart", onStart);
        node.removeEventListener("touchmove", onMove);
        node.removeEventListener("touchend", onEnd);
        node.removeEventListener("touchcancel", onCancel);
        clearLongPressTimer();
      },
    };
  }

  async function doSearch(q: string): Promise<void> {
    searchQuery = q;
    const trimmed = q.trim();
    if (!trimmed) {
      searchHits = [];
      return;
    }
    const isTagOnly = /^#[\p{L}\p{N}_/-]+$/u.test(trimmed);
    try {
      searchHits = await call("note_search_fts", {
        query: isTagOnly ? "" : trimmed,
        tag: isTagOnly ? trimmed.slice(1) : null,
        format: null,
      });
    } catch (e) {
      onError(formatError(e));
    }
  }

  async function searchByTag(tag: string): Promise<void> {
    onSetDrawerView("search");
    await doSearch(`#${tag}`);
  }

  function toggleFolder(path: string): void {
    collapsedFolders = collapsedFolders.includes(path)
      ? collapsedFolders.filter((p) => p !== path)
      : [...collapsedFolders, path];
  }

  function expandAll(): void {
    collapsedFolders = [];
  }

  function collapseAll(): void {
    collapsedFolders = collectFolderPaths(folderTree);
  }

  function revealActive(): void {
    onSetDrawerView("files");
    if (!activePath) return;
    const folder = folderOf(activePath);
    if (folder) {
      const segs = folder.split("/").filter(Boolean);
      // Local scratch set, never read reactively.
      // eslint-disable-next-line svelte/prefer-svelte-reactivity
      const ancestors = new Set<string>();
      let cur = "";
      for (const s of segs) {
        cur = cur ? `${cur}/${s}` : s;
        ancestors.add(cur);
      }
      collapsedFolders = collapsedFolders.filter((p) => !ancestors.has(p));
    }
    requestAnimationFrame(() => {
      document.querySelector(".tree-note.active")?.scrollIntoView({ block: "nearest" });
    });
  }

  function toggleSort(): void {
    sortOrder = sortOrder === "az" ? "modified" : "az";
  }

  function selectMobileView(view: DrawerView): void {
    mobileViewMenuOpen = false;
    onSetDrawerView(view);
  }
</script>

<!-- Tapping anywhere outside the view switcher closes its menu. -->
<svelte:window
  onpointerdown={(e) => {
    if (mobileViewMenuOpen && !(e.target as Element).closest?.(".mobile-view-switcher"))
      mobileViewMenuOpen = false;
  }}
/>

<aside class="sidebar" class:open={sidebarOpen}>
  {#if isDesktop}
    <div class="desktop-sidebar-top">
      <div class="desktop-tab-strip">
        <button
          class="desktop-tab-btn"
          class:active={drawerView === "files"}
          title="Archivos"
          onclick={() => onSetDrawerView("files")}
        >
          <Icon name="folder" size={15} />
          <span>Archivos</span>
        </button>
        <button
          class="desktop-tab-btn"
          class:active={drawerView === "search"}
          title="Buscar"
          onclick={() => onSetDrawerView("search")}
        >
          <Icon name="search" size={15} />
          <span>Buscar</span>
        </button>
        <button
          class="desktop-tab-btn"
          class:active={drawerView === "tags"}
          title="Etiquetas (Tags)"
          onclick={() => onSetDrawerView("tags")}
        >
          <Icon name="hash" size={15} />
          <span
            >Tags {#if allTags.length > 0}({allTags.length}){/if}</span
          >
        </button>
      </div>
    </div>

    {#if drawerView === "files"}
      <div class="desktop-file-toolbar">
        <button class="desktop-tool-btn" title="Nueva nota" onclick={() => void onCreateNote()}>
          <Icon name="note-new" size={16} />
        </button>
        <button
          class="desktop-tool-btn"
          title="Nueva carpeta"
          onclick={() => void onCreateFolder()}
        >
          <Icon name="folder-plus" size={16} />
        </button>
        <button class="desktop-tool-btn" title="Cambiar orden" onclick={toggleSort}>
          <Icon name="sort-lines" size={16} />
        </button>
        <button class="desktop-tool-btn" title="Revelar nota activa" onclick={revealActive}>
          <Icon name="locate-file" size={16} />
        </button>
        <button
          class="desktop-tool-btn"
          title={collapsedFolders.length ? "Expandir todo" : "Contraer todo"}
          onclick={collapsedFolders.length ? expandAll : collapseAll}
        >
          <Icon name={collapsedFolders.length ? "chevrons" : "chevrons-in"} size={16} />
        </button>
      </div>
    {/if}
  {:else}
    <div class="drawer-head">
      <button
        class="floating-btn small"
        onclick={() => onSetSidebarOpen(false)}
        aria-label="Cerrar"
      >
        <Icon name="sidebar" size={18} />
      </button>
      <span class="drawer-title">Notium</span>
    </div>
  {/if}

  {#if drawerView === "files"}
    <div
      class="tree"
      class:drop-root-active={dragOverFolder === ""}
      class:is-dragging={touchDraggingActive}
      data-drop-folder=""
      onselectstart={(e) => e.preventDefault()}
      use:treeTouchGestures
    >
      {#if touchDraggingActive}
        <div
          class="tree-root-dropzone"
          class:drop-target={dragOverFolder === ""}
          data-drop-folder=""
        >
          Soltar aquí para mover a la raíz del Vault
        </div>
      {/if}
      {#snippet renderNode(node: FolderNode, depth: number)}
        {#each node.folders as folder (folder.path)}
          <div
            class="tree-folder"
            class:drop-target={dragOverFolder === folder.path}
            data-drop-folder={folder.path}
            data-folder-path={folder.path}
            data-folder-name={folder.name}
            style="padding-left: {(isDesktop ? 10 : 8) + depth * 14}px"
            role="treeitem"
            aria-selected={activeFolder === folder.path}
            tabindex="-1"
            onselectstart={(e) => e.preventDefault()}
            oncontextmenu={(e) => openFolderContextMenu(e, folder.path, folder.name)}
          >
            <button
              class="tree-chevron"
              onmousedown={(e) => e.preventDefault()}
              onclick={() => toggleFolder(folder.path)}
              aria-label="Alternar carpeta"
            >
              <Icon
                name={collapsedFolders.includes(folder.path) ? "chevron-right" : "chevron-down"}
                size={14}
              />
            </button>
            <button
              class="tree-folder-name"
              data-drop-folder={folder.path}
              onclick={() => {
                if (suppressNextTreeClick) return;
                onSetActiveFolder(folder.path);
                if (isDesktop) toggleFolder(folder.path);
              }}
              class:selected={activeFolder === folder.path}
            >
              <span class="tree-icon">
                <Icon
                  name={collapsedFolders.includes(folder.path) ? "folder" : "folder-open"}
                  size={15}
                />
              </span>
              <span class="tree-title">{folder.name}</span>
            </button>
          </div>
          {#if !collapsedFolders.includes(folder.path)}
            {@render renderNode(folder, depth + 1)}
          {/if}
        {/each}
        {#each node.notes as note (note.note_id)}
          <button
            class="tree-note"
            class:active={note.note_id === activeId}
            class:dragging={draggingNote?.note_id === note.note_id}
            class:long-pressed={longPressedNoteId === note.note_id}
            data-drop-folder={folderOf(note.path)}
            data-note-id={note.note_id}
            style="padding-left: {(isDesktop ? 28 : 24) + depth * 14}px"
            onselectstart={(e) => e.preventDefault()}
            onmousedown={(e) => onNoteMouseDown(e, note)}
            oncontextmenu={(e) => openNoteContextMenu(e, note)}
            onclick={() => {
              if (suppressNextTreeClick) return;
              void onOpenNote(note.note_id);
            }}
          >
            <span class="tree-icon"><Icon name="file" size={15} /></span>
            <span class="tree-title">{note.title}</span>
          </button>
        {/each}
      {/snippet}
      {@render renderNode(folderTree, 0)}
      {#if notes.length === 0 && folders.length === 0}
        <div class="list-caption">Sin notas. Crea la primera.</div>
      {/if}
    </div>
  {:else if drawerView === "search"}
    <div class="drawer-search">
      <input
        placeholder="Buscar notas…"
        bind:value={searchQuery}
        oninput={(e) => void doSearch(e.currentTarget.value)}
      />
      <div class="tree search-results">
        {#each searchHits as hit (hit.note_id)}
          <button class="tree-note" onclick={() => void onOpenNote(hit.note_id)}>
            <span class="tree-title">{hit.title}</span>
          </button>
        {/each}
        {#if searchQuery.trim() && searchHits.length === 0}
          <div class="list-caption">Sin resultados</div>
        {/if}
      </div>
    </div>
  {:else}
    <div class="drawer-search">
      <input placeholder="Filtrar etiquetas (#tag)…" bind:value={tagFilter} />
      <div class="tree search-results">
        {#each filteredTags as tag (tag)}
          {@const taggedNotes = notesForTag(tag)}
          {@const isExpanded = expandedTag === tag}
          <div class="tag-group">
            <div class="tag-row">
              <button
                type="button"
                class="tree-note tag-row-btn"
                onclick={() => (expandedTag = isExpanded ? null : tag)}
              >
                <Icon name={isExpanded ? "chevron-down" : "chevron-right"} size={13} />
                <span class="tree-tag">#</span>
                <span class="tree-title">{tag}</span>
                <span class="badge md">{taggedNotes.length}</span>
              </button>
              <button
                type="button"
                class="desktop-tool-btn"
                title="Buscar notas con #{tag}"
                onclick={() => void searchByTag(tag)}
              >
                <Icon name="search" size={13} />
              </button>
            </div>
            {#if isExpanded}
              <div class="tag-notes-list">
                {#each taggedNotes as n (n.note_id)}
                  <button
                    type="button"
                    class="tree-note"
                    class:active={n.note_id === activeId}
                    style="padding-left: 26px"
                    onclick={() => void onOpenNote(n.note_id)}
                  >
                    <span class="tree-title">{n.title}</span>
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        {/each}
        {#if filteredTags.length === 0}
          <div class="list-caption">
            {allTags.length === 0
              ? "Sin etiquetas. Escribe #etiqueta en cualquier nota."
              : "Sin etiquetas coincidentes."}
          </div>
        {/if}
      </div>
    </div>
  {/if}

  {#if !isDesktop}
    <div class="drawer-actions">
      <div class="drawer-actions-row">
        <button
          class="drawer-action"
          title="Nueva nota"
          aria-label="Nueva nota"
          onclick={() => void onCreateNote()}
        >
          <Icon name="note-new" size={19} />
        </button>
        <button
          class="drawer-action"
          title="Nueva carpeta"
          aria-label="Nueva carpeta"
          onclick={() => void onCreateFolder()}
        >
          <Icon name="folder-plus" size={19} />
        </button>
        <button class="drawer-action" title="Orden" aria-label="Cambiar orden" onclick={toggleSort}>
          <Icon name="sort-lines" size={19} />
        </button>
        <button
          class="drawer-action"
          title="Revelar nota activa"
          aria-label="Revelar nota activa"
          onclick={revealActive}
        >
          <Icon name="locate-file" size={19} />
        </button>
        <button
          class="drawer-action"
          title={collapsedFolders.length ? "Expandir todo" : "Contraer todo"}
          aria-label={collapsedFolders.length ? "Expandir todo" : "Contraer todo"}
          onclick={collapsedFolders.length ? expandAll : collapseAll}
        >
          <Icon name={collapsedFolders.length ? "chevrons" : "chevrons-in"} size={19} />
        </button>
      </div>

      <div class="mobile-view-switcher">
        <button
          type="button"
          class="mobile-view-selector"
          aria-label="Cambiar vista de navegación"
          aria-expanded={mobileViewMenuOpen}
          onclick={() => (mobileViewMenuOpen = !mobileViewMenuOpen)}
        >
          <Icon name={mobileViewMeta.icon} size={20} />
          <span>{mobileViewMeta.label}</span>
          <Icon name="chevrons" size={16} />
        </button>
        {#if mobileViewMenuOpen}
          <div class="mobile-view-menu">
            <button class:active={drawerView === "files"} onclick={() => selectMobileView("files")}>
              <Icon name="folder" size={17} /> Files
            </button>
            <button
              class:active={drawerView === "search"}
              onclick={() => selectMobileView("search")}
            >
              <Icon name="search" size={17} /> Search
            </button>
            <button class:active={drawerView === "tags"} onclick={() => selectMobileView("tags")}>
              <Icon name="hash" size={17} /> Tags
            </button>
          </div>
        {/if}
      </div>

      <div class="drawer-footer">
        <div class="vault-summary">
          <button
            type="button"
            class="vault-name-btn"
            onclick={() => (onSetSidebarOpen(false), onOpenVaultModal())}
          >
            {vault?.name || "Notium"}
          </button>
          <div class="vault-count">{notes.length} archivos · {folders.length} carpetas</div>
        </div>
        <div class="drawer-footer-btns">
          <button
            class="drawer-settings"
            title="Ajustes"
            aria-label="Ajustes"
            onclick={() => (onSetSidebarOpen(false), onOpenSettings())}
          >
            <Icon name="settings" size={19} />
          </button>
        </div>
      </div>
    </div>
  {:else}
    <div class="desktop-sidebar-footer">
      <button
        type="button"
        class="vault-name-btn"
        title="Cambiar o gestionar Vaults"
        onclick={onOpenVaultModal}
      >
        {vault?.name || "Notium"}
      </button>
      <button
        type="button"
        class="vault-sync-btn"
        class:syncing={syncStatus.state === "syncing"}
        class:synced={syncStatus.state === "synced"}
        class:error={syncStatus.state === "error"}
        title={syncTooltip}
        onclick={onOpenSyncSettings}
      >
        <Icon name={syncIconName} size={14} />
        <span>{syncStatusLabel}</span>
      </button>
    </div>
  {/if}
</aside>

{#if touchDraggingActive && draggingNote}
  <div
    class="touch-drag-ghost"
    style="left: {touchDragGhostX + 12}px; top: {touchDragGhostY - 18}px"
  >
    📄 {draggingNote.title}
    {#if dragOverFolder !== null}
      <span class="touch-drag-dest">→ {dragOverFolder || "Raíz"}</span>
    {/if}
  </div>
{/if}
