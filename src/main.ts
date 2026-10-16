import App from "@/App.svelte";
import { DARK_SCHEME_QUERY, loadPrefs } from "@/lib/settings/prefs.svelte";
import { isDashboardAddress } from "@/lib/shell/window-route";
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

// The agents dashboard is its own small app: it runs none of a project window's
// machinery (sessions, watchers, close interception), so it mounts in App's place.
async function rootComponent() {
  if (isDashboardAddress(location.search)) {
    return (await import("@/panels/dashboard/AgentsDashboard.svelte")).default;
  }

  return App;
}

const app = mount(await rootComponent(), { target: document.getElementById("app")! });

export default app;
