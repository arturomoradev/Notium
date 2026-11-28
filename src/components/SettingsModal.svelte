<script lang="ts">
  import { untrack, onMount } from "svelte";
  import { pushBackHandler } from "../lib/backStack";
  import { fade } from "svelte/transition";
  import { call } from "../lib/ipc/commands";
  import { listen } from "@tauri-apps/api/event";
  import Icon from "./editors/Icon.svelte";
  import AttachmentsModal from "./AttachmentsModal.svelte";
  import TrashModal from "./TrashModal.svelte";
  import ToolbarConfig from "./editors/ToolbarConfig.svelte";
  import EditorColorsSettings from "./EditorColorsSettings.svelte";
  import type { ImageQuality, SettingsDto, SyncStatusDto } from "../lib/ipc/types";

  type SettingsPage = "general" | "colors" | "toolbar" | "sync" | "attachments" | "trash" | "snippets";
  type SettingsTab = "home" | SettingsPage;
  type SnippetSection = "shortcuts" | "callouts" | "markdown";

  interface Props {
    settings: SettingsDto;
    initialTab?: SettingsPage;
    onSave: (settings: SettingsDto) => void;
    onOpenNote: (noteId: string) => void;
    onCreateDemoNotes?: () => void;
    onClose: () => void;
  }

  let {
    settings,
    initialTab = "general",
    onSave,
    onOpenNote,
    onCreateDemoNotes,
    onClose,
  }: Props = $props();

  const settingsOptions: Array<{
    id: SettingsPage;
    icon: string;
    label: string;
    description: string;
  }> = [
    {
      id: "general",
      icon: "settings",
      label: "General",
      description: "Editor, apariencia y mantenimiento del Vault",
    },
    {
      id: "colors",
      icon: "palette",
      label: "Colores del editor",
      description: "Colores de títulos, negrita, cursiva, fondo y CSS propio",
    },
    {
      id: "toolbar",
      icon: "wrench",
      label: "Barra de herramientas",
      description: "Elegir y ordenar los comandos de la barra del editor",
    },
    {
      id: "sync",
      icon: "cloud",
      label: "Cloud Sync",
      description: "Servidor, sincronización y estado del dispositivo",
    },
    {
      id: "attachments",
      icon: "attachment",
      label: "Adjuntos",
      description: "Imágenes, documentos y archivos huérfanos",
    },
    {
      id: "trash",
      icon: "trash",
      label: "Papelera",
      description: "Restaurar notas o eliminarlas permanentemente",
    },
    {
      id: "snippets",
      icon: "book",
      label: "Snippets y Guía",
      description: "Markdown, callouts y atajos de teclado",
    },
  ];

  const reduceMotion =
    typeof window !== "undefined" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  let root: HTMLDivElement;
  let activeTab = $state<SettingsTab>(
    untrack(() => (initialTab === "general" ? "home" : initialTab)),
  );
  let settingsQuery = $state("");
  const filteredSettingsOptions = $derived(
    settingsOptions.filter((option) => {
      const query = settingsQuery.trim().toLocaleLowerCase("es");
      return (
        !query || `${option.label} ${option.description}`.toLocaleLowerCase("es").includes(query)
      );
    }),
  );
  const activeTitle = $derived(
    settingsOptions.find((option) => option.id === activeTab)?.label ?? "Ajustes",
  );
  let snippetSection = $state<SnippetSection>("callouts");
  let copiedKey = $state("");
  let rebuildingIndex = $state(false);
  let rebuildStatus = $state("");

  // Sync state
  let syncStatus = $state<SyncStatusDto | null>(null);
  let serverUrl = $state("");
  let authToken = $state("");
  let deviceId = $state("");
  let syncVaultId = $state("default");
  let syncEnabled = $state(false);
  let autoSync = $state(true);
  let intervalSecs = $state(20);
  let savingSync = $state(false);
  let syncActionMsg = $state("");
  let isSyncingNow = $state(false);

  async function loadSync(): Promise<void> {
    try {
      const st = await call("get_sync_status");
      syncStatus = st;
      serverUrl = st.server_url;
      deviceId = st.device_id;
      syncVaultId = st.vault_id || "default";
      syncEnabled = st.enabled;
    } catch (e) {
      console.warn("Failed to get sync status", e);
    }
  }

  onMount(() => {
    void loadSync();
    const unlisten = listen<SyncStatusDto>("sync://status-changed", (event) => {
      syncStatus = event.payload;
      serverUrl = event.payload.server_url;
      deviceId = event.payload.device_id;
      syncVaultId = event.payload.vault_id || "default";
      syncEnabled = event.payload.enabled;
    });
    return () => {
      void unlisten.then((u) => u());
    };
  });

  async function saveSync(): Promise<void> {
    savingSync = true;
    syncActionMsg = "";
    try {
      if (serverUrl.trim().length > 0 && (autoSync || syncEnabled)) {
        syncEnabled = true;
      }
      const updated = await call("configure_sync_server", {
        config: {
          server_url: serverUrl,
          auth_token: authToken,
          device_id: deviceId,
          vault_id: syncVaultId.trim() || "default",
          enabled: syncEnabled,
          auto_sync: autoSync,
          interval_secs: intervalSecs,
        },
      });
      syncStatus = updated;
      syncVaultId = updated.vault_id || "default";
      syncActionMsg = "✓ Configuración de sincronización guardada.";
    } catch (e) {
      syncActionMsg = `Error al guardar configuración: ${String(e)}`;
    } finally {
      savingSync = false;
    }
  }

  async function triggerSyncNow(): Promise<void> {
    isSyncingNow = true;
    syncActionMsg = "";
    try {
      const res = await call("sync_now");
      syncStatus = res;
      if (res.state === "error") {
        syncActionMsg = `Error: ${res.last_error || "Fallo al sincronizar"}`;
      } else {
        syncActionMsg = `✓ Sincronización completada con éxito.`;
      }
    } catch (e) {
      syncActionMsg = `Error de sincronización: ${String(e)}`;
    } finally {
      isSyncingNow = false;
    }
  }

  async function togglePauseSync(): Promise<void> {
    try {
      if (syncStatus?.state === "paused") {
        syncStatus = await call("resume_sync");
      } else {
        syncStatus = await call("pause_sync");
      }
    } catch (e) {
      syncActionMsg = `Error: ${String(e)}`;
    }
  }

  async function rebuildIndex(): Promise<void> {
    rebuildingIndex = true;
    rebuildStatus = "";
    try {
      const res = await call("vault_rebuild_index");
      rebuildStatus = `✓ Índice reconstruido: ${res.scanned} notas analizadas (${res.updated} actualizadas, ${res.adopted} nuevas).`;
    } catch (e) {
      rebuildStatus = `Error al reconstruir índice: ${String(e)}`;
    } finally {
      rebuildingIndex = false;
    }
  }

  let sourceMode = $state(untrack(() => settings.source_mode));
  let theme = $state<"dark" | "light" | "system">(
    untrack(() =>
      settings.theme === "light" || settings.theme === "system" ? settings.theme : "dark",
    ),
  );
  let fontSize = $state<number>(untrack(() => settings.font_size ?? 15));
  let imageQuality = $state<ImageQuality>(untrack(() => settings.image_quality ?? "1080p"));
  const imageQualities: ImageQuality[] = ["720p", "1080p", "1440p"];

  const shortcutsList = [
    { keys: "Ctrl + K / Cmd + K", desc: "Abrir paleta rápida para buscar y cambiar de nota" },
    { keys: "[[", desc: "Abrir autocompletado para enlazar otra nota" },
    { keys: "[[#", desc: "Autocompletar enlace a una sección (# Encabezado) de la nota actual" },
    { keys: "[[Nota#", desc: "Autocompletar enlace a una sección específica de otra nota" },
    { keys: "![[Nota]]", desc: "Incrustar vista previa en vivo de otra nota o imagen adjunta" },
    { keys: "#etiqueta", desc: "Autocompletar e indexar una etiqueta en la nota" },
    {
      keys: "Tab / Shift + Tab",
      desc: "Indentar / desindentar listas o salir de un enlace [texto](url)",
    },
    { keys: "Ctrl + Z / Ctrl + Y", desc: "Deshacer / Rehacer cambios en el editor" },
    { keys: "Esc", desc: "Cerrar menús de autocompletado o ventanas modales" },
    { keys: "Deslizar → (Móvil)", desc: "Abrir panel izquierdo de Archivos, Búsqueda y Tags" },
    { keys: "Deslizar ← (Móvil)", desc: "Abrir panel derecho de Backlinks y Conflictos" },
  ];

  const calloutsList = [
    {
      title: "Nota ([!note])",
      desc: "Apuntes generales o contexto adicional.",
      code: "> [!note] Título de la nota\n> Contenido del callout.",
    },
    {
      title: "Información ([!info])",
      desc: "Datos objetivos, referencias o estado.",
      code: "> [!info] Información\n> Detalle informativo importante.",
    },
    {
      title: "Consejo / Sugerencia ([!tip] / [!hint])",
      desc: "Trucos, recomendaciones o buenas prácticas.",
      code: "> [!tip] Consejo útil\n> Usa [[#Sección]] para saltar a un encabezado.",
    },
    {
      title: "Importante ([!important])",
      desc: "Puntos clave o requisitos esenciales.",
      code: "> [!important] Punto clave\n> No olvides revisar esta sección.",
    },
    {
      title: "Éxito / Verificado ([!success] / [!check])",
      desc: "Confirmación de pasos completados.",
      code: "> [!success] Completado\n> La validación pasó sin errores.",
    },
    {
      title: "Advertencia ([!warning] / [!caution])",
      desc: "Precauciones o avisos importantes.",
      code: "> [!warning] Atención\n> Verifica los datos antes de continuar.",
    },
    {
      title: "Peligro / Error ([!danger] / [!error] / [!bug])",
      desc: "Acciones irreversibles o errores críticos.",
      code: "> [!danger] Acción irreversible\n> Esta operación no se puede deshacer.",
    },
    {
      title: "Pregunta / FAQ ([!question] / [!faq])",
      desc: "Dudas frecuentes o preguntas de investigación.",
      code: "> [!question] ¿Cómo enlazo una sección?\n> Escribe [[#Nombre del encabezado]].",
    },
    {
      title: "Ejemplo ([!example])",
      desc: "Casos prácticos o muestras de uso.",
      code: "> [!example] Caso práctico\n> Ejemplo aplicado paso a paso.",
    },
    {
      title: "Cita ([!quote])",
      desc: "Citas textuales o referencias bibliográficas.",
      code: "> [!quote] Cita destacada\n> «El conocimiento conectado multiplica su valor.»",
    },
    {
      title: "Callout Plegable ([!note]-)",
      desc: "Añade '-' tras el corchete para poder contraer/expandir el bloque.",
      code: "> [!note]- Pulsa para desplegar\n> Contenido oculto inicialmente.",
    },
    {
      title: "Enlaces a Notas y Secciones (Wikilinks)",
      desc: "Conecta notas completas, secciones internas o secciones de otras notas.",
      code: "[[Nombre de Nota]]\n[[Nombre de Nota|Texto personalizado]]\n[[#Encabezado en esta nota]]\n[[Otra Nota#Encabezado en otra nota]]",
    },
    {
      title: "Incrustación / Transclusión (![[...]])",
      desc: "Muestra el contenido de otra nota (o sección) dentro de la nota actual.",
      code: "![[Otra Nota]]\n![[Otra Nota#Sección Específica]]",
    },
    {
      title: "Matemáticas LaTeX ($...$ y $$...$$)",
      desc: "Fórmulas matemáticas en línea o en bloque.",
      code: "Inline: $E = mc^2$ y $\\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}$\n\n$$\n\\sum_{i=1}^{n} i^2 = \\frac{n(n+1)(2n+1)}{6}\n$$",
    },
    {
      title: "Diagramas Mermaid (Flowchart y Secuencia)",
      desc: "Genera diagramas vectoriales SVG directamente desde código.",
      code: '```mermaid\nflowchart LR\n  A["Idea"] --> B["Nota en Notium"]\n  B --> C["Conocimiento Conectado"]\n```',
    },
  ];

  const markdownList = [
    {
      title: "Encabezados (H1 – H6)",
      desc: "Estructuran el documento y crean secciones enlazables mediante [[#Título]].",
      code: "# Encabezado 1\n## Encabezado 2\n### Encabezado 3\n#### Encabezado 4",
    },
    {
      title: "Énfasis y Estilos en Línea",
      desc: "Negrita, cursiva, tachado, resaltado y código inline.",
      code: "**Texto en negrita**\n*Texto en cursiva*\n~~Texto tachado~~\n==Texto resaltado==\n`código en línea`",
    },
    {
      title: "Listas de Viñetas y Numeradas",
      desc: "Usa '-', '*' o números seguidos de punto. Usa Tab para anidar.",
      code: "- Elemento uno\n  - Subelemento\n1. Primer paso\n2. Segundo paso",
    },
    {
      title: "Listas de Tareas (Checklists)",
      desc: "Casillas interactivas que puedes marcar con un clic.",
      code: "- [ ] Tarea pendiente\n- [x] Tarea completada",
    },
    {
      title: "Tablas Markdown",
      desc: "Se transforman automáticamente en tablas estilizadas.",
      code: "| Columna A | Columna B | Columna C |\n| --- | --- | --- |\n| Dato 1 | Dato 2 | Dato 3 |",
    },
    {
      title: "Bloques de Código",
      desc: "Triple comilla invertida seguida del nombre del lenguaje.",
      code: '```typescript\nconst app = "Notium";\nconsole.log(app);\n```',
    },
    {
      title: "Citas, Enlaces Externos y Separadores",
      desc: "Citas con '>', enlaces web y líneas horizontales con '---'.",
      code: "> Cita en bloque\n\n[Sitio web](https://example.com)\n\n---",
    },
    {
      title: "Etiquetas (#tags)",
      desc: "Escribe # seguido de una palabra (fuera de corchetes) para etiquetar la nota.",
      code: "#proyecto #universidad/calculo #importante",
    },
  ];

  function copySnippet(code: string, key: string): void {
    void navigator.clipboard?.writeText(code);
    copiedKey = key;
    setTimeout(() => {
      if (copiedKey === key) copiedKey = "";
    }, 1500);
  }

  function save(): void {
    const size = Math.min(32, Math.max(10, Math.round(fontSize || 15)));
    onSave({ source_mode: sourceMode, theme, font_size: size, image_quality: imageQuality });
  }

  function openSettingsPage(page: SettingsPage): void {
    settingsQuery = "";
    activeTab = page;
  }

  function showSettingsHome(): void {
    settingsQuery = "";
    activeTab = "home";
  }

  function onDocMousedown(e: MouseEvent): void {
    if (root && !root.contains(e.target as Node)) onClose();
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") onClose();
  }

  // Drag-to-dismiss from the grabber/header (the body keeps native scrolling).
  let dragY = $state(0);
  let dragging = $state(false);
  let dragStartY = 0;
  let dragStartT = 0;
  let dragPointer: number | null = null;

  function onDragStart(e: PointerEvent): void {
    if (e.pointerType === "mouse" && e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button")) return;
    dragPointer = e.pointerId;
    dragStartY = e.clientY;
    dragStartT = e.timeStamp;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onDragMove(e: PointerEvent): void {
    if (e.pointerId !== dragPointer) return;
    const dy = e.clientY - dragStartY;
    if (!dragging && dy < 6) return;
    dragging = true;
    dragY = Math.max(0, dy);
  }

  function onDragEnd(e: PointerEvent): void {
    if (e.pointerId !== dragPointer) return;
    dragPointer = null;
    if (!dragging) return;
    dragging = false;
    const velocity = dragY / Math.max(1, e.timeStamp - dragStartT);
    if (dragY > root.offsetHeight * 0.3 || (dragY > 40 && velocity > 0.5)) {
      dragY = root.offsetHeight;
      setTimeout(onClose, reduceMotion ? 0 : 180);
    } else {
      dragY = 0;
    }
  }

  onMount(() =>
    pushBackHandler(() => {
      if (activeTab === "home") onClose();
      else showSettingsHome();
      return true;
    }),
  );
</script>

<svelte:window onmousedown={onDocMousedown} onkeydown={onKeydown} />

<div class="modal-overlay settings-overlay" transition:fade={{ duration: reduceMotion ? 0 : 160 }}>
  <div
    class="modal settings-modal"
    class:settings-sheet={true}
    class:dragging
    style:translate={dragY ? `0 ${dragY}px` : null}
    bind:this={root}
    role="dialog"
    aria-modal="true"
    aria-label={activeTitle}
    transition:fade={{ duration: reduceMotion ? 0 : 200 }}
  >
    <div
      class="settings-drag-zone"
      role="presentation"
      onpointerdown={onDragStart}
      onpointermove={onDragMove}
      onpointerup={onDragEnd}
      onpointercancel={onDragEnd}
    >
      <div class="settings-grabber" aria-hidden="true"></div>
      <div class="modal-header settings-header">
        {#if activeTab === "home"}
          <span class="settings-header-spacer" aria-hidden="true"></span>
        {:else}
          <button
            class="icon-btn settings-back"
            onclick={showSettingsHome}
            aria-label="Volver a Ajustes"
          >
            <Icon name="chevron-right" size={18} />
          </button>
        {/if}
        <h2>{activeTitle}</h2>
        <button class="icon-btn settings-close" onclick={onClose} aria-label="Cerrar">✕</button>
      </div>
    </div>

    <div
      class="modal-body settings-body"
      class:settings-body-embedded={activeTab === "attachments" ||
        activeTab === "trash" ||
        activeTab === "toolbar"}
    >
      {#if activeTab === "home"}
        <div class="settings-home">
          <label class="settings-search">
            <Icon name="search" size={18} />
            <input
              type="search"
              placeholder="Buscar ajustes…"
              aria-label="Buscar ajustes"
              autocomplete="off"
              bind:value={settingsQuery}
            />
          </label>

          <div class="settings-section-label">Opciones</div>
          <div class="settings-option-list">
            {#each filteredSettingsOptions as option (option.id)}
              <button
                type="button"
                class="settings-option"
                onclick={() => openSettingsPage(option.id)}
              >
                <span class="settings-option-icon"><Icon name={option.icon} size={19} /></span>
                <span class="settings-option-copy">
                  <strong>{option.label}</strong>
                  <span>{option.description}</span>
                </span>
                <span class="settings-option-chevron"><Icon name="chevron-right" size={16} /></span>
              </button>
            {/each}
            {#if filteredSettingsOptions.length === 0}
              <div class="settings-no-results">No hay ajustes que coincidan con la búsqueda.</div>
            {/if}
          </div>
        </div>
      {:else if activeTab === "general"}
        <div class="setting">
          <div class="setting-label">
            <strong>Modo código puro en Markdown (Source Mode)</strong>
            <span class="setting-hint"
              >El modo editable muestra el Markdown sin procesar en lugar de la Vista en vivo.</span
            >
          </div>
          <button
            type="button"
            class="switch"
            class:on={sourceMode}
            role="switch"
            aria-checked={sourceMode}
            aria-label="Modo código puro en Markdown"
            onclick={() => (sourceMode = !sourceMode)}
          >
            <span class="knob"></span>
          </button>
        </div>

        <div class="setting">
          <div class="setting-label">
            <strong>Tema visual</strong>
            <span class="setting-hint"
              >Modo oscuro, claro o el mismo que el dispositivo (ajusta también el color del
              cursor).</span
            >
          </div>
          <div class="seg">
            <button
              type="button"
              class:active={theme === "system"}
              onclick={() => (theme = "system")}
            >
              Sistema
            </button>
            <button type="button" class:active={theme === "dark"} onclick={() => (theme = "dark")}>
              Oscuro
            </button>
            <button
              type="button"
              class:active={theme === "light"}
              onclick={() => (theme = "light")}
            >
              Claro
            </button>
          </div>
        </div>

        <div class="setting">
          <div class="setting-label">
            <strong>Compresión de imágenes</strong>
            <span class="setting-hint"
              >Las imágenes nuevas (cámara, galería o archivos) se reducen a esta resolución al
              adjuntarlas.</span
            >
          </div>
          <div class="seg">
            {#each imageQualities as q (q)}
              <button
                type="button"
                class:active={imageQuality === q}
                onclick={() => (imageQuality = q)}
              >
                {q}
              </button>
            {/each}
          </div>
        </div>

        <div class="setting">
          <div class="setting-label">
            <strong>Tamaño de fuente</strong>
            <span class="setting-hint">Tamaño del texto de las notas en píxeles (10–32).</span>
          </div>
          <input
            type="number"
            class="sync-input"
            style="width: 90px;"
            min="10"
            max="32"
            step="1"
            inputmode="numeric"
            bind:value={fontSize}
          />
        </div>

        <div class="setting">
          <div class="setting-label">
            <strong>Mantenimiento del Vault</strong>
            <span class="setting-hint">
              Reescanea todos los archivos Markdown en disco y reconstruye el índice SQLite, los
              enlaces ([[wikilinks]]), etiquetas (#tags) y la base de datos de búsqueda FTS5.
            </span>
          </div>
          <button
            type="button"
            class="trash-btn"
            disabled={rebuildingIndex}
            onclick={() => void rebuildIndex()}
          >
            {#if rebuildingIndex}
              Reconstruyendo…
            {:else}
              Reconstruir índice
            {/if}
          </button>
        </div>
        {#if rebuildStatus}
          <div
            class="setting-hint"
            style="color: var(--accent); margin-top: -6px; margin-bottom: 6px;"
          >
            {rebuildStatus}
          </div>
        {/if}

        {#if onCreateDemoNotes}
          <div class="setting">
            <div class="setting-label">
              <strong>Notas de ejemplo y demostración</strong>
              <span class="setting-hint">
                Genera o restaura la nota «Guía Visual y Ejemplos de Notium» y las notas de prueba
                «Lorem Ipsum - Parte 1 y 2» con todas las marcaciones, callouts y secciones
                vinculadas.
              </span>
            </div>
            <button
              type="button"
              class="trash-btn restore"
              onclick={() => {
                onCreateDemoNotes();
                onClose();
              }}
            >
              Abrir / Restaurar notas de ejemplo
            </button>
          </div>
        {/if}
      {:else if activeTab === "sync"}
        <div class="sync-tab-container">
          <!-- Status Banner Card -->
          <div
            class="sync-status-card"
            class:syncing={syncStatus?.state === "syncing" || isSyncingNow}
            class:error={syncStatus?.state === "error"}
            class:synced={syncStatus?.state === "synced"}
          >
            <div class="sync-status-header">
              <div class="sync-status-title">
                <Icon
                  name={syncStatus?.state === "synced"
                    ? "check"
                    : syncStatus?.state === "syncing" || isSyncingNow
                      ? "sync"
                      : syncStatus?.state === "error"
                        ? "alert"
                        : "cloud"}
                  size={20}
                />
                <div>
                  <strong>
                    {#if syncStatus?.state === "synced"}
                      Sincronizado
                    {:else if syncStatus?.state === "syncing" || isSyncingNow}
                      Sincronizando…
                    {:else if syncStatus?.state === "paused"}
                      Sincronización pausada
                    {:else if syncStatus?.state === "error"}
                      Error de sincronización
                    {:else}
                      Sincronización desactivada
                    {/if}
                  </strong>
                  <div class="setting-hint">
                    {#if syncStatus?.last_synced_at}
                      Última sincronización: {new Date(
                        syncStatus.last_synced_at * 1000,
                      ).toLocaleTimeString()} · {syncStatus.pending_ops} cambios pendientes
                    {:else}
                      {syncStatus?.pending_ops || 0} cambios locales pendientes en el journal HLC
                    {/if}
                  </div>
                </div>
              </div>

              <div class="sync-header-actions">
                <button
                  type="button"
                  class="trash-btn"
                  disabled={!syncEnabled || isSyncingNow}
                  onclick={() => void triggerSyncNow()}
                >
                  <Icon name="sync" size={14} />
                  {isSyncingNow ? "Sincronizando…" : "Sincronizar ahora"}
                </button>
                {#if syncEnabled}
                  <button type="button" class="trash-btn" onclick={() => void togglePauseSync()}>
                    {syncStatus?.state === "paused" ? "Reanudar" : "Pausar"}
                  </button>
                {/if}
              </div>
            </div>

            {#if syncStatus?.last_error}
              <div class="sync-error-banner">
                <Icon name="alert" size={16} />
                <span>{syncStatus.last_error}</span>
              </div>
            {/if}
          </div>

          <!-- Settings Form -->
          <div class="setting">
            <div class="setting-label">
              <strong>Activar Sincronización en la Nube</strong>
              <span class="setting-hint">
                Conecta este Vault al servidor Notium Cloud Sync para sincronizar notas, revisiones
                y adjuntos en tiempo real.
              </span>
            </div>
            <button
              type="button"
              class="switch"
              class:on={syncEnabled}
              role="switch"
              aria-checked={syncEnabled}
              aria-label="Activar sincronización"
              onclick={() => (syncEnabled = !syncEnabled)}
            >
              <span class="knob"></span>
            </button>
          </div>

          <div class="sync-form-group">
            <label class="sync-field-label" for="sync-server-url">
              <strong>URL del Servidor de Sincronización</strong>
              <span class="setting-hint"
                >Dirección de tu servidor (ej. https://sync.notium.app o http://192.168.1.100:8000).</span
              >
            </label>
            <input
              id="sync-server-url"
              type="url"
              class="sync-input"
              placeholder="https://sync.ejemplo.com"
              bind:value={serverUrl}
            />
          </div>

          <div class="sync-form-group">
            <label class="sync-field-label" for="sync-auth-token">
              <strong>Token de Autenticación / API Key (opcional)</strong>
              <span class="setting-hint"
                >Token enviado en la cabecera Authorization: Bearer para proteger el acceso.</span
              >
            </label>
            <input
              id="sync-auth-token"
              type="password"
              class="sync-input"
              placeholder="Token secreto o clave de acceso"
              bind:value={authToken}
            />
          </div>

          <div class="sync-form-group">
            <label class="sync-field-label" for="sync-vault-id">
              <strong>ID del Vault (partición en D1)</strong>
              <span class="setting-hint"
                >Identificador [a-z0-9_-] en Cloudflare D1 para aislar este Vault de otros (por
                defecto: default).</span
              >
            </label>
            <input
              id="sync-vault-id"
              type="text"
              class="sync-input"
              placeholder="default"
              bind:value={syncVaultId}
            />
          </div>

          <div class="sync-form-group">
            <label class="sync-field-label" for="sync-device-id">
              <strong>ID de este Dispositivo</strong>
              <span class="setting-hint"
                >Identificador único en el journal HLC para resolución determinista de conflictos.</span
              >
            </label>
            <div class="sync-input-row">
              <input
                id="sync-device-id"
                type="text"
                class="sync-input mono"
                value={deviceId}
                readonly
              />
            </div>
          </div>

          <div class="setting">
            <div class="setting-label">
              <strong>Sincronización periódica automática</strong>
              <span class="setting-hint"
                >Comprueba y envía cambios en segundo plano adaptativo y al volver a la app.</span
              >
            </div>
            <button
              type="button"
              class="switch"
              class:on={autoSync}
              role="switch"
              aria-checked={autoSync}
              aria-label="Sincronización automática"
              onclick={() => (autoSync = !autoSync)}
            >
              <span class="knob"></span>
            </button>
          </div>

          {#if syncActionMsg}
            <div class="setting-hint" style="color: var(--accent); margin-top: 4px;">
              {syncActionMsg}
            </div>
          {/if}

          <div class="sync-save-row">
            <button
              type="button"
              class="primary"
              disabled={savingSync}
              onclick={() => void saveSync()}
            >
              {savingSync ? "Guardando…" : "Guardar configuración"}
            </button>
          </div>
        </div>
      {:else if activeTab === "attachments"}
        <AttachmentsModal embedded={true} {onOpenNote} {onClose} />
      {:else if activeTab === "colors"}
        <EditorColorsSettings />
      {:else if activeTab === "toolbar"}
        <ToolbarConfig embedded={true} {onClose} />
      {:else if activeTab === "trash"}
        <TrashModal embedded={true} onRestored={(note) => onOpenNote(note.note_id)} {onClose} />
      {:else}
        <div class="snippets-subnav">
          <button
            type="button"
            class:active={snippetSection === "callouts"}
            onclick={() => (snippetSection = "callouts")}
          >
            Callouts y Anotaciones
          </button>
          <button
            type="button"
            class:active={snippetSection === "markdown"}
            onclick={() => (snippetSection = "markdown")}
          >
            Marcaciones Markdown
          </button>
          <button
            type="button"
            class:active={snippetSection === "shortcuts"}
            onclick={() => (snippetSection = "shortcuts")}
          >
            Atajos de Teclado
          </button>
        </div>

        {#if snippetSection === "shortcuts"}
          <div class="snippets-list">
            {#each shortcutsList as item (item.keys)}
              <div class="shortcut-row">
                <kbd class="shortcut-kbd">{item.keys}</kbd>
                <span class="shortcut-desc">{item.desc}</span>
              </div>
            {/each}
          </div>
        {:else if snippetSection === "callouts"}
          <div class="snippets-list">
            {#each calloutsList as item (item.title)}
              <div class="snippet-card">
                <div class="snippet-card-head">
                  <div>
                    <strong>{item.title}</strong>
                    <div class="setting-hint">{item.desc}</div>
                  </div>
                  <button
                    type="button"
                    class="trash-btn"
                    onclick={() => copySnippet(item.code, item.title)}
                  >
                    {copiedKey === item.title ? "✓ Copiado" : "Copiar"}
                  </button>
                </div>
                <pre class="snippet-pre">{item.code}</pre>
              </div>
            {/each}
          </div>
        {:else}
          <div class="snippets-list">
            {#each markdownList as item (item.title)}
              <div class="snippet-card">
                <div class="snippet-card-head">
                  <div>
                    <strong>{item.title}</strong>
                    <div class="setting-hint">{item.desc}</div>
                  </div>
                  <button
                    type="button"
                    class="trash-btn"
                    onclick={() => copySnippet(item.code, item.title)}
                  >
                    {copiedKey === item.title ? "✓ Copiado" : "Copiar"}
                  </button>
                </div>
                <pre class="snippet-pre">{item.code}</pre>
              </div>
            {/each}
          </div>
        {/if}
      {/if}
    </div>

    {#if activeTab === "general" || activeTab === "sync" || activeTab === "snippets"}
      <div class="modal-footer">
        {#if activeTab === "general"}
          <button class="primary" onclick={save}>Guardar</button>
        {:else}
          <button class="primary" onclick={onClose}>Cerrar</button>
        {/if}
      </div>
    {/if}
  </div>
</div>
