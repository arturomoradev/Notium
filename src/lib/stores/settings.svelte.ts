import { call } from "../ipc/commands";
import type { SettingsDto } from "../ipc/types";

const systemDark =
  typeof window !== "undefined" ? window.matchMedia("(prefers-color-scheme: dark)") : null;

class SettingsStore {
  settings = $state<SettingsDto>({
    source_mode: false,
    theme: "dark",
    font_size: 15,
    image_quality: "1080p",
  });
  /** The theme actually shown: `settings.theme` with `"system"` resolved. */
  resolvedTheme = $state<"dark" | "light">("dark");

  constructor() {
    systemDark?.addEventListener("change", () => this.applyTheme());
  }

  applyTheme(): void {
    const t = this.settings.theme;
    this.resolvedTheme =
      t === "light" || (t === "system" && systemDark && !systemDark.matches) ? "light" : "dark";
    if (typeof document !== "undefined") {
      document.documentElement.dataset.theme = this.resolvedTheme;
      document.documentElement.style.setProperty(
        "--editor-font-size",
        `${this.settings.font_size}px`,
      );
    }
  }

  set(s: SettingsDto): void {
    this.settings = s;
    this.applyTheme();
  }

  async save(s: SettingsDto): Promise<void> {
    this.set(s);
    await call("vault_update_settings", { settings: s });
  }

  async toggleTheme(): Promise<void> {
    const next: "dark" | "light" = this.resolvedTheme === "light" ? "dark" : "light";
    await this.save({ ...this.settings, theme: next });
  }
}

export const settingsStore = new SettingsStore();
