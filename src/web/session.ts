// Login for the web client: the Worker's AUTH_TOKEN plus the vault id.
// Kept in sessionStorage (gone when the tab closes) unless "remember" is ticked.

export interface Session {
  token: string;
  vaultId: string;
  remember: boolean;
}

const KEY = "notium-web-session";

export function loadSession(): Session | null {
  for (const store of [sessionStorage, localStorage]) {
    try {
      const raw = store.getItem(KEY);
      if (raw) return JSON.parse(raw) as Session;
    } catch {
      // Blocked or corrupt storage: ask for the token again.
    }
  }
  return null;
}

export function saveSession(s: Session): void {
  (s.remember ? localStorage : sessionStorage).setItem(KEY, JSON.stringify(s));
}

export function clearSession(): void {
  sessionStorage.removeItem(KEY);
  localStorage.removeItem(KEY);
}

/** Check the token against the Worker before keeping it. */
export async function verifyToken(token: string): Promise<string | null> {
  try {
    const res = await fetch("/sync/health", { headers: { Authorization: `Bearer ${token}` } });
    if (res.ok) return null;
    return res.status === 401 ? "Token incorrecto" : `El servidor respondió ${res.status}`;
  } catch {
    return "No se pudo contactar con el servidor";
  }
}
