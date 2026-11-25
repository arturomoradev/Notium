import { mount } from "svelte";
import App from "./App.svelte";
import "@fontsource/poppins/400.css";
import "@fontsource/poppins/500.css";
import "@fontsource/poppins/600.css";
import "@fontsource/poppins/700.css";
import "@fontsource/poppins/400-italic.css";
import "@fontsource/cascadia-code/400.css";
import "@fontsource/cascadia-code/700.css";
import "./app.css";

// In a plain browser during development there is no Tauri backend: load a fake
// one so the UI can be previewed (see src/dev/mockBackend.ts).
const backendReady: Promise<void> =
  import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)
    ? import("./dev/mockBackend").then((m) => m.installMockBackend())
    : Promise.resolve();

void backendReady.then(() => mount(App, { target: document.getElementById("app")! }));
