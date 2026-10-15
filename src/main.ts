import App from "@/App.svelte";
import { DARK_SCHEME_QUERY, loadPrefs } from "@/lib/prefs.svelte";
import { Scheme } from "@/lib/types";
import "@/theme.css";
import { mount } from "svelte";

// Resolve a concrete theme synchronously before first paint to avoid a flash;
// loadPrefs() then applies the persisted choice (mode + fonts).
document.documentElement.dataset.theme = matchMedia(DARK_SCHEME_QUERY).matches
  ? Scheme.enum.dark
  : Scheme.enum.light;

// Apply the persisted prefs before mounting, keeping the pre-paint fallback
// theme if they can't be read. Awaited so the first terminal spawn (which
// forwards `appearance.scheme` to the agent's theme sync) never races the
// settings load with the OS-resolved placeholder.
async function applyPersistedPreferences() {
  try {
    await loadPrefs();
  } catch (error) {
    console.warn("pade: could not load preferences, keeping the OS theme", error);
  }
}
await applyPersistedPreferences();

const app = mount(App, { target: document.getElementById("app")! });

export default app;
