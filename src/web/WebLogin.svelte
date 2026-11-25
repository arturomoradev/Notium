<script lang="ts">
  import { saveSession, verifyToken, type Session } from "./session.ts";

  let { onlogin }: { onlogin: (s: Session) => void } = $props();

  let token = $state("");
  let vaultId = $state("default");
  let remember = $state(false);
  let error = $state("");
  let busy = $state(false);

  async function submit(e: SubmitEvent): Promise<void> {
    e.preventDefault();
    busy = true;
    error = (await verifyToken(token.trim())) ?? "";
    busy = false;
    if (error) return;
    const s: Session = { token: token.trim(), vaultId: vaultId.trim() || "default", remember };
    saveSession(s);
    onlogin(s);
  }
</script>

<main class="login">
  <form onsubmit={submit}>
    <img src="/favicon.svg" alt="" width="48" height="48" />
    <h1>Notium Web</h1>
    <label>
      Token de autenticación
      <input type="password" bind:value={token} autocomplete="current-password" required />
    </label>
    <label>
      ID del Vault
      <input bind:value={vaultId} required />
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={remember} />
      Recordar en este dispositivo
    </label>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <button type="submit" disabled={busy}>{busy ? "Comprobando…" : "Entrar"}</button>
    <p class="hint">
      Sin "Recordar", la sesión y las notas se borran del navegador al cerrar la pestaña.
    </p>
  </form>
</main>

<style>
  .login {
    min-height: 100dvh;
    display: grid;
    place-items: center;
    padding: 16px;
    background: var(--bg);
    color: var(--text);
  }
  form {
    width: min(360px, 100%);
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 28px 24px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 12px;
  }
  img {
    align-self: center;
  }
  h1 {
    margin: 0 0 6px;
    text-align: center;
    font-size: 1.3rem;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 0.85rem;
    color: var(--muted);
  }
  .check {
    flex-direction: row;
    align-items: center;
    gap: 8px;
  }
  input:not([type="checkbox"]) {
    padding: 9px 10px;
    border-radius: 8px;
    border: 1px solid var(--pill-border);
    background: var(--bg);
    color: var(--text);
    font: inherit;
  }
  input:focus-visible,
  button:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 1px;
  }
  button {
    padding: 10px;
    border: 0;
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .error {
    margin: 0;
    color: var(--danger);
    font-size: 0.85rem;
  }
  .hint {
    margin: 0;
    font-size: 0.75rem;
    color: var(--faint);
  }
</style>
