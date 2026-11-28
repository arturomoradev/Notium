<script lang="ts">
  import {
    BUILT_IN_THEMES,
    EDITOR_COLOR_KEYS,
    editorColorsStore,
  } from "../lib/stores/editorColors.svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import Icon from "./editors/Icon.svelte";
  import { marquee } from "../lib/marquee";

  let listOpen = $state(false);
  let selectRoot = $state<HTMLDivElement>();
  // Close the theme list on a click outside it or Escape.
  $effect(() => {
    if (!listOpen) return;
    const outside = (e: PointerEvent) => {
      if (!selectRoot?.contains(e.target as Node)) listOpen = false;
    };
    const esc = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation(); // don't close the whole settings modal
      listOpen = false;
    };
    document.addEventListener("pointerdown", outside, true);
    window.addEventListener("keydown", esc, true);
    return () => {
      document.removeEventListener("pointerdown", outside, true);
      window.removeEventListener("keydown", esc, true);
    };
  });

  const allThemes = $derived([...BUILT_IN_THEMES, ...editorColorsStore.themes]);
  const active = $derived(editorColorsStore.activeTheme);
  // The selected theme, when it's one of the user's (only those can be deleted).
  const activeSaved = $derived(editorColorsStore.themes.find((t) => t.name === active));
  let newName = $state("");
  let themeMessage = $state("");
  // Deleting asks twice: the first click arms the button for that theme.
  let confirmDelete = $state<string | null>(null);

  function saveTheme(): void {
    themeMessage = editorColorsStore.saveTheme(newName);
    if (editorColorsStore.activeTheme === newName.trim()) newName = "";
  }

  // Colours are kept per theme; this page edits the one on screen.
  const theme = $derived(settingsStore.resolvedTheme);
  const colors = $derived(editorColorsStore.colors[theme]);
  let message = $state("");
  let fileInput: HTMLInputElement;

  async function importFile(e: Event): Promise<void> {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    message = editorColorsStore.importFile(file.name, await file.text(), theme);
  }
</script>

<div class="setting">
  <div class="setting-label">
    <strong>Temas guardados</strong>
    <span class="setting-hint"
      >Elegir uno lo aplica. Los temas incluidos no se pueden eliminar.</span
    >
    {#if themeMessage}<span class="setting-hint">{themeMessage}</span>{/if}
  </div>
  <div class="ec-pick">
    <!-- Own dropdown, not <select>: native options can't scroll long names (marquee). -->
    <div class="ec-select" bind:this={selectRoot}>
      <button
        type="button"
        class="ec-name ec-select-btn"
        aria-label="Temas guardados"
        aria-haspopup="listbox"
        aria-expanded={listOpen}
        onclick={() => (listOpen = !listOpen)}
      >
        <span class="ec-select-text" use:marquee
          ><span class="marquee-inner">{active ?? "Personalizado"}</span></span
        >
        <span aria-hidden="true">▾</span>
      </button>
      {#if listOpen}
        <div class="ec-select-list" role="listbox" aria-label="Temas guardados">
          {#each allThemes as t (t.name)}
            <button
              type="button"
              role="option"
              aria-selected={active === t.name}
              class="ec-select-option"
              class:selected={active === t.name}
              onclick={() => {
                editorColorsStore.applySaved(t);
                listOpen = false;
                confirmDelete = null;
                themeMessage = "";
              }}
            >
              <span class="ec-select-text" use:marquee
                ><span class="marquee-inner">{t.name}{t.builtIn ? " (incluido)" : ""}</span></span
              >
            </button>
          {/each}
        </div>
      {/if}
    </div>
    {#if activeSaved}
      <button
        type="button"
        class="danger-btn"
        aria-label={`Eliminar tema ${activeSaved.name}`}
        title="Eliminar tema"
        onclick={() => {
          const name = activeSaved!.name;
          if (confirmDelete !== name) {
            confirmDelete = name;
            return;
          }
          editorColorsStore.deleteTheme(name);
          confirmDelete = null;
          themeMessage = `Tema «${name}» eliminado.`;
        }}
      >
        {#if confirmDelete === activeSaved.name}¿Eliminar?{:else}<Icon
            name="trash"
            size={15}
          />{/if}
      </button>
    {/if}
  </div>
</div>

<div class="setting">
  <div class="setting-label">
    <strong>Guardar los colores actuales como tema</strong>
    <span class="setting-hint">
      Guarda los colores de ambos modos y el CSS cargado. Con un nombre que ya existe, lo actualiza.
    </span>
  </div>
  <form
    class="ec-pick"
    onsubmit={(e) => {
      e.preventDefault();
      saveTheme();
    }}
  >
    <input class="ec-name" type="text" placeholder="Nombre del tema" bind:value={newName} />
    <button type="submit" class="trash-btn" disabled={!newName.trim()}>Guardar</button>
  </form>
</div>

<div class="settings-section-label">Colores</div>
<div class="setting-hint ec-intro">
  Colores del tema <strong>{theme === "dark" ? "oscuro" : "claro"}</strong>; cambia de tema en
  General para ajustar el otro. Fondo, texto y acento cambian toda la app; lo que no toques usa el color del tema.
</div>

{#each EDITOR_COLOR_KEYS as { key, label } (key)}
  <div class="setting">
    <div class="setting-label">
      <strong>{label}</strong>
      {#if !colors[key]}<span class="setting-hint">Predeterminado</span>{/if}
    </div>
    <div class="ec-pick">
      <input
        type="color"
        class:ec-unset={!colors[key]}
        value={colors[key] ?? "#888888"}
        aria-label={label}
        oninput={(e) => editorColorsStore.set(theme, key, e.currentTarget.value)}
      />
      <button
        type="button"
        class="ec-reset"
        disabled={!colors[key]}
        title="Volver al color del tema"
        aria-label={`Restablecer ${label}`}
        onclick={() => editorColorsStore.set(theme, key, null)}>↺</button
      >
    </div>
  </div>
{/each}

<div class="setting">
  <div class="setting-label">
    <strong>Cargar estilos desde archivo</strong>
    <span class="setting-hint">
      <code>.css</code>: una hoja de estilos propia (p. ej.
      <code>.cm-h1 {"{ font-size: 2em }"}</code>).
      <code>.json</code>: colores como <code>{'{ "h1": "#e06c75", "bold": "#e5c07b" }'}</code>, o
      <code>{'{ "dark": {…}, "light": {…} }'}</code> para los dos temas.
    </span>
    {#if editorColorsStore.customCss}
      <span class="setting-hint">CSS activo: {editorColorsStore.customCss.name}</span>
    {/if}
    {#if message}<span class="setting-hint">{message}</span>{/if}
  </div>
  <div class="ec-file">
    <input
      bind:this={fileInput}
      type="file"
      accept=".css,.json,text/css,application/json"
      hidden
      onchange={importFile}
    />
    <button type="button" class="trash-btn" onclick={() => fileInput.click()}>Cargar archivo</button
    >
    {#if editorColorsStore.customCss}
      <button
        type="button"
        class="trash-btn"
        onclick={() => {
          editorColorsStore.clearCustomCss();
          message = "CSS personalizado quitado.";
        }}>Quitar CSS</button
      >
    {/if}
  </div>
</div>

<div class="setting">
  <div class="setting-label">
    <strong>Restablecer todo</strong>
    <span class="setting-hint">Quita los colores de ambos temas y el CSS personalizado.</span>
  </div>
  <button
    type="button"
    class="danger-btn"
    onclick={() => {
      editorColorsStore.resetAll();
      message = "";
    }}>Restablecer</button
  >
</div>

<style>
  .ec-intro {
    margin-bottom: 8px;
  }
  .ec-select {
    position: relative;
  }
  .ec-select-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    text-align: left;
  }
  .ec-select-text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
  }
  .ec-select-list {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    z-index: 5;
    width: 200px;
    max-height: 240px;
    overflow-y: auto;
    padding: 4px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
    box-shadow: var(--shadow);
  }
  .ec-select-option {
    display: flex;
    width: 100%;
    padding: 6px 8px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .ec-select-option:hover,
  .ec-select-option.selected {
    background: var(--panel-2);
  }
  .ec-name {
    width: 160px;
    height: 30px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--panel-2);
    color: var(--text);
    font: inherit;
  }
  .ec-pick,
  .ec-file {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    justify-content: flex-end;
  }
  input[type="color"] {
    width: 40px;
    height: 28px;
    padding: 0 2px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--panel-2);
    cursor: pointer;
  }
  .ec-unset {
    opacity: 0.45;
  }
  .ec-reset {
    width: 28px;
    height: 28px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }
  .ec-reset:disabled {
    opacity: 0.3;
    cursor: default;
  }
</style>
