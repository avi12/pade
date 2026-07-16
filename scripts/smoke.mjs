// E2E smoke: boot the real app and assert its shell renders. Dependency-free —
// WebView2 speaks the Chrome DevTools Protocol when launched with a debugging
// port, and Node ships fetch + WebSocket. Two checks, DOM-presence only:
//
//   1. `?w=empty`             → the project picker mounts ("Open a project").
//   2. `?w=open&path=<repo>`  → the agent chooser mounts ("Choose an agent"),
//                               proving settings/agents IPC round-trips.
//
// Scope limit: CDP sees the DOM, not the OS-composited surface — this suite
// can NOT detect compositor problems like the WebView2 resize blank (see
// docs/handoff-webview2-resize-blank.md). It is a boot-and-render gate only.
//
// Reuses an already-running instance when one serves CDP on :9222 (the dev
// loop); otherwise launches `pnpm app` itself and tears it down afterwards.
import { execSync, spawn } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";

const CDP_ORIGIN = "http://127.0.0.1:9222";
const APP_URL = "http://localhost:1420";
const LAUNCH_TIMEOUT_MS = 180_000;

async function cdpTargets() {
  try {
    const response = await fetch(`${CDP_ORIGIN}/json`);
    return await response.json();
  } catch {
    return null;
  }
}

let launched = null;
if (!(await cdpTargets())) {
  console.log("smoke: launching the app (pnpm app) with a CDP port…");
  launched = spawn("pnpm", ["app"], {
    shell: true,
    stdio: "ignore",
    env: {
      ...process.env,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9222"
    }
  });
  const deadline = Date.now() + LAUNCH_TIMEOUT_MS;
  while (!(await cdpTargets())) {
    if (Date.now() > deadline) {
      fail("app never exposed CDP on :9222");
    }

    await sleep(2000);
  }
}

const page = (await cdpTargets()).find(target => target.type === "page");
if (!page) {
  fail("no page target on the CDP endpoint");
}

const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.onopen = resolve;
  socket.onerror = reject;
});
let nextId = 1;
const pending = new Map();
socket.onmessage = event => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
  }
};
function send(method, params = {}) {
  const id = nextId++;
  socket.send(
    JSON.stringify({
      id,
      method,
      params
    })
  );
  return new Promise(resolve => pending.set(id, resolve));
}
async function evaluate(expression) {
  const reply = await send("Runtime.evaluate", {
    expression,
    returnByValue: true
  });
  return reply.result?.result?.value;
}

/** Navigate and poll until `expression` is true (the app boots asynchronously). */
async function navigateAndExpect({ url, expression, label }) {
  await send("Page.navigate", { url });
  const deadline = Date.now() + 20_000;
  while (Date.now() < deadline) {
    if (await evaluate(expression)) {
      console.log(`smoke: PASS — ${label}`);
      return;
    }

    await sleep(500);
  }
  fail(label);
}

function fail(label) {
  console.error(`smoke: FAIL — ${label}`);
  teardown();
  process.exit(1);
}

function teardown() {
  socket?.close();

  if (launched?.pid) {
    // The pnpm shim spawns a tree (vite + cargo + the app) — kill all of it.
    try {
      execSync(`taskkill /pid ${launched.pid} /T /F`, { stdio: "ignore" });
    } catch {
      // Already gone.
    }
  }
}

const repoPath = process.cwd();
await navigateAndExpect({
  url: `${APP_URL}/?w=empty`,
  expression: `!!document.querySelector(".picker h1")`,
  label: "picker renders on ?w=empty"
});
await navigateAndExpect({
  url: `${APP_URL}/?w=open&path=${encodeURIComponent(repoPath)}`,
  expression: `[...document.querySelectorAll("h1")].some((h) => h.textContent.includes("Choose an agent")) || !!document.querySelector(".xterm")`,
  label: "agent chooser (or a live terminal) renders on ?w=open"
});

console.log("smoke: all checks passed");
teardown();
