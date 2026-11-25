<script lang="ts">
  import { onMount } from "svelte";
  import { pushBackHandler } from "../lib/backStack";
  import { call } from "../lib/ipc/commands";
  import Icon from "./editors/Icon.svelte";
  import type { VaultBackupBundleDto, VaultInfoDto, VaultProfileDto } from "../lib/ipc/types";

  interface Props {
    currentVault: VaultInfoDto | null;
    isDesktop: boolean;
    firstRun?: boolean;
    onSwitched: (info: VaultInfoDto) => void;
    onClose: () => void;
  }

  let {
    currentVault,
    isDesktop,
    firstRun = false,
    onRestoredOrSwitched,
    onSwitched,
    onClose,
  }: Props & { onRestoredOrSwitched?: () => void } = $props();

  let root: HTMLDivElement;
  let profiles = $state<VaultProfileDto[]>([]);
  let initialVaultName = $state("");
  let newVaultName = $state("");
  let customPath = $state("");
  let error = $state("");
  let statusMsg = $state("");
  let importInput = $state<HTMLInputElement>();

  // Export progress state
  let exporting = $state(false);
  let exportProgress = $state(0);
  let exportStage = $state("");

  // Delete confirmation state
  let pendingDeleteVault = $state<VaultProfileDto | null>(null);

  async function loadProfiles(): Promise<void> {
    try {
      profiles = await call("vault_list_profiles");
    } catch (e) {
      error = String(e);
    }
  }

  async function saveInitialVaultName(): Promise<void> {
    const trimmed = initialVaultName.trim();
    if (!trimmed) {
      error = "Escribe un nombre para tu Vault inicial.";
      return;
    }
    error = "";
    statusMsg = "";
    try {
      const info = await call("vault_rename_current", { name: trimmed });
      await loadProfiles();
      onSwitched(info);
      onClose();
    } catch (e) {
      error = String(e);
    }
  }

  async function switchVault(path: string): Promise<void> {
    error = "";
    statusMsg = "";
    try {
      const info = await call("vault_open", { path });
      await loadProfiles();
      onSwitched(info);
      onClose();
    } catch (e) {
      error = String(e);
    }
  }

  async function createNamedVault(): Promise<void> {
    const trimmed = newVaultName.trim();
    if (!trimmed) return;
    error = "";
    statusMsg = "";
    try {
      const info = await call("vault_create_named", { name: trimmed });
      newVaultName = "";
      await loadProfiles();
      onSwitched(info);
      onClose();
    } catch (e) {
      error = String(e);
    }
  }

  async function confirmDeleteVault(): Promise<void> {
    const target = pendingDeleteVault;
    if (!target) return;
    error = "";
    statusMsg = "";
    try {
      const nextInfo = await call("vault_delete_profile", { path: target.path });
      pendingDeleteVault = null;
      await loadProfiles();
      onSwitched(nextInfo);
      statusMsg = `El Vault «${target.name}» fue eliminado permanentemente.`;
    } catch (e) {
      error = String(e);
      pendingDeleteVault = null;
    }
  }

  async function openCustomPath(): Promise<void> {
    const trimmed = customPath.trim();
    if (!trimmed) return;
    await switchVault(trimmed);
  }

  async function browseFolder(): Promise<void> {
    error = "";
    try {
      const selected = await call("vault_pick_folder");
      if (selected) {
        customPath = selected;
        await switchVault(selected);
      }
    } catch (e) {
      error = String(e);
    }
  }

  function sleep(ms: number): Promise<void> {
    return new Promise((r) => setTimeout(r, ms));
  }

  async function saveWithAndroidBridge(
    filename: string,
    jsonContent: string,
  ): Promise<{ ok: boolean; message: string }> {
    const bridge = (
      window as unknown as { NotiumAndroid?: { saveJsonDocument: (f: string, c: string) => void } }
    ).NotiumAndroid;
    if (!bridge?.saveJsonDocument) {
      return { ok: false, message: "No disponible" };
    }
    return new Promise((resolve) => {
      const handler = (ev: Event) => {
        window.removeEventListener("notium:android-export-result", handler);
        const detail = (ev as CustomEvent<{ ok: boolean; message: string }>).detail;
        resolve(detail ?? { ok: false, message: "Cancelado" });
      };
      window.addEventListener("notium:android-export-result", handler);
      bridge.saveJsonDocument(filename, jsonContent);
    });
  }

  async function exportVault(): Promise<void> {
    if (exporting) return;
    error = "";
    statusMsg = "";
    exporting = true;
    exportProgress = 8;
    exportStage = "Recopilando notas y adjuntos del Vault…";

    try {
      const bundle = await call("vault_export_bundle");
      const total = Math.max(1, bundle.entries.length);

      for (let i = 0; i < bundle.entries.length; i++) {
        const entry = bundle.entries[i];
        const shortName = entry.rel_path.split("/").pop() ?? entry.rel_path;
        exportProgress = Math.round(15 + ((i + 1) / total) * 65);
        exportStage = `Empaquetando (${i + 1}/${bundle.entries.length}): ${shortName}`;
        await sleep(Math.min(45, Math.max(12, Math.floor(240 / total))));
      }

      exportProgress = 88;
      exportStage = "Generando paquete .notium.json…";
      await sleep(60);
      const json = JSON.stringify(bundle, null, 2);
      const filename = `${bundle.vault_name}-backup-${bundle.exported_at}.notium.json`;

      exportProgress = 95;
      exportStage = "Elige dónde guardar el archivo exportado…";

      // 1. Native Android Storage Access Framework (ACTION_CREATE_DOCUMENT)
      const hasAndroidBridge = Boolean(
        (window as unknown as { NotiumAndroid?: { saveJsonDocument?: unknown } }).NotiumAndroid
          ?.saveJsonDocument,
      );
      if (hasAndroidBridge) {
        const res = await saveWithAndroidBridge(filename, json);
        if (!res.ok) {
          exportStage = "";
          statusMsg = "Exportación cancelada.";
          return;
        }
        exportProgress = 100;
        exportStage = "¡Exportación completada!";
        statusMsg = `Archivo guardado correctamente (${bundle.entries.length} notas/adjuntos incluidos).`;
        return;
      }

      // 2. Desktop File System Access API (Windows Explorer "Save As" picker)
      const savePicker = (
        window as unknown as {
          showSaveFilePicker?: (opts: unknown) => Promise<{
            createWritable: () => Promise<{
              write: (d: string) => Promise<void>;
              close: () => Promise<void>;
            }>;
          }>;
        }
      ).showSaveFilePicker;

      if (typeof savePicker === "function") {
        try {
          const handle = await savePicker({
            suggestedName: filename,
            types: [
              {
                description: "Paquete de respaldo Notium (.json)",
                accept: { "application/json": [".json"] },
              },
            ],
          });
          const writable = await handle.createWritable();
          await writable.write(json);
          await writable.close();
          exportProgress = 100;
          exportStage = "¡Exportación completada!";
          statusMsg = `Copia guardada en la ubicación elegida (${bundle.entries.length} archivos empaquetados en 1 archivo).`;
          return;
        } catch (pickerErr) {
          const msg = String(pickerErr);
          if (
            msg.includes("AbortError") ||
            msg.toLowerCase().includes("aborted") ||
            msg.toLowerCase().includes("cancel")
          ) {
            statusMsg = "Exportación cancelada.";
            return;
          }
          throw pickerErr;
        }
      }

      // 3. Fallback browser download
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = filename;
      a.click();
      URL.revokeObjectURL(url);
      exportProgress = 100;
      statusMsg = `Copia descargada como «${filename}» (${bundle.entries.length} notas/adjuntos en 1 archivo).`;
    } catch (e) {
      error = String(e);
    } finally {
      setTimeout(() => {
        exporting = false;
      }, 450);
    }
  }

  async function importVaultFile(file?: File): Promise<void> {
    if (!file) return;
    error = "";
    statusMsg = "";
    try {
      const text = await file.text();
      const bundle = JSON.parse(text) as VaultBackupBundleDto;
      const info = await call("vault_import_bundle", { bundle });
      statusMsg = `Se importaron ${bundle.entries.length} archivos en «${info.name}».`;
      onSwitched(info);
      onRestoredOrSwitched?.();
    } catch (e) {
      error = String(e);
    }
  }

  function onDocMousedown(e: MouseEvent): void {
    if (firstRun) return;
    if (root && !root.contains(e.target as Node)) onClose();
  }

  function onKeydown(e: KeyboardEvent): void {
    if (!firstRun && e.key === "Escape") onClose();
  }

  onMount(() => {
    void loadProfiles();
  });

  onMount(() => pushBackHandler(() => (firstRun ? false : (onClose(), true))));
</script>

<svelte:window onmousedown={onDocMousedown} onkeydown={onKeydown} />

<div class="modal-overlay">
  <div class="modal vault-modal" bind:this={root} role="dialog" aria-label="Gestión de Vaults">
    <div class="modal-header">
      <h2>
        <Icon name="folder" size={16} />
        {firstRun ? "¡Bienvenido! Nombra tu Vault principal" : "Gestión de Vaults"}
      </h2>
      {#if !firstRun}
        <button class="icon-btn" onclick={onClose} aria-label="Cerrar">✕</button>
      {/if}
    </div>

    <div class="modal-body vault-body">
      {#if error}
        <div class="error-banner">{error}</div>
      {/if}
      {#if statusMsg}
        <div class="reload-banner">{statusMsg}</div>
      {/if}

      {#if firstRun}
        <div class="vault-section">
          <strong>¿Qué nombre quieres ponerle a tu Vault por defecto?</strong>
          <span class="setting-hint">
            Aquí se guardarán tus notas iniciales. Puedes crear más Vaults o cambiar entre ellos en
            cualquier momento.
          </span>
          <div class="vault-input-row">
            <input
              type="text"
              placeholder="Ej. Mis Notas, Personal, Universidad…"
              bind:value={initialVaultName}
              onkeydown={(e) => e.key === "Enter" && void saveInitialVaultName()}
            />
            <button type="button" class="primary" onclick={() => void saveInitialVaultName()}>
              Comenzar
            </button>
          </div>
        </div>
      {:else}
        <div class="vault-section">
          <div class="vault-active-row">
            <div>
              <strong>Vault activo: {currentVault?.name ?? "Notium"}</strong>
              <div class="setting-hint">{currentVault?.root ?? ""}</div>
            </div>
          </div>
        </div>

        {#if pendingDeleteVault}
          <div class="vault-delete-confirm">
            <strong>⚠️ ¿Eliminar permanentemente el Vault «{pendingDeleteVault.name}»?</strong>
            <p>
              Esta acción borrará la carpeta <code>{pendingDeleteVault.path}</code> con todas sus
              notas y adjuntos.
              <strong>Esta acción NO se puede deshacer.</strong>
            </p>
            <div class="vault-delete-actions">
              <button type="button" class="trash-btn" onclick={() => (pendingDeleteVault = null)}>
                Cancelar
              </button>
              <button type="button" class="danger-btn" onclick={() => void confirmDeleteVault()}>
                Sí, eliminar permanentemente
              </button>
            </div>
          </div>
        {/if}

        <div class="vault-section">
          <strong>Vaults disponibles ({profiles.length})</strong>
          <div class="vault-list">
            {#each profiles as p (p.path)}
              <div class="vault-row" class:active={p.active}>
                <div class="vault-row-info">
                  <strong
                    >{p.name}
                    {#if p.active}<span class="badge md">Activo</span>{/if}</strong
                  >
                  <span class="trash-meta">{p.path}</span>
                </div>
                <div class="trash-actions">
                  {#if !p.active}
                    <button
                      type="button"
                      class="trash-btn restore"
                      onclick={() => void switchVault(p.path)}
                    >
                      Abrir
                    </button>
                  {/if}
                  <button
                    type="button"
                    class="trash-btn danger"
                    title="Eliminar este Vault permanentemente"
                    onclick={() => (pendingDeleteVault = p)}
                  >
                    <Icon name="trash" size={14} />
                  </button>
                </div>
              </div>
            {/each}
          </div>
        </div>

        <div class="vault-section">
          <strong>Crear nuevo Vault</strong>
          <div class="vault-input-row">
            <input
              type="text"
              placeholder="Nombre del nuevo Vault (ej. Trabajo, Universidad…)"
              bind:value={newVaultName}
              onkeydown={(e) => e.key === "Enter" && void createNamedVault()}
            />
            <button type="button" class="primary" onclick={() => void createNamedVault()}
              >Crear</button
            >
          </div>
        </div>

        {#if isDesktop}
          <div class="vault-section">
            <strong>Abrir carpeta de Vault existente (PC)</strong>
            <div class="vault-input-row">
              <input
                type="text"
                placeholder="Ruta absoluta (ej. D:\Notas\MiVault)"
                bind:value={customPath}
                onkeydown={(e) => e.key === "Enter" && void openCustomPath()}
              />
              <button type="button" class="trash-btn" onclick={() => void browseFolder()}
                >Examinar…</button
              >
              <button type="button" class="trash-btn restore" onclick={() => void openCustomPath()}
                >Abrir ruta</button
              >
            </div>
          </div>
        {/if}

        <div class="vault-section">
          <strong>Respaldo / Portabilidad (Exportar e Importar)</strong>
          <span class="setting-hint">
            Al exportar se empaquetan todas las notas y adjuntos en un único archivo <code
              >.notium.json</code
            > y se abre el explorador de archivos para que elijas dónde guardarlo.
          </span>

          {#if exporting}
            <div class="export-progress-wrap" role="status" aria-live="polite">
              <div class="export-progress-head">
                <span>{exportStage}</span>
                <strong>{exportProgress}%</strong>
              </div>
              <div class="export-progress-bar">
                <div class="export-progress-fill" style="width: {exportProgress}%"></div>
              </div>
            </div>
          {/if}

          <div class="vault-backup-btns">
            <button
              type="button"
              class="trash-btn restore"
              disabled={exporting}
              onclick={() => void exportVault()}
            >
              {exporting ? "Exportando…" : "Exportar Vault…"}
            </button>
            <button
              type="button"
              class="trash-btn"
              disabled={exporting}
              onclick={() => importInput?.click()}
            >
              Importar paquete…
            </button>
            <input
              bind:this={importInput}
              class="hidden-file"
              type="file"
              accept=".json,application/json"
              onchange={(e) => void importVaultFile(e.currentTarget.files?.[0])}
            />
          </div>
        </div>
      {/if}
    </div>

    {#if !firstRun}
      <div class="modal-footer">
        <button type="button" class="primary" onclick={onClose}>Cerrar</button>
      </div>
    {/if}
  </div>
</div>
