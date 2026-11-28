// Entry point of the web build (web/index.html, `npm run build:web`). The
// native app keeps using src/main.ts; nothing here is bundled into it.
import { mount, unmount } from "svelte";
import App from "../App.svelte";
import "@fontsource/poppins/400.css";
import "@fontsource/poppins/500.css";
import "@fontsource/poppins/600.css";
import "@fontsource/poppins/700.css";
import "@fontsource/poppins/400-italic.css";
import "@fontsource/cascadia-code/400.css";
import "@fontsource/cascadia-code/700.css";
import "../app.css";
import "../lib/stores/editorColors.svelte";
import "./web.css";
import WebLogin from "./WebLogin.svelte";
import { clearCache } from "./cache.ts";
import { hideSplash } from "../lib/splash.ts";
import { clearSession, loadSession, type Session } from "./session.ts";

const target = document.getElementById("app")!;

async function start(session: Session): Promise<void> {
  const { installWebBackend } = await import("./backend/index.ts");
  await installWebBackend(session);
  mount(App, { target });
  addLogout();
}

/** Small fixed "Salir" button: forget the token and the cached vault. */
function addLogout(): void {
  const btn = document.createElement("button");
  btn.className = "web-logout";
  btn.textContent = "Salir";
  btn.title = "Cerrar sesión y borrar las notas de este navegador";
  btn.onclick = async () => {
    clearSession();
    await clearCache();
    location.reload();
  };
  document.body.append(btn);
}

const saved = loadSession();
if (saved) void start(saved);
else {
  // No session: drop any vault cached by an earlier remembered login.
  void clearCache();
  hideSplash();
  const login = mount(WebLogin, {
    target,
    props: {
      onlogin: (s: Session) => {
        void unmount(login);
        void start(s);
      },
    },
  });
}
