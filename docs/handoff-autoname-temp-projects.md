# Handoff: auto-name temp projects via the Copilot API

**Status:** spec + integration research complete, not yet implemented.
**Owner of next session:** implement per this doc. Read `docs/requirements.md` and
the `ade-*` memory files first.

## Goal

When the user starts a **temp workspace** (`workspace_temp`) and then works in it
(agent creates/edits files), ADE should **automatically generate a short,
human-readable project name** and apply it:

- in the **project view** (topbar `.dir` label), and
- in the **projects list** (the Recent list in `ProjectPicker`, replacing the
  `temp-<stamp>` folder name).

The user can **always rename** it; **renaming changes the directory name on disk**.

## Behavior (decided with the user)

- **Name source (layered, platform-aware):**
  1. **Installed agent CLI, one-shot headless — the primary, cross-platform
     path.** ADE already wraps an agent (Claude Code / Codex / …). Invoke it
     non-interactively to produce the name, e.g. `claude -p "<naming prompt>"`
     (or Codex's equivalent), capture stdout, sanitize. This works on **Windows,
     macOS, and Linux**, needs **no extra auth** (reuses the user's
     subscription), and stays agent-agnostic — the ideal default.
  2. **Copilot API — a Windows-only optimization/alternative** (see below). Only
     worth wiring if the agent-CLI path is unavailable and Copilot is signed in.
  3. **Local heuristic — the always-on fallback** (offline, no agent, no token).
- Pick the method per platform/availability at runtime: prefer (1); on Windows,
  (2) may substitute if no agent CLI is present; (3) always backs both.
- **Trigger:** **after first meaningful activity** — once the agent has
  created/edited a few files (watch the Change Feed / `watcher.rs` events) OR
  after the first prompt+response. Debounce and run **once** per temp workspace
  (guard with a flag so it doesn't re-name on every save).
- **Heuristic fallback:** derive from the first prompt (if captured) or the most
  significant new file(s) — e.g. slug of the repo's top-level intent
  (`package.json` name, first heading in a README, or the dominant filename).
- **Rename semantics:** for a temp workspace, rename should rename the dir **in
  place** (keep it under `.../workspaces/`) — NOT force-promote into a root. The
  existing `workspace_rename` currently moves into `roots[0]`; add a variant (or
  branch) that renames a temp dir in place. Both keep it ADE-owned/deletable.

## Where this plugs into the existing code

- Temp creation: `src-tauri/src/workspace.rs` → `workspace_temp()`. Already tracks
  `owned_workspaces`. Names are currently the raw `temp-<stamp>` dir name.
- Naming should ultimately call `workspace_rename`/a new `workspace_set_label`
  that updates the dir name + `recent_projects`/`owned_workspaces` entries (reuse
  `retarget`).
- Frontend already shows the dir name in the topbar (`App.svelte` `shortDir`) and
  the Recent list (`ProjectPicker.svelte` `basename(path)`), so once the dir is
  renamed, both update for free.
- Change detection to trigger naming: the Change Feed already emits per-save
  events (`watcher.rs` → `feed://change`). Count distinct files touched; after N
  (≈3) or the first agent turn, fire the naming routine once.
- Add a new Rust module `copilot.rs` (the LLM client) + a `naming.rs` (heuristic +
  orchestration) behind a Tauri command, e.g. `project_autoname()` returning the
  suggested name; the frontend applies it via the rename/label command.

## Platform matrix

| Platform | Primary method | Notes |
| --- | --- | --- |
| all | **Installed agent CLI one-shot** (`claude -p …`) | Cross-platform, no extra auth, reuses subscription. Preferred everywhere. |
| Windows | Copilot API via MSAL (below) | Optional/alt only; `msalruntime.dll` is Windows-only. |
| macOS/Linux | agent CLI, else heuristic | **No `msalruntime.dll`.** A Copilot token would need the browser/Auth0 path (localStorage) or the MS Copilot app — not worth it; use the agent CLI or heuristic. |
| all | Local heuristic | Always-on fallback. |

## Copilot on Windows — integration research (optional path)

**Windows-only.** Two local repos were studied. **`reverse-engineer-copilot`**
documents the Copilot **chat protocol**; **`any-stt`** shows the **native token
acquisition** on Windows via `msalruntime.dll` (which does **not** exist on
macOS/Linux — hence the agent-CLI path is the real cross-platform answer).

### Native token (the important part for a Tauri/Rust app)

A native process cannot read Copilot's browser-local token. The working native
path (from `any-stt`) is **MSAL Runtime**:

- File: `C:\repositories\avi\any-stt\dll\legacy\token_fetcher.cpp`
- Load `msalruntime.dll` (redistributable, ships with the `pymsalruntime` pip
  package, MIT). It wraps Windows WAM for consumer MSA.
- Params:
  - clientId: `14638111-3389-403d-b206-a6a71d9f8f16`
  - scope: `140e65af-45d1-4427-bf08-3e7295db6836/chatai.readwrite`
  - authority: `https://login.microsoftonline.com/consumers`
  - redirect: `https://login.microsoftonline.com/common/oauth2/nativeclient`
  - **critical:** set additional param `msal_request_type = consumer_passthrough`
    (bypasses the consumer-MSA SPA check).
- Flow: `SignInSilentlyAsync` → fallback `SignInInteractivelyAsync`. Cache the
  token to `%LOCALAPPDATA%\ade\copilot_token.txt`; refresh ~every 45 min.
- The result is an opaque JWE bearer token (~24h lifetime; `any-stt` refreshes on
  a schedule).

> From Rust: either shell out to a tiny bundled helper that calls `msalruntime.dll`,
> or FFI into `msalruntime.dll` directly. `any-stt`'s C++ is the reference.

### Chat protocol (from `reverse-engineer-copilot`)

- Docs: `C:\repositories\avi\reverse-engineer-copilot\FINDINGS.md`,
  `docs/auth0-token-evidence.md`; client:
  `examples/copilot-extension/src/entrypoints/copilot-bridge.content.ts`
  (`bootstrap()`, `openSocket()`, `solveHashcash()`, `drive()`).
- Bootstrap (REST, `credentials: include`, sets anon cookie):
  - `POST https://copilot.microsoft.com/c/api/start?ncedge=1&channel=edge` body `{}`
  - `POST https://copilot.microsoft.com/c/api/conversations?api-version=4` body `{}`
    → `{ id: conversationId }`
- Chat socket: `wss://copilot.microsoft.com/c/api/chat?api-version=2&clientSessionId=<uuid>&accessToken=<JWE>`
  - Send `{event:"setOptions",...}`, `{event:"reportLocalConsents",grantedConsents:[]}`,
    then `{event:"send", conversationId, content:[{type:"text", text:<prompt>}], mode:"smart", context:{}}`.
  - Stream back `appendText` deltas until `done`.
  - If no `accessToken` (or reputation-gated): a `challenge` frame (hashcash)
    must be solved — see `solveHashcash`. With a valid `accessToken`, the
    challenge is suppressed. **Origin** is validated (allowlist:
    `copilot.microsoft.com`, `support.microsoft.com`, `www.bing.com`).

### Naming prompt

Send a compact prompt like: *"Suggest a 2–4 word kebab-case project name for a
codebase with these files and this initial task. Reply with only the name."* plus
the file list (paths, maybe first lines) and the first user prompt. Sanitize the
reply to a safe dir name (`[a-z0-9-]`).

## Caveats / decisions for the implementer

- Copilot token is **browser-local for the Auth0 guest path**; the **MSAL native
  path** is the one that works from ADE. Verify the MSA scope still returns a
  token usable against `c/api/chat` (the two repos use slightly different
  identities — test both the Auth0 audience and the AAD/consumer scope).
- Because token acquisition can fail (no MSA sign-in, offline), the **heuristic
  fallback is mandatory** and must produce a decent name on its own.
- Do all of this behind a feature flag / setting; never block the temp-workspace
  launch on naming (it runs async, after activity).
- Keep it swappable: structure naming behind a small `Namer` trait with
  implementations `AgentCliNamer` (default, all platforms), `CopilotNamer`
  (Windows-only, optional), `HeuristicNamer` (fallback). Select at runtime by
  platform + availability. `copilot.rs` is only compiled/used on Windows
  (`#[cfg(windows)]`).
- Respect existing lint rules (CLAUDE.md): zod at the IPC boundary, destructured
  object params, `await` over `void`, `replaceAll`, native popovers, nested CSS,
  `style:` directives, etc.

## Reference file index

- `C:\repositories\avi\any-stt\dll\legacy\token_fetcher.cpp` — MSAL native token
- `C:\repositories\avi\any-stt\dll\legacy\stt_copilot.cpp` — Copilot WS usage (C++)
- `C:\repositories\avi\any-stt\docs\PROTOCOL.md` — Copilot protocol capture
- `C:\repositories\avi\reverse-engineer-copilot\FINDINGS.md` — chat protocol + auth
- `C:\repositories\avi\reverse-engineer-copilot\examples\copilot-extension\src\entrypoints\copilot-bridge.content.ts` — working client
- `C:\repositories\avi\reverse-engineer-copilot\docs\token-autorotate-handoff.md` — token refresh
