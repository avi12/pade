# Windows 11 modern "Open in PADE" context menu (2026-07-16)

PADE's "Open in PADE" folder right-click entry now targets **both** Windows menus:

| Menu | How it's registered | Reaches |
| --- | --- | --- |
| **Legacy** | `HKCU\…\Directory\shell\PADE` registry keys (`contextmenu.rs`) | Windows 10, and Windows 11's "Show more options" |
| **Modern** (Win11, shown first) | a **packaged `IExplorerCommand` COM handler** + a **sparse MSIX manifest** | Windows 11's default right-click menu |

Windows 11's modern menu **only** loads a handler that has *package identity* — there
is no registry shortcut. So the modern menu is a separate COM DLL registered through a
sparse (external-location) manifest.

## Deployment model (decided): dev-mode loose, unsigned

We register the loose manifest with `Add-AppxPackage -Register <AppxManifest.xml>
-ExternalLocation <dir>`, which works **unsigned** when Windows **Developer Mode** is
ON. No certificate, no `makeappx`/`signtool`. Personal-machine only. The legacy menu
still works everywhere and is additive.

## What was added

| Path | What |
| --- | --- |
| `src-tauri/contextmenu-handler/` | **New workspace-member crate** — a `cdylib` COM server. Build: `cargo build -p contextmenu-handler` → `contextmenu_handler.dll`. |
| `…/contextmenu-handler/src/lib.rs` | `IExplorerCommand` impl (title "Open in PADE", exe icon, `Invoke` reads the folder path from the `IShellItemArray` and launches `pade.exe <folder>`), an `IClassFactory`, and the `DllGetClassObject` / `DllCanUnloadNow` / `DllMain` exports. |
| `…/contextmenu-handler/AppxManifest.xml` | The sparse manifest **template** (one `{{EXECUTABLE}}` placeholder). |
| `src-tauri/src/contextmenu/modern.rs` | Registers/unregisters/queries the sparse package (materializes the manifest + `Assets\` next to the exe, shells out to `Add-AppxPackage` / `Remove-AppxPackage`). |
| `src-tauri/src/contextmenu.rs` | Now drives **both** menus from `context_menu_register/unregister/status`. |
| `src-tauri/Cargo.toml` | Became the workspace root (`[workspace] members = ["contextmenu-handler"]`). |
| `src/panels/picker/OnLaunchSection.svelte` | Surfaces the modern-menu failure (Developer-Mode-off) as a tonal warning. |

**CLSID:** `{C6FD5832-8BA5-4FDE-A5CC-A74C36AD27AC}` — authoritative in the handler
crate's `lib.rs` (`GUID::from_u128`), mirrored **braceless** in `AppxManifest.xml`
(`com:Class@Id` and every `desktop5:Verb@Clsid`). Change all copies together.

### Co-location — where the files sit

The manifest's **ExternalLocation is the directory `pade.exe` runs from** (dev:
`src-tauri/target/debug/`). Both the exe and the handler DLL must be there. Because the
handler is a workspace member, its DLL builds into that **same `target/` dir** next to
`pade.exe` automatically — no copy step. At register time `modern.rs` writes
`AppxManifest.xml` (with `{{EXECUTABLE}}` → the real exe name) and `Assets\*.png` (the
logos, embedded from `src-tauri/icons/`) into that same dir.

## RUNBOOK — enable and verify (must be run by a human; see caveat)

```powershell
# 1. Enable Developer Mode (one time). Settings → System → For developers →
#    Developer Mode = On.  (Unsigned -Register needs it; without it Add-AppxPackage
#    fails with 0x80073CFF.)

# 2. Build the app AND the handler DLL (both land in target\debug next to each other).
cd C:\repositories\avi\pade
pnpm tauri dev        # or `pnpm tauri build` — produces pade.exe + runs the frontend
cargo build -p contextmenu-handler --manifest-path src-tauri\Cargo.toml

# 3. Register. Easiest: in PADE's project picker, tick
#    "Add 'Open in PADE' to the folder right-click menu".
#    That calls context_menu_register, which writes the legacy keys AND registers the
#    sparse package. (Equivalent manual command, if you prefer:)
#      $dir = "C:\repositories\avi\pade\src-tauri\target\debug"
#      Add-AppxPackage -Register "$dir\AppxManifest.xml" -ExternalLocation $dir
#    (the toggle materializes AppxManifest.xml + Assets\ into $dir first)

# 4. Restart Explorer so it loads the handler.
taskkill /f /im explorer.exe & start explorer
#   (PowerShell: Stop-Process -Name explorer -Force)

# 5. Verify: right-click a FOLDER (and a folder's empty background) in File Explorer.
#    "Open in PADE" should appear in the MODERN (first) menu. Clicking it opens PADE
#    on that folder.

# Unregister: untick the toggle, or:
#   Get-AppxPackage -Name PADE.ContextMenu | Remove-AppxPackage
```

If registration fails because Developer Mode is off, the picker shows: *"The modern
Windows 11 menu needs Developer Mode turned on … The legacy right-click menu was still
added."* — the legacy menu (and "Show more options") still works.

## Verified vs. NOT verified

- **Verified here:** `cargo build -p contextmenu-handler` compiles the COM server;
  `pnpm lint:rust` (clippy pedantic, `-D warnings`) + `cargo fmt --check` clean for the
  `pade` crate; the handler crate is clippy-pedantic/fmt clean too; `pnpm lint:js`,
  `lint:css`, and `svelte-check` clean for the UI change. The 371 KB DLL is produced.
- **NOT verified (needs a human + a real Explorer):** that the entry actually appears in
  the live Windows 11 modern menu, that `Add-AppxPackage -Register` accepts this exact
  manifest on the target build, and that `Invoke` launches PADE. This environment has no
  GUI/Explorer to click.

## Known uncertainties in the manifest / COM details

- **`ProcessorArchitecture="neutral"`** — matches the verified MS grant-identity sample.
  If `Add-AppxPackage` rejects it against the native x64 DLL, try `x64`.
- **`Executable`/assets must physically exist** at the external location or deploy can
  report `0x80080204` ("manifest is invalid"). We point `Executable` at the real
  `pade.exe` (always present) and write real logo PNGs, so this should be covered.
- **`com:SurrogateServer`** (not `com:InProcServer`, which does not exist in this schema)
  is the correct element for an in-proc DLL shell handler — the deployment engine writes
  the private `InprocServer32` for us. Verified against MS Learn + two shipping manifests.
- **windows-rs**: the handler builds on `windows-core` 0.61 + `windows-implement` 0.60
  (already in the lockfile via Tauri; no extra copies). `#[implement]` targets the
  generated `*_Impl` wrapper and interface args arrive as `windows_core::Ref` (0.59+
  model).

## Sources

- Extending the Context Menu and Share Dialog in Windows 11 —
  <https://blogs.windows.com/windowsdeveloper/2021/07/19/extending-the-context-menu-and-share-dialog-in-windows-11/>
- Grant package identity by packaging with external location —
  <https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps>
- Integrate a packaged app with File Explorer (com:SurrogateServer + desktop4/5 XML) —
  <https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/integrate-packaged-app-with-file-explorer>
- `com:Class` / `desktop5:Verb` / `desktop5:ItemType` / `uap10:AllowExternalContent`
  schema references — learn.microsoft.com `/uwp/schemas/appxpackage/uapmanifestschema/…`
- `Add-AppxPackage` (`-Register` + `-ExternalLocation`) —
  <https://learn.microsoft.com/en-us/powershell/module/appx/add-appxpackage>
- windows-rs `#[implement]` (0.59+ model) — <https://docs.rs/windows-implement/latest/windows_implement/attr.implement.html>;
  `IExplorerCommand_Impl` trait — microsoft.github.io/windows-docs-rs
- Real shipping manifests: `ikas-mc/ContextMenuForWindows11`, Microsoft `ContextMenuSample`
