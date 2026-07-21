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

// Apply the persisted prefs before mounting, keeping the pre-paint fallback
// theme if they can't be read. Awaited so the first terminal spawn (which
// forwards `appearance.scheme` to the agent's theme sync) never races the
// settings load with the OS-resolved placeholder.
async function applyPersistedPreferences(): Promise<void> {
  try {
    await loadPrefs();
  } catch {
    // Keep the synchronously-resolved fallback theme when prefs can't load.
  }
}
await applyPersistedPreferences();

const app = mount(App, { target: document.getElementById("app")! });

export default app;
