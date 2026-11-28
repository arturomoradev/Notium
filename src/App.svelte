<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen, TauriEvent, type UnlistenFn } from "@tauri-apps/api/event";
  import { call, importAttachment as importAttachmentFile } from "./lib/ipc/commands";
  import MarkdownEditor from "./components/editors/MarkdownEditor.svelte";
  import { invalidateEmbedCache } from "./components/editors/markdownEngine";
  import Toolbar from "./components/editors/Toolbar.svelte";
  import ToolbarConfig from "./components/editors/ToolbarConfig.svelte";
  import Icon from "./components/editors/Icon.svelte";
  import CommandPalette from "./components/search/CommandPalette.svelte";
  import SettingsModal from "./components/SettingsModal.svelte";
  import TrashModal from "./components/TrashModal.svelte";
  import VaultModal from "./components/VaultModal.svelte";
  import AttachmentsModal from "./components/AttachmentsModal.svelte";
  import SidebarDrawer from "./components/navigation/SidebarDrawer.svelte";
  import TreeActionModal from "./components/navigation/TreeActionModal.svelte";
  import RightInspectorPanel from "./components/panels/RightInspectorPanel.svelte";
  import type { EditorHandle } from "./components/editors/engine";
  import { countWords, statsOf, type Stats } from "./lib/plainText";
  import { updateViewport } from "./lib/mobile/viewport";
  import { DEMO_GUIDE_TITLE, ensureDemoNotes } from "./lib/demoNotes";
  import { settingsStore } from "./lib/stores/settings.svelte";
  import { compressImage, compressPendingAttachments } from "./lib/compressImage";
  import { editorStore, type EditorKind, type ViewMode } from "./lib/stores/editor.svelte";
  import { hideSplash } from "./lib/splash";
  import {
    folderOf,
    formatError,
    getSyncUiMeta,
    isHorizontallyScrollable,
    type ConflictItem,
    type DrawerView,
    type PanelTab,
    type TreeActionModal as TreeActionModalType,
    type TreeContextTarget,
  } from "./lib/vault/tree";
  import type {
    VaultInfoDto,
    NoteSummaryDto,
    BacklinkDto,
    OutlinkDto,
    SettingsDto,
    SyncStatusDto,
  } from "./lib/ipc/types";

  import { syncSystemBars } from "./lib/androidBars";
  import { pushBackHandler } from "./lib/backStack";
  import { layout } from "./lib/layout.svelte";

  // Docked desktop layout vs. the compact phone-style UI (see lib/layout).
  const isDesktop = $derived(!layout.compact);

  let vault = $state<VaultInfoDto | null>(null);
  let notes = $state<NoteSummaryDto[]>([]);
  let folders = $state<string[]>([]);
  const settings = $derived(settingsStore.settings);

  $effect(() => {
    settingsStore.applyTheme();
  });

  function toggleTheme(): void {
    void settingsStore.toggleTheme().catch((e) => (error = formatError(e)));
  }

  let activeId = $state<string | null>(null);
  let activeFormat = $state<EditorKind>("markdown");
  let activeNoteContent = $state("");
  let activePath = $state("");
  let rev = $state(0);
  let dirty = $state(false);
  let saving = $state(false);
  let status = $state("Cargando…");
  let error = $state("");
  let backlinks = $state<BacklinkDto[]>([]);
  let unlinked = $state<BacklinkDto[]>([]);
  let outlinks = $state<OutlinkDto[]>([]);
  let paletteOpen = $state(false);
  let sidebarOpen = $state(!layout.compact);
  let panelOpen = $state(false);
  let panelTab = $state<PanelTab>("backlinks");
  let conflicts = $state<ConflictItem[]>([]);
  let externalChanged = $state(false);
  let loadNonce = $state(0);
  const viewMode = $derived(editorStore.viewMode);
  let stats = $state<Stats>({ words: 0, chars: 0 });
  let settingsOpen = $state(false);
  let trashOpen = $state(false);
  let vaultModalOpen = $state(false);
  let attachmentsModalOpen = $state(false);
  let firstRunVaultSetup = $state(false);
  let moreOpen = $state(false);
  let attachmentMenu = $state(false);
  let galleryInput = $state<HTMLInputElement>();
  let cameraInput = $state<HTMLInputElement>();
  let drawerView = $state<DrawerView>("files");
  let activeFolder = $state("");

  let treeContext = $state<TreeContextTarget | null>(null);
  let treeActionModal = $state<TreeActionModalType | null>(null);
  let touchDraggingActive = $state(false);

  let syncStatus = $state<SyncStatusDto>({
    enabled: false,
    server_url: "",
    device_id: "",
    vault_id: "default",
    state: "offline",
    last_synced_at: null,
    pending_ops: 0,
    last_error: null,
  });
  let settingsInitialTab = $state<"general" | "sync" | "snippets">("general");

  // Crossing the breakpoint (window resize) swaps docked sidebar and overlay
  // drawers: start the new layout from its resting state.
  let wasCompact = layout.compact;
  $effect(() => {
    const compact = layout.compact;
    if (compact === wasCompact) return;
    wasCompact = compact;
    sidebarOpen = !compact;
    panelOpen = false;
  });

  // Android draws the caret/selection handles above all web content, so a focused
  // editor would poke its handle through an overlay drawer: drop focus when one opens.
  $effect(() => {
    if (!layout.compact || !(sidebarOpen || panelOpen || dragProgress !== null)) return;
    const focused = document.activeElement;
    if (focused instanceof HTMLElement && focused.closest(".cm-editor")) focused.blur();
  });

  // Android: tint the area behind the system bars like the surface under them.
  $effect(() => {
    void settingsStore.resolvedTheme;
    syncSystemBars(!isDesktop && (sidebarOpen || panelOpen) ? "panel" : "editor");
  });

  function openSyncSettings(): void {
    settingsInitialTab = "sync";
    settingsOpen = true;
  }

  const syncUi = $derived(getSyncUiMeta(syncStatus));
  const syncIconName = $derived(syncUi.iconName);
  const syncStatusLabel = $derived(syncUi.label);
  const syncTooltip = $derived(syncUi.tooltip);

  let saveTimer: number | undefined;
  let saveInFlight: Promise<void> | undefined;
  // Drawer swipe: the drawer follows the finger, then settles open or closed.
  type DrawerDrag = {
    el: HTMLElement;
    kind: "sidebar" | "panel";
    opening: boolean;
    width: number;
  };
  let touchStartX = 0;
  let touchStartY = 0;
  let touchStartT = 0;
  let touchStartTarget: EventTarget | null = null;
  let swipeEligible = false;
  let swipeLocked = false;
  let drawerDrag: DrawerDrag | null = null;
  /** 0–1 while a drawer is being dragged (drives the backdrop); null otherwise. */
  let dragProgress = $state<number | null>(null);

  function onTouchStart(e: TouchEvent): void {
    swipeEligible = false;
    swipeLocked = false;
    drawerDrag = null;
    if (isDesktop || e.touches.length !== 1) return;
    if (touchDraggingActive || treeContext || treeActionModal) return;
    if (paletteOpen || settingsOpen || trashOpen || vaultModalOpen || attachmentMenu) return;
    const targetEl = e.target instanceof Element ? e.target : null;
    if (targetEl?.closest(".format-pill, .menu-popover, .cm-wiki-completion")) return;
    if (isHorizontallyScrollable(targetEl)) return;
    touchStartX = e.touches[0].clientX;
    touchStartY = e.touches[0].clientY;
    touchStartT = e.timeStamp;
    touchStartTarget = e.target;
    swipeEligible = true;
  }

  function drawerOffset(d: DrawerDrag, dx: number): number {
    // translateX for the drawer at this finger offset, clamped to its travel.
    const w = d.width;
    if (d.kind === "sidebar")
      return d.opening ? Math.min(0, -w + Math.max(0, dx)) : Math.max(-w, Math.min(0, dx));
    return d.opening ? Math.max(0, w + Math.min(0, dx)) : Math.min(w, Math.max(0, dx));
  }

  function onTouchMove(e: TouchEvent): void {
    if (!swipeEligible || e.touches.length !== 1 || touchDraggingActive) return;
    const dx = e.touches[0].clientX - touchStartX;
    const dy = e.touches[0].clientY - touchStartY;
    if (!swipeLocked) {
      if (Math.abs(dx) < 10 && Math.abs(dy) < 10) return;
      if (Math.abs(dx) < Math.abs(dy) * 1.4) {
        swipeEligible = false; // vertical scroll wins
        return;
      }
      const kind: DrawerDrag["kind"] | null =
        dx > 0
          ? panelOpen
            ? "panel"
            : sidebarOpen
              ? null
              : "sidebar"
          : sidebarOpen
            ? "sidebar"
            : panelOpen
              ? null
              : "panel";
      const el = kind ? document.querySelector<HTMLElement>(`.app > .${kind}`) : null;
      if (!kind || !el) {
        swipeEligible = false;
        return;
      }
      swipeLocked = true;
      touchStartX += Math.sign(dx) * 10; // start tracking from here, without a jump
      drawerDrag = {
        el,
        kind,
        opening: kind === "sidebar" ? !sidebarOpen : !panelOpen,
        width: el.offsetWidth,
      };
      el.style.transition = "none";
    }
    const d = drawerDrag;
    if (!d) return;
    e.preventDefault(); // keep the page from scrolling under the drag
    const x = drawerOffset(d, e.touches[0].clientX - touchStartX);
    d.el.style.transform = `translateX(${x}px)`;
    dragProgress = 1 - Math.abs(x) / d.width;
  }

  function onTouchEnd(e: TouchEvent): void {
    const d = drawerDrag;
    drawerDrag = null;
    swipeEligible = false;
    if (!d) return;
    const dx = e.changedTouches[0].clientX - touchStartX;
    const moved = Math.abs(dx) / d.width;
    const flick = Math.abs(dx) > 30 && Math.abs(dx) / Math.max(1, e.timeStamp - touchStartT) > 0.5;
    const commit = moved > 0.35 || flick;
    const open = d.opening ? commit : !commit;
    // Hand back to the CSS transition, which animates from the dragged position.
    d.el.style.transition = "";
    d.el.style.transform = "";
    dragProgress = null;
    if (d.kind === "sidebar") sidebarOpen = open;
    else panelOpen = open;
  }

  const activeTitle = $derived(notes.find((n) => n.note_id === activeId)?.title ?? "");

  function currentEditor(): EditorHandle | undefined {
    return editorStore.current(activeFormat);
  }

  function registerEditor(format: EditorKind, handle: EditorHandle): void {
    editorStore.register(format, handle, settings.source_mode);
  }

  function unregisterEditor(format: EditorKind, handle: EditorHandle): void {
    editorStore.unregister(format, handle);
  }

  function clearActiveNoteState(): void {
    activeId = null;
    activeNoteContent = "";
    activePath = "";
    dirty = false;
    backlinks = [];
    unlinked = [];
    outlinks = [];
  }

  function handleChange(): void {
    dirty = true;
    externalChanged = false;
    status = "Editando…";
    scheduleSave();
    refreshStats();
  }

  function refreshStats(): void {
    const text = currentEditor()?.getPlainText() ?? "";
    stats = { words: countWords(text), chars: text.length };
  }

  function handleLink(target: string): void {
    const clean = target.split("|")[0].trim();
    const hashIdx = clean.indexOf("#");
    const notePart = (hashIdx >= 0 ? clean.slice(0, hashIdx) : clean).trim();
    const headingPart = hashIdx >= 0 ? clean.slice(hashIdx + 1).trim() : "";

    const scrollHeading = (h: string) => {
      if (!h) return;
      requestAnimationFrame(() => {
        currentEditor()?.scrollToHeading?.(h);
        setTimeout(() => currentEditor()?.scrollToHeading?.(h), 60);
      });
    };

    if (!notePart) {
      scrollHeading(headingPart);
      return;
    }

    const norm = (s: string) =>
      s
        .toLowerCase()
        .normalize("NFD")
        .replace(/[\u0300-\u036f]/g, "");
    const targetNorm = norm(notePart);
    const byId = notes.find((n) => n.note_id === notePart);
    const byTitle = notes.find((n) => norm(n.title) === targetNorm || norm(n.path) === targetNorm);
    const match = byId ?? byTitle;
    if (match) {
      void openNote(match.note_id).then(() => scrollHeading(headingPart));
    } else {
      void (async () => {
        try {
          await save();
          const currentFolder = folderOf(activePath);
          const created = await call("note_create", {
            kind: "markdown",
            title: notePart,
            folder: currentFolder || activeFolder || null,
          });
          if (headingPart) {
            const withSection = `${created.content.trimEnd()}\n\n## ${headingPart}\n\n`;
            await call("note_save", {
              noteId: created.note.note_id,
              content: withSection,
              baseRev: created.rev,
            });
          }
          invalidateEmbedCache();
          await refresh();
          await openNote(created.note.note_id);
          if (headingPart) scrollHeading(headingPart);
        } catch (e) {
          error = formatError(e);
        }
      })();
    }
  }

  function scheduleSave(): void {
    if (saveTimer !== undefined) window.clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => void save(), 500);
  }

  async function save(): Promise<void> {
    if (saveTimer !== undefined) {
      window.clearTimeout(saveTimer);
      saveTimer = undefined;
    }
    if (saveInFlight) await saveInFlight;
    let failed = false;
    while (activeId && dirty && !saving && !failed) {
      saving = true;
      status = "Guardando…";
      const run = (async () => {
        try {
          const content = currentEditor()?.getContent() ?? "";
          const savedId = activeId!;
          const result = await call("note_save", { noteId: savedId, content, baseRev: rev });
          invalidateEmbedCache();
          if (activeId === savedId) {
            rev = result.rev;
            activeNoteContent = content;
            notes = notes.map((n) => (n.note_id === savedId ? result.note : n));
            const unchanged = (currentEditor()?.getContent() ?? "") === content;
            dirty = !unchanged;
            status = unchanged ? "Guardado" : "Editando…";
          }
        } catch (e) {
          const msg = formatError(e);
          if (msg.toLowerCase().includes("conflict") && activeId) {
            try {
              const disk = await call("note_read", { noteId: activeId });
              recordConflict({
                noteId: activeId,
                title: activeTitle || disk.note.title,
                path: disk.note.path,
                localContent: currentEditor()?.getContent() ?? activeNoteContent,
                diskContent: disk.content,
                diskRev: disk.rev,
                detectedAt: new Date().toLocaleTimeString("es-ES"),
              });
              // The local text now lives in the conflict entry.
              status = "Conflicto detectado";
              dirty = false;
              return;
            } catch {
              // fall through: keep the edits dirty
            }
          }
          // Keep `dirty` so the edits are not dropped; openNote() refuses to switch away.
          error = msg;
          status = "Error al guardar";
          failed = true;
        } finally {
          saving = false;
        }
      })();
      saveInFlight = run;
      await run;
      saveInFlight = undefined;
    }
  }

  async function refresh(): Promise<void> {
    const [noteList, loadedSettings, folderList] = await Promise.all([
      call("note_list"),
      call("vault_get_settings"),
      call("vault_list_folders").then((items) =>
        items.map((item) => item.path.split("/").slice(2).join("/")).filter(Boolean),
      ),
    ]);
    notes = noteList;
    settingsStore.set(loadedSettings);
    folders = folderList;
    // Renamed images rewrite their notes' links: show the new text.
    void compressPendingAttachments(loadedSettings.image_quality).then(
      (changed) => void (changed && onVaultChanged([])),
    );
  }

  async function loadBacklinks(id: string, title: string): Promise<void> {
    try {
      const [b, u, o] = await Promise.all([
        call("note_get_backlinks", { noteId: id }),
        call("note_unlinked_mentions", { noteId: id, title }),
        call("note_get_outlinks", { noteId: id }),
      ]);
      backlinks = b;
      unlinked = u;
      outlinks = o;
    } catch {
      // non-fatal
    }
  }

  async function openNote(id: string): Promise<void> {
    // Commit a title being typed before the editor (and its blur handler) goes away.
    const focused = document.activeElement;
    if (focused instanceof HTMLElement && focused.classList.contains("cm-inline-title")) {
      focused.blur();
    }
    await save();
    if (dirty) return; // save failed: switching would discard the unsaved text
    try {
      const note = await call("note_read", { noteId: id });
      activeFormat = note.note.format;
      activeNoteContent = note.content;
      activePath = note.note.path;
      rev = note.rev;
      activeId = id;
      loadNonce += 1;
      dirty = false;
      externalChanged = false;
      status = "Listo";
      error = "";
      editorStore.viewMode = "edit";
      stats = statsOf(note.content);
      if (!isDesktop) {
        sidebarOpen = false;
        panelOpen = false;
      }
      await loadBacklinks(id, note.note.title);
    } catch (e) {
      error = formatError(e);
      status = "Error";
    }
  }

  function recordConflict(item: ConflictItem): void {
    conflicts = [item, ...conflicts.filter((c) => c.noteId !== item.noteId)];
    externalChanged = true;
    panelOpen = true;
    panelTab = "conflicts";
  }

  async function onVaultChanged(paths?: string[] | null): Promise<void> {
    invalidateEmbedCache();
    const changedPaths = Array.isArray(paths) ? paths : [];
    await refresh();
    if (!activeId) return;
    await loadBacklinks(activeId, activeTitle);
    const activeAffected =
      changedPaths.length === 0 || (Boolean(activePath) && changedPaths.includes(activePath));
    if (activeAffected) {
      try {
        const note = await call("note_read", { noteId: activeId });
        const shown = currentEditor()?.getContent() ?? activeNoteContent;
        if (note.content === shown) {
          // Same text (our own save, a no-op sync, app resume): keep the editor mounted.
          activeNoteContent = note.content;
          activePath = note.note.path;
          rev = note.rev;
        } else if (note.content !== activeNoteContent || note.rev !== rev) {
          if (dirty) {
            recordConflict({
              noteId: activeId,
              title: activeTitle || note.note.title,
              path: note.note.path,
              localContent: currentEditor()?.getContent() ?? activeNoteContent,
              diskContent: note.content,
              diskRev: note.rev,
              detectedAt: new Date().toLocaleTimeString("es-ES"),
            });
          } else {
            activeNoteContent = note.content;
            activePath = note.note.path;
            rev = note.rev;
            loadNonce += 1;
            stats = statsOf(note.content);
          }
        }
      } catch {
        // active note may have been deleted externally
      }
    }
  }

  async function reloadActive(): Promise<void> {
    externalChanged = false;
    if (activeId) await openNote(activeId);
  }

  let createMenuOpen = $state(false);

  async function createNote(folder?: string, initialContent?: string): Promise<void> {
    createMenuOpen = false;
    try {
      const note = await call("note_create", {
        kind: "markdown",
        title: null,
        folder: folder ?? activeFolder ?? null,
      });
      if (initialContent) {
        await call("note_save", {
          noteId: note.note.note_id,
          content: initialContent,
          baseRev: note.rev,
        });
      }
      await refresh();
      await openNote(note.note.note_id);
      // Select the placeholder title so typing renames the new note.
      await tick();
      const title = document.querySelector<HTMLTextAreaElement>(".cm-inline-title");
      title?.focus();
      title?.select();
    } catch (e) {
      error = formatError(e);
    }
  }

  async function createFolder(parentFolder?: string): Promise<void> {
    treeContext = null;
    error = "";
    treeActionModal = { kind: "create-folder", parentFolder };
  }

  async function renameNote(): Promise<void> {
    const cur = notes.find((n) => n.note_id === activeId);
    if (!cur) return;
    treeContext = null;
    error = "";
    treeActionModal = { kind: "rename-note", note: cur };
  }

  // Inline title rename; same link-repair prompt as the rename dialog.
  async function renameActiveNote(nextTitle: string): Promise<boolean> {
    const cur = notes.find((n) => n.note_id === activeId);
    if (!cur || nextTitle === cur.title) return false;
    error = "";
    try {
      await save();
      const references = await call("note_check_link_references", { oldTitle: cur.title });
      if (references.length > 0) {
        treeActionModal = { kind: "repair-links", note: cur, newTitle: nextTitle, references };
        return false;
      }
      await call("note_rename", { noteId: cur.note_id, newTitle: nextTitle });
      // The rename bumps the note's rev and path; without this the next save is a false conflict.
      const renamed = await call("note_read", { noteId: cur.note_id });
      if (activeId === cur.note_id) {
        rev = renamed.rev;
        activePath = renamed.note.path;
      }
      await refresh();
      return true;
    } catch (e) {
      error = formatError(e);
      return false;
    }
  }

  async function moveNote(): Promise<void> {
    const cur = notes.find((n) => n.note_id === activeId);
    if (!cur) return;
    treeContext = null;
    error = "";
    treeActionModal = { kind: "move-note", note: cur };
  }

  async function deleteNote(): Promise<void> {
    const cur = notes.find((n) => n.note_id === activeId);
    if (!cur) return;
    treeContext = null;
    error = "";
    treeActionModal = { kind: "delete-note", note: cur };
  }

  async function saveSettings(s: SettingsDto): Promise<void> {
    settingsOpen = false;
    try {
      await settingsStore.save(s);
    } catch (e) {
      error = formatError(e);
    }
  }

  async function createOrOpenDemoNotes(overwrite = true): Promise<void> {
    try {
      const guideId = await ensureDemoNotes(notes, overwrite);
      await refresh();
      await openNote(guideId);
    } catch (e) {
      error = formatError(e);
    }
  }

  async function importAttachment(file?: File): Promise<void> {
    if (!file) return;
    try {
      const upload = await compressImage(file, settingsStore.settings.image_quality);
      const path = await importAttachmentFile(upload);
      currentEditor()?.getEngine()?.insertAttachment?.(path, upload.type);
    } catch (e) {
      error = formatError(e);
    }
    attachmentMenu = false;
  }

  function toggleViewMode(): void {
    const next: ViewMode = editorStore.viewMode === "edit" ? "reading" : "edit";
    editorStore.viewMode = next;
    currentEditor()?.setViewMode?.(next, settings.source_mode);
  }

  function onKeydown(e: KeyboardEvent): void {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
      e.preventDefault();
      paletteOpen = true;
    }
  }

  /**
   * Android back button / gesture: close the top-most open layer. Modals
   * register their own handlers (see `lib/backStack`); this one covers the
   * shell. Returning false lets Android leave the app.
   */
  function onBack(): boolean {
    if (moreOpen) return ((moreOpen = false), true);
    if (attachmentMenu) return ((attachmentMenu = false), true);
    if (treeActionModal) return ((treeActionModal = null), true);
    if (treeContext) return ((treeContext = null), true);
    if (panelOpen) return ((panelOpen = false), true);
    if (!isDesktop && sidebarOpen) return ((sidebarOpen = false), true);
    if (activeId) {
      void save().then(() => {
        clearActiveNoteState();
        status = "Listo";
      });
      return true;
    }
    return false;
  }

  onMount(() => pushBackHandler(onBack));

  onMount(() => {
    // Subscribe to backend events before initializing the vault so none are
    // missed, and release a subscription that resolves after teardown.
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];
    const subscribe = (pending: Promise<UnlistenFn>): void => {
      void pending
        .then((unlisten) => {
          if (disposed) unlisten();
          else unlisteners.push(unlisten);
        })
        .catch(() => {});
    };
    subscribe(listen<SyncStatusDto>("sync://status-changed", (e) => (syncStatus = e.payload)));
    subscribe(listen("vault://reconciled", () => vault && void refresh()));
    subscribe(listen<string[]>("vault-changed", (e) => vault && void onVaultChanged(e.payload)));
    subscribe(listen(TauriEvent.WINDOW_SUSPENDED, () => void save()));
    subscribe(listen(TauriEvent.WINDOW_RESUMED, () => vault && void refresh()));
    const onAttachmentMenu = () => (attachmentMenu = true);
    const onVisibility = () => {
      if (document.visibilityState === "hidden") void save();
    };
    updateViewport();
    window.visualViewport?.addEventListener("resize", updateViewport);
    (async () => {
      try {
        vault = await call("vault_init");
        if (vault.is_first_run) {
          firstRunVaultSetup = true;
          vaultModalOpen = true;
        }
        await refresh();
        // Seed the demo notes only on a brand-new vault: once deleted they must
        // stay deleted (re-seeding re-created them on every launch and synced
        // them back to other devices). The web build never seeds the synced vault.
        if (
          vault.is_first_run &&
          !import.meta.env.VITE_NOTIUM_WEB &&
          !notes.some((n) => n.title === DEMO_GUIDE_TITLE)
        ) {
          await ensureDemoNotes(notes, false);
          await refresh();
        }
        status = "Listo";
      } catch (e) {
        console.error("Vault initialization failed:", e);
        error = formatError(e);
        status = "Error";
      }
      hideSplash();
      try {
        syncStatus = await call("get_sync_status");
      } catch {
        // Keep the default "offline" status; sync is optional.
      }
    })();
    window.addEventListener("keydown", onKeydown);
    window.addEventListener("touchstart", onTouchStart, { passive: true });
    window.addEventListener("touchmove", onTouchMove, { passive: false });
    window.addEventListener("touchend", onTouchEnd, { passive: true });
    window.addEventListener("touchcancel", onTouchEnd, { passive: true });
    window.addEventListener("notium:attachment-menu", onAttachmentMenu);
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      disposed = true;
      for (const unlisten of unlisteners) unlisten();
      window.removeEventListener("keydown", onKeydown);
      window.removeEventListener("touchstart", onTouchStart);
      window.removeEventListener("touchmove", onTouchMove);
      window.removeEventListener("touchend", onTouchEnd);
      window.removeEventListener("touchcancel", onTouchEnd);
      window.removeEventListener("notium:attachment-menu", onAttachmentMenu);
      document.removeEventListener("visibilitychange", onVisibility);
      window.visualViewport?.removeEventListener("resize", updateViewport);
    };
  });
</script>

<div
  class="app"
  style:--sidebar-width={isDesktop ? `${layout.panelWidths.sidebar}px` : null}
  style:--panel-width={isDesktop ? `${layout.panelWidths.panel}px` : null}
  class:is-desktop={isDesktop}
  class:sidebar-closed={!sidebarOpen}
  class:panel-open={panelOpen}
>
  {#if !isDesktop}
    <div
      class="backdrop"
      class:visible={sidebarOpen || panelOpen}
      class:dragging={dragProgress !== null}
      style:opacity={dragProgress}
      role="presentation"
      onclick={() => ((sidebarOpen = false), (panelOpen = false))}
    ></div>
  {/if}

  {#if isDesktop}
    <nav class="desktop-ribbon" aria-label="Barra de acciones">
      <div class="ribbon-top">
        <button
          class="ribbon-btn"
          class:active={sidebarOpen}
          onclick={() => (sidebarOpen = !sidebarOpen)}
          title={sidebarOpen ? "Ocultar barra lateral" : "Mostrar barra lateral"}
          aria-label="Barra lateral"
        >
          <Icon name="sidebar" size={18} />
        </button>
      </div>
      <div class="ribbon-actions">
        <button
          class="ribbon-btn"
          title="Buscar notas (Ctrl+K)"
          onclick={() => (paletteOpen = true)}><Icon name="search" size={18} /></button
        >
        <button
          class="ribbon-btn"
          class:active={sidebarOpen && drawerView === "tags"}
          title="Etiquetas (Tags)"
          onclick={() => ((sidebarOpen = true), (drawerView = "tags"))}
          ><Icon name="hash" size={18} /></button
        >
        <button class="ribbon-btn" title="Nueva nota" onclick={() => void createNote()}
          ><Icon name="note-new" size={18} /></button
        >
        <button
          class="ribbon-btn"
          class:active={panelOpen}
          title={panelOpen ? "Ocultar backlinks" : "Mostrar backlinks"}
          onclick={() => (panelOpen = !panelOpen)}><Icon name="link" size={18} /></button
        >
      </div>
      <div class="ribbon-bottom">
        <button
          class="ribbon-btn"
          class:active={attachmentsModalOpen}
          title="Gestor de adjuntos"
          onclick={() => (attachmentsModalOpen = true)}><Icon name="attachment" size={18} /></button
        >
        <button
          class="ribbon-btn"
          class:active={trashOpen}
          title="Papelera"
          onclick={() => (trashOpen = true)}><Icon name="trash" size={18} /></button
        >
        <button
          class="ribbon-btn"
          title={settingsStore.resolvedTheme === "light"
            ? "Cambiar a tema oscuro"
            : "Cambiar a tema claro"}
          onclick={toggleTheme}
          ><Icon
            name={settingsStore.resolvedTheme === "light" ? "moon" : "sun"}
            size={18}
          /></button
        >
        <button
          class="ribbon-btn"
          class:active={settingsOpen && settingsInitialTab === "sync"}
          title={syncTooltip}
          onclick={openSyncSettings}><Icon name={syncIconName} size={18} /></button
        >
        <button
          class="ribbon-btn"
          title="Ajustes"
          onclick={() => ((settingsInitialTab = "general"), (settingsOpen = true))}
          ><Icon name="settings" size={18} /></button
        >
      </div>
    </nav>
  {/if}

  <SidebarDrawer
    {isDesktop}
    {sidebarOpen}
    {drawerView}
    {vault}
    {notes}
    {folders}
    {activeId}
    {activePath}
    {activeFolder}
    {syncStatus}
    {syncIconName}
    {syncStatusLabel}
    {syncTooltip}
    {touchDraggingActive}
    onSetSidebarOpen={(open) => (sidebarOpen = open)}
    onSetDrawerView={(view) => (drawerView = view)}
    onSetActiveFolder={(folder) => (activeFolder = folder)}
    onSetTouchDragging={(active) => {
      touchDraggingActive = active;
      if (active) treeContext = null;
    }}
    onShowTreeContext={(ctx) => (treeContext = ctx)}
    onOpenNote={openNote}
    onCreateNote={() => createNote()}
    onCreateFolder={() => createFolder()}
    onSaveBeforeAction={save}
    onRefresh={refresh}
    onOpenSyncSettings={openSyncSettings}
    onOpenSettings={() => ((settingsInitialTab = "general"), (settingsOpen = true))}
    onOpenVaultModal={() => (vaultModalOpen = true)}
    onError={(msg) => (error = msg)}
  />

  <main class="editor">
    <div class="top-controls">
      {#if !isDesktop}
        <button
          class="floating-btn"
          onclick={() => (sidebarOpen = !sidebarOpen)}
          aria-label="Barra lateral"><Icon name="sidebar" size={18} /></button
        >
      {/if}
      <span class="top-title">
        {#if status === "Editando…" || status === "Guardando…"}<span
            class="status-dot"
            class:busy={status === "Guardando…"}
          ></span>{/if}
      </span>
      <div class="top-right">
        <button
          class="sync-indicator-btn"
          class:syncing={syncStatus.state === "syncing"}
          class:synced={syncStatus.state === "synced"}
          class:error={syncStatus.state === "error"}
          title={syncTooltip}
          onclick={openSyncSettings}
          aria-label={syncTooltip}
        >
          <Icon name={syncIconName} size={15} />
          {#if syncStatus.state === "syncing"}<span style="font-size: 11px; margin-left: 2px;"
              >Sync…</span
            >{/if}
        </button>
        {#if activeId}
          <button
            class="floating-btn"
            onclick={toggleViewMode}
            aria-label={viewMode === "edit" ? "Cambiar a Solo vista" : "Cambiar a Vista en vivo"}
            title={viewMode === "edit" ? "Solo vista" : "Vista en vivo"}
          >
            <Icon name={viewMode === "edit" ? "book" : "pencil"} size={18} />
          </button>
        {/if}
        <div class="pop-wrap">
          <button
            class="floating-btn"
            onclick={() => (moreOpen = !moreOpen)}
            aria-label="Más acciones"><Icon name="dots" size={18} /></button
          >
          {#if moreOpen}
            <div class="menu-popover">
              <button class="more-item" onclick={() => ((moreOpen = false), void renameNote())}
                >✏️ Renombrar nota</button
              >
              <button class="more-item" onclick={() => ((moreOpen = false), void moveNote())}
                >📂 Mover a carpeta</button
              >
              <button class="more-item" onclick={() => ((moreOpen = false), void deleteNote())}
                >🗑️ Eliminar nota</button
              >
              <div class="menu-sep"></div>
              <button
                class="more-item"
                onclick={() => ((moreOpen = false), (panelOpen = !panelOpen))}
                >{panelOpen ? "🔗 Ocultar backlinks" : "🔗 Mostrar backlinks"}</button
              >
              <button
                class="more-item"
                onclick={() => ((moreOpen = false), editorStore.toggleFullWidth())}
                >{editorStore.fullWidth ? "📖 Ancho de lectura" : "↔️ Ancho completo"}</button
              >
              <button class="more-item" onclick={() => ((moreOpen = false), toggleTheme())}
                >{settingsStore.resolvedTheme === "light" ? "🌙 Tema oscuro" : "☀️ Tema claro"}</button
              >
              <button class="more-item" onclick={() => ((moreOpen = false), openSyncSettings())}
                >☁️ Sincronización Cloud</button
              >
              <button
                class="more-item"
                onclick={() => ((moreOpen = false), (attachmentsModalOpen = true))}>📎 Adjuntos</button
              >
              <button class="more-item" onclick={() => ((moreOpen = false), (trashOpen = true))}
                >🗑️ Papelera</button
              >
              <button
                class="more-item"
                onclick={() => ((moreOpen = false), (vaultModalOpen = true))}
                >🗄️ Gestionar Vaults</button
              >
              <button
                class="more-item"
                onclick={() => (
                  (moreOpen = false),
                  (settingsInitialTab = "general"),
                  (settingsOpen = true)
                )}>⚙️ Ajustes</button
              >
            </div>
          {/if}
        </div>
      </div>
    </div>

    {#if error}
      <div class="error-banner">
        <span>{error}</span>
        <button
          class="error-banner-dismiss"
          onclick={() => ((error = ""), void save(), void refresh())}>Reintentar</button
        >
      </div>
    {/if}
    {#if externalChanged}
      <div class="reload-banner">
        <span>Esta nota cambió en el disco y tiene cambios locales.</span>
        <div class="reload-banner-actions">
          <button onclick={() => ((panelOpen = true), (panelTab = "conflicts"))}
            >Resolver conflicto</button
          >
          <button onclick={() => void reloadActive()}>Recargar del disco</button>
        </div>
      </div>
    {/if}

    {#if activeId}
      <div class="editor-body">
        {#key loadNonce}
          <MarkdownEditor
            content={activeNoteContent}
            {viewMode}
            sourceMode={settings.source_mode}
            onChange={handleChange}
            onLinkActivate={handleLink}
            title={activeTitle}
            onRenameTitle={renameActiveNote}
            onReady={(handle) => registerEditor("markdown", handle)}
            onEditorDestroy={(handle) => unregisterEditor("markdown", handle)}
          />
        {/key}
      </div>

      {#if viewMode === "edit"}
        <div class="toolbar-anchor">
          <Toolbar actions={editorStore.mainActions} />
        </div>
      {/if}
      {#if attachmentMenu}
        <div class="attachment-menu" role="dialog" aria-label="Añadir adjunto">
          {#if layout.mobilePlatform}<button onclick={() => cameraInput?.click()}
              >Tomar foto (Cámara)</button
            >{/if}
          <button onclick={() => galleryInput?.click()}
            >{layout.mobilePlatform ? "Cargar archivo / Galería" : "Cargar archivo"}</button
          >
          <button class="attachment-cancel" onclick={() => (attachmentMenu = false)}
            >Cancelar</button
          >
        </div>
      {/if}
      <input
        bind:this={galleryInput}
        class="hidden-file"
        type="file"
        onchange={(e) => void importAttachment(e.currentTarget.files?.[0])}
      />
      <input
        bind:this={cameraInput}
        class="hidden-file"
        type="file"
        accept="image/*"
        capture="environment"
        onchange={(e) => void importAttachment(e.currentTarget.files?.[0])}
      />
    {:else}
      <div class="empty">
        <Icon name="file" size={32} />
        <p>Selecciona una nota o crea una nueva.</p>
      </div>
      <div class="create-fab">
        {#if createMenuOpen}
          <button class="create-fab-option" onclick={() => void createNote()}>
            <Icon name="file" size={18} /> Nota .md
          </button>
          <button class="create-fab-option" onclick={() => void createNote(undefined, "- [ ] ")}>
            <Icon name="checklist" size={18} /> Checklist
          </button>
        {/if}
        <button
          class="create-fab-main"
          aria-label={createMenuOpen ? "Cancelar" : "Nueva nota"}
          aria-expanded={createMenuOpen}
          onclick={() => (createMenuOpen = !createMenuOpen)}
        >
          <Icon name={createMenuOpen ? "x" : "note-new"} size={22} />
        </button>
      </div>
    {/if}
  </main>

  <RightInspectorPanel
    {isDesktop}
    {panelOpen}
    {panelTab}
    {activeId}
    {activeTitle}
    {backlinks}
    {unlinked}
    {outlinks}
    {conflicts}
    {stats}
    onSetPanelOpen={(open) => (panelOpen = open)}
    onSetPanelTab={(tab) => (panelTab = tab)}
    onOpenNote={openNote}
    onHandleLink={handleLink}
    onOpenAttachmentsModal={() => (attachmentsModalOpen = true)}
    onOpenSearchDrawer={() => {
      if (!isDesktop) panelOpen = false;
      sidebarOpen = true;
      drawerView = "search";
    }}
    onSaveBeforeAction={save}
    onLoadBacklinks={loadBacklinks}
    onRefresh={refresh}
    onRemoveConflict={(noteId) => (conflicts = conflicts.filter((x) => x.noteId !== noteId))}
    onApplyLocalResolution={(newRev, content) => {
      rev = newRev;
      activeNoteContent = content;
      loadNonce += 1;
      dirty = false;
      externalChanged = false;
      status = "Guardado";
    }}
    onClearExternalConflictFlags={() => ((dirty = false), (externalChanged = false))}
    onError={(msg) => (error = msg)}
  />
</div>

{#if paletteOpen}
  <CommandPalette
    onSelect={(id) => ((paletteOpen = false), void openNote(id))}
    onClose={() => (paletteOpen = false)}
  />
{/if}

{#if settingsOpen}
  <SettingsModal
    {settings}
    initialTab={settingsInitialTab}
    onSave={saveSettings}
    onOpenNote={(id) => {
      settingsOpen = false;
      settingsInitialTab = "general";
      void refresh().then(() => openNote(id));
    }}
    onCreateDemoNotes={() => void createOrOpenDemoNotes(true)}
    onClose={() => ((settingsOpen = false), (settingsInitialTab = "general"))}
  />
{/if}

{#if editorStore.toolbarConfigOpen}
  <ToolbarConfig onClose={() => (editorStore.toolbarConfigOpen = false)} />
{/if}

{#if trashOpen}
  <TrashModal
    onRestored={(restored) => void refresh().then(() => openNote(restored.note_id))}
    onClose={() => (trashOpen = false)}
  />
{/if}

{#if attachmentsModalOpen}
  <AttachmentsModal
    onOpenNote={(id) => ((attachmentsModalOpen = false), void openNote(id))}
    onClose={() => (attachmentsModalOpen = false)}
  />
{/if}

{#if vaultModalOpen}
  <VaultModal
    currentVault={vault}
    isDesktop={!layout.mobilePlatform}
    firstRun={firstRunVaultSetup}
    onSwitched={(info) => {
      vault = info;
      firstRunVaultSetup = Boolean(info.is_first_run);
      clearActiveNoteState();
      void refresh();
    }}
    onClose={() => ((firstRunVaultSetup = false), (vaultModalOpen = false))}
  />
{/if}

<TreeActionModal
  {treeContext}
  {treeActionModal}
  {notes}
  {folders}
  {activeId}
  {activeFolder}
  onCloseContext={() => (treeContext = null)}
  onSetActionModal={(modal) => (treeActionModal = modal)}
  onSetActiveFolder={(folder) => (activeFolder = folder)}
  onClearActiveNote={clearActiveNoteState}
  onSaveBeforeAction={save}
  onRefresh={refresh}
  onOpenNote={openNote}
  onError={(msg) => (error = msg)}
/>
