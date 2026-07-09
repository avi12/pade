import "./theme.css";
import "@xterm/xterm/css/xterm.css";
import { mount } from "svelte";
import App from "./App.svelte";
import { loadPrefs } from "./lib/prefs.svelte";

// Resolve a concrete theme synchronously before first paint to avoid a flash;
// loadPrefs() then applies the persisted choice (mode + fonts).
document.documentElement.dataset.theme = window.matchMedia("(prefers-color-scheme: dark)").matches
  ? "dark"
  : "light";
void loadPrefs();

const app = mount(App, { target: document.getElementById("app")! });

export default app;
