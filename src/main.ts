import App from "@/App.svelte";
import { loadPrefs } from "@/lib/prefs.svelte";
import "@/theme.css";
import "@xterm/xterm/css/xterm.css";
import { mount } from "svelte";

// Resolve a concrete theme synchronously before first paint to avoid a flash;
// loadPrefs() then applies the persisted choice (mode + fonts).
document.documentElement.dataset.theme = matchMedia("(prefers-color-scheme: dark)").matches
  ? "dark"
  : "light";

// Fire-and-forget: apply the persisted prefs once loaded, keeping the pre-paint
// fallback theme if they can't be read. Owns its own try/catch so it never rejects.
async function applyPersistedPreferences(): Promise<void> {
  try {
    await loadPrefs();
  } catch {
    // Keep the synchronously-resolved fallback theme when prefs can't load.
  }
}
applyPersistedPreferences();

const app = mount(App, { target: document.getElementById("app")! });

export default app;
