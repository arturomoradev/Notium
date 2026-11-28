<script lang="ts">
  import { call } from "../../lib/ipc/commands";
  import Icon from "../editors/Icon.svelte";
  import { folderOf, formatError, type ConflictItem, type PanelTab } from "../../lib/vault/tree";
  import type { BacklinkDto, OutlinkDto } from "../../lib/ipc/types";
  import { linkFirstMention, type Stats } from "../../lib/plainText";

  interface Props {
    isDesktop: boolean;
    panelOpen: boolean;
    panelTab: PanelTab;
    activeId: string | null;
    activeTitle: string;
    backlinks: BacklinkDto[];
    unlinked: BacklinkDto[];
    outlinks: OutlinkDto[];
    conflicts: ConflictItem[];
    stats: Stats;
    onSetPanelOpen: (open: boolean) => void;
    onSetPanelTab: (tab: PanelTab) => void;
    onOpenNote: (id: string) => Promise<void>;
    onHandleLink: (target: string) => void;
    onOpenAttachmentsModal: () => void;
    onOpenSearchDrawer: () => void;
    onSaveBeforeAction: () => Promise<void>;
    onLoadBacklinks: (id: string, title: string) => Promise<void>;
    onRefresh: () => Promise<void>;
    onRemoveConflict: (noteId: string) => void;
    onApplyLocalResolution: (rev: number, content: string) => void;
    onClearExternalConflictFlags: () => void;
    onError: (msg: string) => void;
  }

  let {
    isDesktop,
    panelOpen,
    panelTab,
    activeId,
    activeTitle,
    backlinks,
    unlinked,
    outlinks,
    conflicts,
    stats,
    onSetPanelOpen,
    onSetPanelTab,
    onOpenNote,
    onHandleLink,
    onOpenAttachmentsModal,
    onOpenSearchDrawer,
    onSaveBeforeAction,
    onLoadBacklinks,
    onRefresh,
    onRemoveConflict,
    onApplyLocalResolution,
    onClearExternalConflictFlags,
    onError,
  }: Props = $props();

  let backlinkLinkedOpen = $state(true);
  let backlinkUnlinkedOpen = $state(true);
  let backlinkOutlinksOpen = $state(true);
  let backlinkSort = $state<"az" | "za">("az");
  let backlinkFilter = $state("");
  let panelSearchInput = $state<HTMLInputElement>();
  let mobileViewMenuOpen = $state(false);

  const normalizedBacklinkFilter = $derived(backlinkFilter.trim().toLocaleLowerCase("es"));
  const mobilePanelMeta = $derived(
    panelTab === "backlinks"
      ? { icon: "link", label: "Backlinks" }
      : { icon: "alert", label: "Conflictos" },
  );

  function matchesFilter(query: string, values: Array<string | null>): boolean {
    return !query || values.some((value) => value?.toLocaleLowerCase("es").includes(query));
  }

  const sortedBacklinks = $derived(
    backlinks
      .filter((b) =>
        matchesFilter(normalizedBacklinkFilter, [b.source_title, b.snippet, b.link_text]),
      )
      .sort((a, b) =>
        backlinkSort === "az"
          ? a.source_title.localeCompare(b.source_title, "es")
          : b.source_title.localeCompare(a.source_title, "es"),
      ),
  );
  const sortedUnlinked = $derived(
    unlinked
      .filter((u) =>
        matchesFilter(normalizedBacklinkFilter, [u.source_title, u.snippet, u.link_text]),
      )
      .sort((a, b) =>
        backlinkSort === "az"
          ? a.source_title.localeCompare(b.source_title, "es")
          : b.source_title.localeCompare(a.source_title, "es"),
      ),
  );
  const filteredOutlinks = $derived(
    outlinks.filter((o) =>
      matchesFilter(normalizedBacklinkFilter, [o.target_title, o.target_raw, o.link_text]),
    ),
  );

  function selectMobilePanelTab(tab: PanelTab): void {
    mobileViewMenuOpen = false;
    onSetPanelTab(tab);
  }

  function handlePanelSearch(): void {
    if (!isDesktop && panelTab === "backlinks") {
      panelSearchInput?.focus();
      return;
    }
    onOpenSearchDrawer();
  }

  function setBacklinkSections(open: boolean): void {
    backlinkLinkedOpen = open;
    backlinkUnlinkedOpen = open;
    backlinkOutlinksOpen = open;
  }

  async function linkUnlinkedMention(sourceId: string): Promise<void> {
    if (!activeId || !activeTitle) return;
    try {
      await onSaveBeforeAction();
      const source = await call("note_read", { noteId: sourceId });
      const updated = linkFirstMention(source.content, activeTitle);
      if (updated) {
        await call("note_save", {
          noteId: sourceId,
          content: updated,
          baseRev: source.rev,
        });
        await onLoadBacklinks(activeId, activeTitle);
      }
    } catch (e) {
      onError(formatError(e));
    }
  }

  async function resolveKeepLocal(c: ConflictItem): Promise<void> {
    try {
      const latest = await call("note_read", { noteId: c.noteId });
      const res = await call("note_save", {
        noteId: c.noteId,
        content: c.localContent,
        baseRev: latest.rev,
      });
      onRemoveConflict(c.noteId);
      if (activeId === c.noteId) {
        onApplyLocalResolution(res.rev, c.localContent);
      }
      await onRefresh();
    } catch (e) {
      onError(formatError(e));
    }
  }

  async function resolveKeepDisk(c: ConflictItem): Promise<void> {
    onRemoveConflict(c.noteId);
    if (activeId === c.noteId) {
      onClearExternalConflictFlags();
      await onOpenNote(c.noteId);
    }
  }

  async function resolveSaveBoth(c: ConflictItem): Promise<void> {
    try {
      const folder = folderOf(c.path);
      const copyTitle = `${c.title} (Copia conflicto)`;
      const copyNote = await call("note_create", {
        kind: "tiptap",
        title: copyTitle,
        folder: folder || null,
      });
      await call("note_save", {
        noteId: copyNote.note.note_id,
        content: c.localContent,
        baseRev: copyNote.rev,
      });
      onRemoveConflict(c.noteId);
      onClearExternalConflictFlags();
      await onRefresh();
      if (activeId === c.noteId) {
        await onOpenNote(c.noteId);
      }
    } catch (e) {
      onError(formatError(e));
    }
  }
</script>

<!-- Tapping anywhere outside the view switcher closes its menu. -->
<svelte:window
  onpointerdown={(e) => {
    if (mobileViewMenuOpen && !(e.target as Element).closest?.(".mobile-view-switcher"))
      mobileViewMenuOpen = false;
  }}
/>

<aside class="panel" class:open={panelOpen}>
  <div class="panel-head">
    <span class="panel-title">
      <Icon name={panelTab === "conflicts" ? "alert" : "link"} size={15} />
      {panelTab === "conflicts" ? "Conflictos" : "Enlaces y Backlinks"}
    </span>
    <button class="floating-btn small" onclick={() => onSetPanelOpen(false)} aria-label="Cerrar"
      >✕</button
    >
  </div>

  {#if !isDesktop && panelTab === "backlinks"}
    <div class="panel-search">
      <Icon name="search" size={18} />
      <input
        bind:this={panelSearchInput}
        bind:value={backlinkFilter}
        type="search"
        placeholder="Search..."
        aria-label="Filtrar enlaces y backlinks"
        autocomplete="off"
      />
    </div>
  {/if}

  <div class="panel-scroll">
    {#if panelTab === "backlinks"}
      <div class="bl-section">
        <button
          type="button"
          class="bl-heading"
          onclick={() => (backlinkLinkedOpen = !backlinkLinkedOpen)}
        >
          <span>Linked mentions</span>
          <span class="bl-count">{sortedBacklinks.length}</span>
        </button>
        {#if backlinkLinkedOpen}
          {#each sortedBacklinks as b (b.source_id + b.link_text)}
            <button class="bl-item" onclick={() => void onOpenNote(b.source_id)}>
              <strong>{b.source_title}</strong>
              <span class="bl-snippet">{b.snippet || b.link_text}</span>
            </button>
          {/each}
          {#if sortedBacklinks.length === 0}
            <div class="list-caption">
              {backlinkFilter.trim()
                ? "No linked mentions match your search."
                : "No backlinks found."}
            </div>
          {/if}
        {/if}
      </div>

      <div class="bl-section">
        <button
          type="button"
          class="bl-heading"
          onclick={() => (backlinkUnlinkedOpen = !backlinkUnlinkedOpen)}
        >
          <span>Unlinked mentions</span>
          <span class="bl-count">{sortedUnlinked.length}</span>
        </button>
        {#if backlinkUnlinkedOpen}
          {#each sortedUnlinked as u (u.source_id)}
            <div class="bl-item has-link">
              <button class="bl-main" onclick={() => void onOpenNote(u.source_id)}>
                <strong>{u.source_title}</strong>
                <span class="bl-snippet">{u.snippet || u.link_text}</span>
              </button>
              <button
                class="bl-link-btn"
                title="Enlazar mención"
                onclick={() => void linkUnlinkedMention(u.source_id)}
              >
                <Icon name="note-link" size={14} />
              </button>
            </div>
          {/each}
          {#if sortedUnlinked.length === 0}
            <div class="list-caption">
              {backlinkFilter.trim()
                ? "No unlinked mentions match your search."
                : "No unlinked mentions found."}
            </div>
          {/if}
        {/if}
      </div>

      <div class="bl-section">
        <button
          type="button"
          class="bl-heading"
          onclick={() => (backlinkOutlinksOpen = !backlinkOutlinksOpen)}
        >
          <span>Enlaces salientes</span>
          <span class="bl-count">{filteredOutlinks.length}</span>
        </button>
        {#if backlinkOutlinksOpen}
          {#each filteredOutlinks as o (o.target_raw + o.link_text)}
            <div class="bl-item has-link outlink-item" class:is-unresolved={!o.is_resolved}>
              {#if o.is_resolved}
                <button
                  type="button"
                  class="bl-main outlink-main"
                  onclick={() =>
                    o.is_attachment ? onOpenAttachmentsModal() : onHandleLink(o.target_raw)}
                  title={o.is_attachment ? "Abrir en Gestor de Adjuntos" : "Abrir nota"}
                >
                  <span class="outlink-icon">{o.is_attachment ? "📎" : "📄"}</span>
                  <div class="outlink-info">
                    <strong>{o.target_title || o.target_raw}</strong>
                    {#if o.link_text && o.link_text !== o.target_raw}
                      <span class="bl-snippet">Texto: {o.link_text}</span>
                    {/if}
                  </div>
                </button>
              {:else}
                <div class="bl-main outlink-main unresolved">
                  <span class="outlink-icon">⚠️</span>
                  <div class="outlink-info">
                    <strong>{o.target_raw}</strong>
                    <span class="badge ghost-badge">No existe</span>
                  </div>
                </div>
                <button
                  type="button"
                  class="bl-create-ghost-btn"
                  title="Crear nota «{o.target_raw}»"
                  onclick={() => onHandleLink(o.target_raw)}
                >
                  + Crear
                </button>
              {/if}
            </div>
          {/each}
          {#if filteredOutlinks.length === 0}
            <div class="list-caption">
              {backlinkFilter.trim()
                ? "Ningún enlace saliente coincide con la búsqueda."
                : "Esta nota no contiene enlaces salientes."}
            </div>
          {/if}
        {/if}
      </div>
    {:else}
      <div class="bl-section">
        <div class="bl-heading">
          <span>Conflictos detectados</span>
          <span class="bl-count">{conflicts.length}</span>
        </div>
        {#if conflicts.length === 0}
          <div class="list-caption">No hay conflictos pendientes en este momento.</div>
        {:else}
          {#each conflicts as c (c.noteId)}
            <div class="conflict-card">
              <div class="conflict-head">
                <strong>{c.title}</strong>
                <span class="trash-meta">{c.detectedAt}</span>
              </div>
              <div class="conflict-preview">
                <div class="conflict-box">
                  <span class="conflict-label"
                    >Tu versión en editor ({c.localContent.length} car.)</span
                  >
                  <pre>{c.localContent.slice(0, 240)}{c.localContent.length > 240 ? "…" : ""}</pre>
                </div>
                <div class="conflict-box">
                  <span class="conflict-label">Versión en disco (rev {c.diskRev})</span>
                  <pre>{c.diskContent.slice(0, 240)}{c.diskContent.length > 240 ? "…" : ""}</pre>
                </div>
              </div>
              <div class="conflict-actions">
                <button
                  type="button"
                  class="trash-btn restore"
                  onclick={() => void resolveKeepLocal(c)}
                >
                  Conservar local
                </button>
                <button type="button" class="trash-btn" onclick={() => void resolveKeepDisk(c)}>
                  Usar disco
                </button>
                <button type="button" class="trash-btn" onclick={() => void resolveSaveBoth(c)}>
                  Guardar ambos
                </button>
              </div>
            </div>
          {/each}
        {/if}
      </div>
    {/if}
  </div>

  <div class="panel-footer">
    <div class="bl-tools">
      <button
        class="drawer-action"
        title="Contraer secciones"
        aria-label="Contraer secciones"
        onclick={() => setBacklinkSections(false)}
      >
        <Icon name={isDesktop ? "chevrons" : "chevrons-in"} size={16} />
      </button>
      <button
        class="drawer-action"
        title="Expandir secciones"
        aria-label="Expandir secciones"
        onclick={() => setBacklinkSections(true)}
      >
        <Icon name="chevrons" size={16} />
      </button>
      <button
        class="drawer-action"
        title="Cambiar orden"
        aria-label="Cambiar orden"
        onclick={() => (backlinkSort = backlinkSort === "az" ? "za" : "az")}
      >
        <Icon name={isDesktop ? "sort" : "sort-lines"} size={16} />
      </button>
      <button
        class="drawer-action"
        title="Filtrar"
        aria-label="Filtrar"
        onclick={handlePanelSearch}
      >
        <Icon name="search" size={16} />
      </button>
    </div>
    {#if isDesktop}
      <div class="drawer-switcher panel-switcher">
        <button class:active={panelTab === "backlinks"} onclick={() => onSetPanelTab("backlinks")}>
          <Icon name="link" size={14} /> Backlinks
        </button>
        <button class:active={panelTab === "conflicts"} onclick={() => onSetPanelTab("conflicts")}>
          <Icon name="alert" size={14} /> Conflictos
          {#if conflicts.length > 0}({conflicts.length}){/if}
        </button>
      </div>
    {:else}
      <div class="mobile-view-switcher">
        <button
          type="button"
          class="mobile-view-selector"
          aria-label="Cambiar vista del panel"
          aria-expanded={mobileViewMenuOpen}
          onclick={() => (mobileViewMenuOpen = !mobileViewMenuOpen)}
        >
          <Icon name={mobilePanelMeta.icon} size={20} />
          <span>{mobilePanelMeta.label}</span>
          <Icon name="chevrons" size={16} />
        </button>
        {#if mobileViewMenuOpen}
          <div class="mobile-view-menu">
            <button
              class:active={panelTab === "backlinks"}
              onclick={() => selectMobilePanelTab("backlinks")}
            >
              <Icon name="link" size={17} /> Backlinks
            </button>
            <button
              class:active={panelTab === "conflicts"}
              onclick={() => selectMobilePanelTab("conflicts")}
            >
              <Icon name="alert" size={17} /> Conflictos
              {#if conflicts.length > 0}({conflicts.length}){/if}
            </button>
          </div>
        {/if}
      </div>
    {/if}
    <div class="bl-stats">
      <strong>{activeTitle || "Sin nota"}</strong>
      <span>{stats.words} palabras · {stats.chars} caracteres</span>
    </div>
  </div>
</aside>
