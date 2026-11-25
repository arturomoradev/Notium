# Hoja de Ruta y Backlog Técnico — Notium 1.0 (`docs/ROADMAP_AND_BACKLOG.md`)

Este documento sirve como referencia viva para todos los agentes y desarrolladores sobre qué sigue en la planificación de **Notium 1.0** y qué tareas técnicas, correcciones y mejoras de UX están en curso o pendientes.

---

## 1. Planificación por Fases (Roadmap)

### Fase 1 — Cierre de MVP Local-First (✅ COMPLETADA — Cerrada formalmente)
- Editor Markdown estilo Obsidian (CodeMirror 6) con Live Preview, bloques LaTeX/Mermaid (renderizado con Pele, todos los tipos de diagrama)/Callouts/Tablas/Código, transclusión `![[Nota]]`, enlaces a secciones `[[Nota#Sección]]`, autocompletado de notas/secciones/etiquetas.
- Gestión multi-vault (creación en primer arranque, cambio de Vault, borrado con confirmación, exportación/importación `.notium.json` con selector nativo y barra de progreso).
- Papelera con temporizador individual de 30 días por nota y auto-purga.
- Resolución de conflictos integrada en el panel lateral derecho.
- Tema oscuro/claro adaptativo y guía interactiva de Snippets en Ajustes.
- **Cierre formal completado:**
  - Verificación interactiva completa en hardware físico (`P1`–`P15`).
  - Acta de entrega definitiva generada: [`evidence/PHASE_1_SIGN_OFF.md`](evidence/PHASE_1_SIGN_OFF.md).

### Fase 1.5 — Pulido del Grafo de Enlaces y Sincronización Cloud v1 (✅ COMPLETADA)
1. **Gestor de Adjuntos (Attachments Browser — IMPLEMENTADO):**
   - Vista/modal para inspeccionar adjuntos del Vault, previsualizar imágenes/documentos con miniaturas, ver qué notas los referencian (con salto directo a la nota), copiar sintaxis `![[adjunto]]`, detectar adjuntos huérfanos y eliminarlos de forma individual o en lote (`attachment_list`, `attachment_delete`, `attachment_delete_orphans`). Acceso desde ribbon desktop, menú contextual y footer móvil.
   - **Rendimiento de imágenes (IMPLEMENTADO):** el editor muestra los adjuntos con Blob URLs (antes data URLs en base64), la lectura del archivo se hace fuera del lock del vault y las imágenes nuevas (cámara, galería o archivos) se reducen a WebP según el ajuste "Compresión de imágenes" (720p / 1080p / 1440p, por defecto 1080p). GIF y SVG se suben sin cambios.
2. **UX de Enlaces No Resueltos y Reparación de Enlaces (Link Repair — IMPLEMENTADO):**
   - Listar enlaces salientes (`note_get_outlinks`) y enlaces no resueltos (fantasmas) en el panel lateral de Enlaces y Backlinks, con botón `+ Crear nota` para crearlas instantáneamente.
   - Asistente de actualización de enlaces (`note_check_link_references`, `note_repair_links`): al renombrar una nota, detecta si otras notas la enlazan y ofrece actualizar automáticamente el texto de los `[[Enlaces]]` en todos los archivos Markdown o mantener la resolución por alias.
3. **Transporte de Sincronización en la Nube (Cloud Sync v1 — IMPLEMENTADO):**
   - Conexión del motor local (`SyncDb`, journal de operaciones HLC, revisiones y merge de 3 vías en `crates/notium-core/src/sync/mod.rs`) con transporte de red HTTP REST/JSON y blobs binarios (`POST /sync/push`, `GET /sync/pull`, `POST /sync/blob/<hash>`, `GET /sync/blob/<hash>`).
   - Sincronización en segundo plano adaptativa (polling periódico, refresco inmediato en guardado/creación/eliminación y reanudación en Android) con canal de eventos Tauri (`sync://status-changed` y `vault://reconciled`).
   - Comandos IPC completos: `get_sync_status`, `sync_now`, `configure_sync_server`, `pause_sync`, `resume_sync`.
   - Pestaña de configuración de sincronización en `SettingsModal.svelte` con formulario completo (servidor, token, device ID, switches, log de estado y botones de acción manual).
   - Indicador visual reactivo en tiempo real con icono de nube (Synced, Syncing…, Offline, Error, Pausado) en cabecera desktop/móvil, barra de acciones desktop y footers.
   - **Consolidación 100 % Cloudflare Workers + D1 (`notium-db`) sin R2 ni Durable Objects:**
     - Almacenamiento unificado en D1 tanto para operaciones de notas (`journal`) como para adjuntos binarios (`blobs`, con límite de `MAX_D1_BLOB_BYTES = 900_000` bytes).
     - Seguimiento local de adjuntos subidos en `.app/sync.db` (`synced_blobs`) para evitar la resubida redundante cada 20 s, y descarga automática multi-dispositivo verificando integridad criptográfica `BLAKE3` y prevención de path traversal.
     - Aislamiento multi-vault mediante columna `vault_id` (por defecto `"default"`) en D1 y campo configurable en Ajustes de Cloud Sync.
4. **Notium Web (IMPLEMENTADO — pendiente de desplegar; ver [`docs/WEB.md`](WEB.md)):**
   - La misma UI en el navegador, servida por el Worker `notium-sync` (`dist-web/`), con login por token, caché opcional en IndexedDB y sincronización contra D1 como un dispositivo más. Build separada (`web/`, `npm run build:web`): la app nativa no incluye nada de `src/web`.
   - `notium-core` compilado a WebAssembly (`crates/notium-wasm`, feature `native` por defecto en el core): títulos, etiquetas, enlaces, BLAKE3, frontmatter y rutas usan el mismo código Rust.
   - Funciones web: editar, crear, borrar, buscar, sugerencias `[[`, backlinks/outlinks/menciones, reparar enlaces, ver y subir adjuntos (≤ 900 KB), renombrar/mover notas y carpetas.
   - **Sync de rutas (también nativo):** las ops llevan un `path` opcional al crear, renombrar o mover; gana el cambio más reciente por HLC. Antes, renombrar o mover en el PC no llegaba al móvil.
   - Worker: columna opcional `journal.path` y corrección de la paginación de `/sync/pull` (no salta ops que comparten `ts` al final de una página llena).
   - Desplegado (2026-11-20): la web es el Worker `notium-web` (detrás de Cloudflare Access) y reenvía `/sync/*` a `notium-sync` por *service binding*; `notium-sync` exige `AUTH_TOKEN`. Fuera de alcance por ahora: papelera/restaurar entre dispositivos, borrar carpetas desde la web, varios vaults, alias en la resolución de enlaces.
   - **Papelera compartida (IMPLEMENTADO):** borrar, restaurar y eliminar definitivamente se sincronizan entre PC, móvil y web; a los 30 días la nota desaparece de todas las papeleras y el cron del Worker borra su historial en D1.
   - **R2 retirado (2026-11-20):** el bucket antiguo `notium-attachments` se descarta (contenido no importante) con una regla de ciclo de vida que expira todo en 1 día; queda borrar el bucket vacío (`npx wrangler r2 bucket delete notium-attachments`). Los adjuntos viven solo en D1 (≤ 900 KB).

### Fase 2 — Inteligencia Artificial (Gemini RAG) y Almacenamiento Avanzado
1. **Pipeline RAG con Gemini / Embeddings Locales:**
   - Aprovechar la tabla `chunks` (ya poblada con IDs deterministas `blake3` en cada guardado) para generar embeddings (Gemini Embedding API o modelo local).
   - Búsqueda híbrida (FTS5 léxica + vectorial semántica + filtros de etiquetas) y asistente conversacional con citas navegables (`note://<note_id>#<chunk_id>`).
2. **Vista de Grafo (Graph View):**
   - Visualización interactiva del grafo de notas y enlaces a partir de la tabla `links`.
3. **Edición en vivo sobre carpetas SAF en Android (`SafVaultProvider`):**
   - Permitir operar directamente sobre un árbol de documentos SAF en Android además del almacenamiento rápido privado de la app.

---

## 2. Correcciones y Nuevas Capacidades Prioritarias (Apartado A — En ejecución)

1. **[x] Restauración de Papelera conservando la subcarpeta original:**
   - Codificar la ruta relativa de la subcarpeta en el archivo de `.app/trash/` al eliminar (`delete_to_trash`) y restaurar la nota en su subcarpeta original (`restore_from_trash`).
2. **[x] Sanitización de consultas FTS5 y soporte Unicode en etiquetas:**
   - Evitar errores de sintaxis SQL en `notes_fts MATCH` cuando el usuario escribe caracteres especiales (`-`, `:`, `"`, `(`, `)`, `#`, etc.).
   - Permitir etiquetas con tildes y `ñ` (`#diseño`, `#matemáticas`) tanto en el buscador lateral como en `CommandPalette` (`Ctrl+K`).
3. **[x] Callouts plegables interactivos (`> [!note]-` y `> [!note]+`):**
   - Soportar colapso/expansión interactivo en `CalloutWidget` haciendo clic en el encabezado sin saltar forzosamente al modo código, y añadir botón para editar el bloque.
4. **[x] Invalidación inmediata de caché de notas incrustadas (`![[Nota]]`):**
   - Limpiar `embedExcerptCache` en cuanto se guarda o modifica cualquier nota, para que las vistas previas incrustadas se actualicen al instante.
5. **[x] Creación automática al hacer clic en un `[[Enlace]]` hacia una nota que no existe:**
   - Si el usuario pulsa sobre `[[Nueva Nota]]` (o `[[Nueva Nota#Sección]]`) y la nota no existe, crearla automáticamente, abrirla y (si incluye `#Sección`) añadir o saltar al encabezado.
6. **[x] Gestión avanzada de archivos y carpetas en el panel de navegación:**
   - **Arrastrar y soltar (Drag & Drop / Touch Drag):** Mover notas de una carpeta a otra (o a la raíz) arrastrándolas con el ratón o con el dedo.
   - **Menú contextual en notas (clic derecho / pulsación larga):** Renombrar nota, Mover de carpeta y Eliminar nota.
   - **Menú contextual en carpetas (clic derecho / pulsación larga):**
     - Renombrar carpeta.
     - Eliminar carpeta (eligiendo entre **conservar el contenido** moviéndolo a la raíz/carpeta padre, o **eliminar todo el contenido** enviándolo a la papelera).
     - Mover carpeta a otro Vault disponible.

---

## 3. Mejoras de UX y Arquitectura (Apartado B — Completado)

1. **[x] Reemplazar `window.prompt` y `window.confirm` restantes por modales propios de la UI:**
   - Unificar todos los diálogos de entrada y confirmación bajo el diseño visual de Notium:
     - `createFolder` ahora abre `TreeActionModal` con `{ kind: "create-folder" }` permitiendo crear carpetas y subcarpetas sin alertas nativas.
     - `TrashModal` ahora utiliza tarjetas de confirmación en línea integradas para eliminar notas individuales permanentemente o vaciar la papelera.
2. **[x] Selector nativo de carpetas en PC al abrir un Vault externo (`VaultModal.svelte`):**
   - Comando IPC `vault_pick_folder` respaldado por `rfd::AsyncFileDialog` en entornos de escritorio (cfg'd out para Android) con botón `"Examinar…"` en la interfaz de gestión de Vaults.
3. **[x] Gestión completa de carpetas en el árbol lateral:**
   - Opciones completas en el menú contextual de carpetas: *"Nueva subcarpeta…"*, *"Renombrar carpeta"*, *"Mover a otro Vault…"*, y *"Eliminar carpeta…"* (con elección de conservar o enviar a papelera su contenido).
4. **[x] Botón de "Reconstruir índice" en Ajustes (`SettingsModal.svelte`):**
   - Comando IPC `vault_rebuild_index` expuesto para ejecutar `Index::rebuild` sobre el vault activo y reportar notas escaneadas, actualizadas y adoptadas directamente en la sección de Ajustes.
5. **[x] Icono definitivo de la aplicación (Branding):**
   - Logo oficial `notium-icon.svg` en la raíz del proyecto (sustituye a `Notium.svg`/`Notium.jpg`); también como `public/favicon.svg`.
   - Generación de todos los assets de iconos (Windows `.ico`, PNGs 32 a 512, iOS y mipmaps de Android de alta resolución) mediante la herramienta oficial de Tauri. El icono adaptativo de Android usa solo el glifo sobre fondo `#3B3FE0`.
   - Favicon web enlazado en `index.html`.
6. **[x] Limpieza documental de referencias residuales al antiguo editor `.json`:**
   - Actualización de descripciones de paquetes en `Cargo.toml`, comentarios de resolución de enlaces en `index.rs` y unificación en torno al motor Markdown puro estilo Obsidian.
7. **[x] Modularización del Frontend (`App.svelte` y `markdownEngine.ts`):**
   - Extracción de `SidebarDrawer.svelte`, `RightInspectorPanel.svelte`, `TreeActionModal.svelte`, `LinkRepairModal.svelte` y `src/lib/stores/vault.svelte.ts`, reduciendo `src/App.svelte` de 2418 a `< 800` líneas.
   - Extracción de `caches.ts`, `listWidgets.ts`, `linkAndEmbedWidgets.ts`, `blockWidgets.ts` y `liveDecorations.ts` bajo `src/components/editors/widgets/`, reduciendo `src/components/editors/markdownEngine.ts` de 1667 a `< 650` líneas.
8. **[x] Datos de tema y adopción parcial del estilo Obsidian (`docs/THEME_DATA.md`):**
   - Referencia canónica de design tokens derivada de [`docs/Obsidian_UI_Style_Reference.md`](Obsidian_UI_Style_Reference.md): acento azul (`#76a7f5` / `#0050bf`), paleta neutra (`#1e1e1e`/`#262626`/`#191919`), tipografía Poppins conservada, radio base de `4px` y tokens semánticos nuevos (`--faint`, `--accent-hover`, `--code-bg`/`--code-fg`, `--highlight-bg`, scrollbar y focus).
   - Aplicación de los valores en `src/app.css` (bloques `:root` y `[data-theme="light"]`) junto con scrollbars sutiles y anillo de foco estilo Obsidian.
   - **Corrección (2026-11-16):** la referencia anterior era inexacta. `docs/THEME_DATA.md` se reescribió con los valores reales leídos del `app.css` de Obsidian 1.15.0 (escritorio y móvil): acento morado `#8a5cf5`, rampa de grises real (`#1c1c1c` editor / `#282828` paneles; en móvil oscuro `#000000` / `#1e1e1e`), radios `4/8/12/24px`, tipografía, iconos Lucide y medidas de componentes. Tokens de color y callouts actualizados en `src/app.css`, con overrides móviles. `Obsidian_UI_Style_Reference.md` queda marcado como obsoleto.
   - **[x] CSS de componentes adaptado (2026-11-16):** radios `4/8/12px`, enlaces subrayados con `--accent-text`, encabezados con tamaños/pesos de Obsidian, citas y embeds con borde de acento de `2px`, callouts sin borde con `--callout-color` por tipo y tema, y fondo de ventana Android a juego con el editor. Pendientes menores en "Known gaps" de `docs/THEME_DATA.md`.
   - **[x] Cierre del restilo (2026-11-16):** etiquetas `#tag` como píldoras en el editor, degradado en la barra superior para que el texto no se solape con el título, interruptores con medidas de Obsidian y barras del sistema Android sincronizadas con el tema y con los paneles abiertos (`NotiumAndroid.setSystemBarColor`).
9. **[x] Restilo móvil de Navegación y Backlinks según las referencias de Obsidian:**
   - Paneles móviles a pantalla completa, árbol con filas táctiles más amplias, texto de `17px`, iconografía simplificada y nota activa neutra con forma de píldora (`--panel-2` + `9999px`).
   - Selectores de vista Files/Search/Tags y Backlinks/Conflictos con menús propios, acciones inferiores reorganizadas y accesos rápidos de configuración conservados.
   - Filtro local funcional para Linked mentions, Unlinked mentions y enlaces salientes, conteos reactivos y estados vacíos coherentes con la referencia.
10. **[x] Barra de herramientas configurable estilo Obsidian (2026-11-22):**
   - Catálogo de ~50 comandos (`TOOLBAR_COMMANDS` en `src/components/editors/toolbar.ts`): títulos 1–6, comentario, matemáticas, callout, nota al pie, mover líneas, cursor, tabla, cortar/copiar/pegar, fecha/hora…
   - Panel «Configurar barra de herramientas» (llave inglesa y Ajustes → Barra de herramientas): quitar con X, reordenar arrastrando el asa y añadir desde «Más opciones». Orden guardado por dispositivo en `localStorage`.
   - Editor, motor Markdown y layout desktop sin cambios; reglas móvil documentadas en `docs/THEME_DATA.md`.
10. **[x] Navegación de Ajustes por páginas y animación de hoja inferior:**
    - Nueva portada de Ajustes con buscador funcional, título centrado y filas agrupadas para General, Cloud Sync, Adjuntos, Papelera y Snippets/Guía.
    - Accesos rápidos de sincronización, adjuntos y papelera retirados del footer móvil de navegación; cada herramienta tiene ahora una página propia dentro de Ajustes.
    - `AttachmentsModal` y `TrashModal` admiten modo embebido reutilizando toda su lógica y acciones, con retorno a la portada y apertura de notas conservada.
    - Entrada/salida de Ajustes con deslizamiento vertical (`160px`, `320ms`, `cubicOut`) y fade del overlay, respetando `prefers-reduced-motion`.
11. **[x] Revisión de buenas prácticas frente a la documentación oficial (Tauri 2, Svelte 5, Vite):**
    - **Android reproducible:** `src-tauri/gen/android` (MainActivity, manifiesto, Gradle) ahora está versionado; solo se ignoran `gen/schemas` y los logs de Kotlin. El APK release lo firma Gradle (`signingConfigs.release` + `keystore.properties` sin versionar) en lugar de `apksigner` manual.
    - **Sincronización sin bloqueos:** `run_sync_cycle` ya no retiene el mutex del Vault durante las peticiones HTTP; los ciclos se serializan con un mutex propio y se revalida la raíz del Vault en cada paso local.
    - **Durabilidad:** SQLite en modo WAL con `synchronous=NORMAL` y `busy_timeout`; `atomic_write` hace `fsync` antes del `rename` (la inyección masiva de identidad usa la variante sin `fsync`).
    - **Token de sincronización:** en escritorio se guarda en el almacén de credenciales del sistema (`keyring`) con migración automática desde `sync_config.json`; en móvil se conserva el archivo dentro del sandbox de la app.
    - **IPC binario:** adjuntos mediante `tauri::ipc::Response` / cuerpo crudo de `Request`; los bundles de copia usan base64 (se siguen importando los bundles antiguos con arrays numéricos).
    - **Configuración:** `vite.config.ts` según la guía de Tauri (`envPrefix`, `target` por plataforma, `minify`/`sourcemap` según `TAURI_ENV_DEBUG`), `verbatimModuleSyntax`, sin `vitePreprocess`, perfil release con `lto`/`codegen-units = 1`/`strip`, y `tauri-plugin-log` en lugar de `eprintln!`.
    - **Frontend:** mapa tipado de comandos en `src/lib/ipc/commands.ts` (`call(...)`) para todas las invocaciones IPC, atributos de evento de Svelte 5 en `<svelte:window>`, `lib/stores/vault.svelte.ts` movido a `lib/vault/tree.ts` y suscripción a eventos antes de inicializar el Vault.
    - **Herramientas:** ESLint + Prettier (`npm run lint`, `npm run format`, `npm run verify`), `cargo fmt` y `cargo clippy` sin avisos.
    - **Worker de sincronización:** `AUTH_TOKEN` obligatorio (falla cerrado), comparación en tiempo constante, `compatibility_date` actualizado; el cliente exige `https://` salvo en `localhost`.
    - **Pendiente (seguimiento):** sustituir el puente `addJavascriptInterface` de exportación en Android por `tauri-plugin-dialog` + `tauri-plugin-fs`, y dividir `src/app.css` en estilos por componente.
12. **[x] Navegación "atrás" de Android por capas (2026-11-16):**
    - El botón/gesto atrás ya no cierra la app: cierra la capa superior (menús, modales, página de Ajustes → portada → cierre, paneles de Archivos/Backlinks y, por último, la nota abierta). Solo sin nada abierto sale de la app.
    - Pila de manejadores en `src/lib/backStack.ts`; `MainActivity` desactiva el manejo de historial de Wry y delega en `window.__notiumHandleBack()`.
13. **[x] Diseño responsive unificado (2026-11-16):**
    - Un único interruptor de layout (`is-compact` en `<html>`, `src/lib/layout.svelte.ts`) sustituye la mezcla de detección por user-agent en el marcado y `@media (max-width: 767px)` en el CSS, que se descuadraba en móvil apaisado, tablets y ventanas estrechas de PC.
    - UI compacta en móvil/tablet a cualquier tamaño y en ventanas de PC < 768px; paneles como hojas laterales (`min(84vw, 420px)`) en pantallas anchas; ajustes para pantallas bajas y de 320px.
    - Vista previa en navegador con backend simulado solo en desarrollo (`src/dev/mockBackend.ts`, `npm run dev`, `?mobile`).
