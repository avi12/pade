# Handoff: make "Open in PADE" mimic PowerToys' context menu (2026-07-16)

**Goal:** the "Open in PADE" folder right-click entry should behave exactly like
PowerToys' context menus (**File Locksmith**, **PowerRename**): toggle on/off
cleanly, **no Explorer restart**, **no lingering** entry, and ideally **no
Developer Mode** requirement.

This session got partway there (version-aware registration + a self-hiding
handler). The remaining parity gaps — especially dropping the Developer-Mode wall —
are below. **The OS context menu could not be visually verified from this session
(it's the Explorer shell, not the app); a human on a real Win11 machine must confirm
each behavior.**

---

## Update 2026-07-16 (session 2): gap 2 shipped, gap 1 decided → defer

- **Gap 2 (register-once + self-hide) — DONE.** Commit `0bac855`
  (`fix(contextmenu): register once + self-hide on toggle, never redeploy`). The
  toggle-off now only flips the flag; the Win11 package stays registered. Verified at
  the OS level on a real Win11 box (build 26200): hiding keeps the package registered,
  `context_menu_status` reads registered-but-hidden as off, and re-enabling skips
  redeployment. `modern::unregister` is kept `#[allow(dead_code)]` for a future
  uninstall path. (Still **not** visually confirmed in Explorer — the shell can't be
  driven from the session; a human must eyeball the menu appear/disappear.)
- **Gap 1 (drop Developer Mode) — DECIDED: defer to a future installer** (Avi's call,
  2026-07-16). Rationale: the "no Dev Mode" payoff only helps someone *without* Dev
  Mode, which is nobody today (Avi's own dev machine has it on); a self-signed cert
  doesn't remove the admin step, it relocates it; and cert-trust belongs in an
  **installer's** elevated step, not a UAC prompt on first toggle. When PADE gains an
  installer, ship a signed sparse `.msix`, have the installer trust its (ideally real)
  code-signing cert, then register with `-ExternalLocation` (no `-Register`, no Dev
  Mode). **The mechanism is proven feasible** — on a real Win11 box this session:
  `makeappx pack /nv` packs our exact `AppxManifest.xml` into an 8.9 KB sparse `.msix`
  with no edits; `signtool sign /fd SHA256` signs it with a self-signed `CN=PADE` cert
  (matched to the manifest `Publisher`); the *only* remaining wall is trusting that
  cert (a one-time admin step → `LocalMachine\TrustedPeople`), which is exactly the
  step that belongs in the installer. So gap-1 detail below stands as the runbook for
  when that installer exists.
- **Gap 3 (WinRT `PackageManager` instead of PowerShell)** — still an optional
  nice-to-have; untouched.

### Finding: the entry appears in BOTH menus by design (not a bug, not ours to fix)

On Win11 build 26200 the packaged `IExplorerCommand` handler shows up in **both** the
modern menu *and* "Show more options". Confirmed **not** a stale cache — it survived a
full `explorer.exe` restart and returns on every toggle-on. This is intrinsic Windows
behavior: a modern packaged handler **auto-propagates** into the legacy menu, and
**PowerToys does the identical thing** (PowerRename / Image Resizer — open issues
[#19271](https://github.com/microsoft/PowerToys/issues/19271),
[#25355](https://github.com/microsoft/PowerToys/issues/25355),
[#31574](https://github.com/microsoft/PowerToys/issues/31574),
[#36696](https://github.com/microsoft/PowerToys/issues/36696)). There is **no knob**
— not in the `desktop4`/`desktop5` manifest, not in `IExplorerCommand`
(`GetState`/`GetFlags`) — to place a third-party handler in the modern menu *only*;
that placement is reserved to Windows' own shell verbs. Our registration is actually
**cleaner** than PowerToys: a single entry per menu, because we don't also register a
separate Win10 classic `IContextMenu` handler (which is what doubles PowerToys' legacy
entry). **Decision: accept the dual appearance.**

**OPEN (needs a human):** verify the self-hide flag hides "Open in PADE" from **both**
menus on toggle-off. The modern menu honors `GetState`→`ECS_HIDDEN`; it's unconfirmed
whether "Show more options" does. If the legacy menu ignores `GetState`, toggle-off
would leave a ghost there — *that* would be the real bug (and would force a rethink:
either accept it, or unregister on toggle-off and trade away the no-restart property).

---

## What this session already did (committed, unpushed)

| Commit | Change |
| --- | --- |
| `10acc9d` | **One menu per Windows version.** Win11 (build ≥ 22000, read from the registry) → the modern packaged handler *only*; Win10/older → the legacy registry keys *only*. Registering both was showing "Open in PADE" twice on Win11 (modern menu **and** "Show more options"). |
| `0937383` | **Self-hide, no Explorer restart.** The handler's `IExplorerCommand::GetState` now reads `HKCU\Software\PADE\ContextMenu` (DWORD: `0`=hidden, `1`/absent=shown) fresh on every menu build. Toggle **off** writes `0` first (a cached handler stops showing at once) then removes the package; **on** writes `1`. |

A short-lived commit that **restarted `explorer.exe`** after (un)register was
**dropped** (`git reset`) — that was the wrong lever; PowerToys never restarts
Explorer.

Verified this session: the flag roundtrips (`reg` write/read), both crates pass
`clippy::pedantic -D warnings` + `fmt`, the handler DLL rebuilds. **Not** verified:
the actual menu appearing/disappearing in Explorer.

> After editing the handler, rebuild it: `cargo build -p contextmenu-handler`
> (it's **not** a dependency of `pade`; the DLL lands next to `pade.exe`).

---

## The PowerToys reference (what to mimic)

Repo: `microsoft/PowerToys`. Fetch files with
`gh api repos/microsoft/PowerToys/contents/<path> --jq .content | base64 -d`.

- **`src/common/utils/package.h`** — the shared helper.
  - `RegisterSparsePackage(externalLocation, sparsePkgPath)` uses WinRT
    `PackageManager.AddPackageByUriAsync(packageUri, options)` with
    `options.ExternalLocationUri(externalUri)` + `ForceUpdateFromAnyVersion(true)`.
    **`packageUri` is a built `.msix` sparse package**, not a loose `AppxManifest.xml`.
  - `UnRegisterPackage(name)` → `PackageManager.RemovePackageAsync(fullName)`.
  - `IsWin11OrGreater()` → `VerifyVersionInfo` with `dwBuildNumber = 22000`
    (matches our registry read of `CurrentBuildNumber`).
  - **No `SHChangeNotify`, no Explorer restart** in these functions.
- **Module enable/disable** — `src/modules/FileLocksmith/FileLocksmithExt/PowerToysModule.cpp`,
  `src/modules/powerrename/dll/dllmain.cpp`:
  - `enable()` → `package::RegisterSparsePackage(...)`.
  - `disable()` → does **not** call `UnRegisterPackage`. The package stays
    registered; the handler **self-hides** via `GetState` reading the module's
    enabled state. Toggling is instant, no deployment, no restart.
- `SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, NULL, NULL)` appears **only**
  for classic/registry associations and the installer (e.g.
  `src/modules/registrypreview/RegistryPreviewExt/dllmain.cpp`,
  `installer/PowerToysSetupCustomActionsVNext/CustomAction.cpp`) — never for the
  packaged menu toggle.

---

## Parity gaps to close (the actual task)

### 1. Drop the Developer-Mode requirement — DEFERRED to a future installer (see the 2026-07-16 update above); this is the runbook for then

PADE registers a **loose manifest**: `Add-AppxPackage -Register <AppxManifest.xml>
-ExternalLocation <dir>` (see `src-tauri/src/contextmenu/modern.rs`). That only works
**unsigned when Developer Mode is ON** — so today enabling the menu fails with a
"turn on Developer Mode" error otherwise.

PowerToys ships a **built, signed sparse `.msix`** registered with an external
location, which needs **no** Developer Mode. To mimic:

- Build a sparse package from `AppxManifest.xml` with `makeappx pack /d <dir>
  /p <pkg>.msix /nv` (sparse: `/nv` skips validation for the external-location model).
- Sign it: for a personal machine, a **self-signed cert** whose Subject matches the
  manifest `Publisher`, added to `Cert:\LocalMachine\TrustedPeople` (needs admin
  once); or ship a real cert. `signtool sign /fd SHA256 /a /f cert.pfx <pkg>.msix`.
- Register with `Add-AppxPackage <pkg>.msix -ExternalLocation <dir>` (**no**
  `-Register`), or the WinRT equivalent (gap 3).
- The manifest `Publisher` must equal the signing cert's Subject exactly.

Decide with the user: self-signed-per-machine (a one-time admin trust step) vs.
keeping the dev-mode loose model. This is the crux of "mimic PowerToys."

### 2. Register once + self-hide only (don't unregister on toggle) — DONE (commit `0bac855`)

PADE currently **unregisters the package on disable** (writes flag `0`, then
`Remove-AppxPackage`). PowerToys **keeps the package registered** and only flips the
enabled flag; the handler hides itself. Benefits: re-enable is instant, never
re-prompts Dev Mode / re-deploys. Downside: the package stays registered while "off"
(invisible; only removed on uninstall — which PADE has no installer for yet).

Recommended: register the sparse package **once** on first enable (skip if already
registered), and make the toggle **only** write `HKCU\Software\PADE\ContextMenu`.
`context_menu_status` then = "package registered AND flag ≠ 0". Keep
`modern::unregister` for a future uninstall/cleanup path.

### 3. Use WinRT `PackageManager`, not PowerShell (optional, nice-to-have)

PADE shells out to `Add-AppxPackage` / `Remove-AppxPackage`. PowerToys uses WinRT
`PackageManager` directly. The `windows` crate is already in the tree; add the
`Windows_Management_Deployment` feature to `src-tauri/Cargo.toml` and call
`PackageManager::AddPackageByUriAsync` / `RemovePackageAsync`. Removes the PowerShell
dependency and gives structured errors. Not required for correctness.

---

## Verification plan (needs a human + real Explorer)

Run through this on a Win11 box (and ideally a Win10 VM):

1. Toggle **on** in the picker → right-click a **folder** and a folder's **empty
   background** → "Open in PADE" is in the **first** menu (not "Show more options").
2. Click it → PADE opens that folder as the project.
3. Toggle **off** → right-click again → it's **gone immediately**, **no `explorer.exe`
   flash**, and it doesn't linger under "Show more options" either.
4. Toggle **on** again → it's back (instant, and after gap 1, with **no Developer
   Mode**).
5. Win10: appears in the classic menu; toggling off removes it.
6. Never duplicated (not in both modern + "Show more options" at once).

If step 3 lingers with the current code, it means Explorer isn't re-calling
`GetState` on the cached handler — confirm the flag write happened
(`reg query HKCU\Software\PADE /v ContextMenu`) and that the **rebuilt** DLL is the
one registered (external location = the dir with the fresh `contextmenu_handler.dll`).

---

## Key files & pointers

| File | Role |
| --- | --- |
| `src-tauri/src/contextmenu.rs` | Version detect (`is_windows_11`), legacy keys, the `set_menu_shown` flag writer, and `context_menu_register/unregister/status`. |
| `src-tauri/src/contextmenu/modern.rs` | Sparse-package (un)register via `Add-AppxPackage -Register` / `Remove-AppxPackage` (**← gap 1/3 live here**). |
| `src-tauri/contextmenu-handler/src/lib.rs` | The COM `IExplorerCommand` handler. `GetState` self-hides via `context_menu_hidden()` reading the registry flag; `Invoke` launches `pade.exe <folder>`. |
| `src-tauri/contextmenu-handler/AppxManifest.xml` | Sparse manifest template (`{{EXECUTABLE}}` placeholder). CLSID must match `lib.rs`. |
| `src/panels/picker/OnLaunchSection.svelte` | The "Add Open in PADE to the folder right-click menu" checkbox → `contextMenu.register/unregister/status` (via `lib/bridge.ts`). |
| `docs/handoff-windows11-context-menu.md` | The original build runbook (CLSID, `makeappx`/signing notes, dev-mode model). Parts about "both menus" / dev-mode are now superseded by this doc. |

Shared flag: **`HKCU\Software\PADE\ContextMenu`** (DWORD, `1`/absent = shown,
`0` = hidden). Written by the app, read by the handler on every menu build.
CLSID: `{C6FD5832-8BA5-4FDE-A5CC-A74C36AD27AC}` (authoritative in the handler's
`lib.rs`, mirrored braceless in `AppxManifest.xml`).
