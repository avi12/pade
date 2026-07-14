# Handoff — WebView2 blank-on-resize (Windows)

Written 2026-07-14. Read this before touching the terminal resize or the Tauri
window config again. This supersedes the resize section of
`handoff-terminal-resize.md` (that file's *pending feature* section is still valid
— see the bottom here).

## The symptom

On Windows, resizing the PADE window **quickly** (a normal mouse-drag counts —
it fires a continuous `WM_SIZING` stream) makes the webview **stop presenting its
content**: the window shows the flat surface colour with no UI until the resize
stops (and sometimes stays stuck until you interact). A user-visible bug.

## Root cause — it is NOT our code (proven)

This is the well-known **WebView2 windowed-hosting rapid-resize non-present bug**
on Windows (Chromium is slow to resize its content; the host frame outruns the
webview's present). It is inherent to Tauri/wry on Windows.

Proven with a controlled A/B (see the test harness below — every fix was verified
against the **real composited window**, not the DOM):

| Build | At rest | After a rapid-resize storm |
| --- | --- | --- |
| committed scale-fill (CSS `scale` transform) | renders | **blank** |
| scale transform **removed** | renders | **blank** (identical) |
| `--disable-gpu-compositing` | renders | **blank** |
| `--disable-gpu` (software) | renders | **blank** |
| `transparent: true` | renders | **see-through to desktop** (a real user drag reproduced this too) |

So: the CSS scale-fill is **exonerated** (keep it), GPU flags don't help, and
`transparent: true` only swaps "blank" for "see-through" (both are the same
non-present, different fill) — do **not** re-apply it, it alarmed the user twice.
The opaque blank is the **surface colour** (`14,20,27`), i.e. the *content*
vanishes; a `DefaultBackgroundColor` / `WEBVIEW2_DEFAULT_BACKGROUND_COLOR` fix
therefore won't help (it's already surface-coloured).

## The proper test harness (CRITICAL — use this, don't repeat my mistake)

**CDP screenshots render the DOM, not the OS-composited window**, so they CANNOT
see this blank — I wasted hours "verifying" green while the real window was dark.
Capture the *real* window instead:

- `scratchpad/shoot.ps1` — `PrintWindow(PW_RENDERFULLCONTENT)` grabs the window's
  real composited pixels (z-order independent) and prints
  `distinctColorsOnRow=N`: **N==1 ⇒ blank**, N>1 ⇒ rendered. Usage:
  `powershell -File shoot.ps1 <out.png>`.
- `scratchpad/cdp.mjs` — dependency-free CDP client (launch app with
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9222'`).
  `windowdrag` drives the **real OS window** via `Browser.setWindowBounds`.
  ⚠️ `windowdrag` is a *pathological* storm (~40 resizes/s) — it blanks **every**
  config, so it's only good for confirming the opaque blank, NOT for proving a fix.
- **To prove a fix you need a realistic `WM_SIZING` drag**, which `setWindowBounds`
  is not. Either drive a real mouse-drag on the resize border via Win32
  `SendInput`/`mouse_event` + `SetCursorPos` (intrusive — moves the user's cursor;
  get permission), or ask the user to drag once and report. `shoot.ps1` reads the
  result either way.
- See `pade-webview-cdp-reach` memory for the CDP-reach recipe (how to rebuild
  `cdp.mjs`). The `shoot.ps1` capture tool (recreate it — it's the one that can
  actually *see* the blank):

```powershell
param([string]$Out)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class WShoot {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
$p = Get-Process pade | ? { $_.MainWindowHandle -ne 0 } | Select -First 1
$h = $p.MainWindowHandle
$r = New-Object WShoot+RECT; [WShoot]::GetWindowRect($h, [ref]$r) | Out-Null
$w = $r.Right-$r.Left; $ht = $r.Bottom-$r.Top
$bmp = New-Object System.Drawing.Bitmap($w,$ht); $g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc(); [WShoot]::PrintWindow($h,$hdc,2) | Out-Null; $g.ReleaseHdc($hdc)  # flag 2 = PW_RENDERFULLCONTENT
$cy = [int]($ht/2); $d = @{}; for ($x=40; $x -lt $w-40; $x+=8) { $px=$bmp.GetPixel($x,$cy); $d["$($px.R),$($px.G),$($px.B)"]=1 }
$bmp.Save($Out); "distinctColorsOnRow=$($d.Count) (1=blank) -> $Out"
```

## Fix avenues (ranked)

1. **WebView2 composition (visual) hosting — the real Microsoft-recommended fix,
   and the "patch the lib" the user asked for.** wry uses the *windowed*
   `ICoreWebView2Controller`, which has the resize flicker/blank. Microsoft's
   answer for smooth resize is `ICoreWebView2CompositionController` +
   `RootVisualTarget` (DComp/Windows.UI.Composition visual tree the host controls
   and can size in lockstep with the frame). This is a **patch to wry**
   (`wry/src/webview2/`), likely a vendored fork or a `[patch.crates-io]` on wry.
   Substantial (DComp swap-chain/visual setup, bounds sync on
   `WM_WINDOWPOSCHANGED`), but it's the only thing that actually fixes the
   non-present. Check for existing wry PRs/forks first.
2. **`WM_ENTERSIZEMOVE`/`WM_EXITSIZEMOVE` handling in tao** — subclass the window
   proc (raw HWND via `raw-window-handle`) to synchronise the webview present
   during the modal resize loop. Uncertain whether it helps, because with windowed
   hosting the app doesn't own WebView2's present path — likely only avenue 1 truly
   works. Research raphlinus "smooth resize test" for the concept.
3. **Accept the limitation** — it's widely reported with no clean app-side fix
   (tauri#6322, wry#80). Slow/normal-speed resize is fine; only fast resize blanks.

## Sources

- tauri: [#6322 resize slower than wry](https://github.com/tauri-apps/tauri/issues/6322),
  [#13270 transparent workaround](https://github.com/tauri-apps/tauri/issues/13270),
  [#10053 manual webview resize](https://github.com/tauri-apps/tauri/issues/10053) · wry [#80](https://github.com/tauri-apps/wry/issues/80)
- WebView2Feedback: [#906 resize artefacts](https://github.com/MicrosoftEdge/WebView2Feedback/issues/906),
  [#2815 flickers on resize](https://github.com/MicrosoftEdge/WebView2Feedback/issues/2815),
  [#2715 window-menu affects resize](https://github.com/MicrosoftEdge/WebView2Feedback/issues/2715)
- [CoreWebView2CompositionController docs](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2compositioncontroller)
- [raphlinus — the smooth resize test](https://raphlinus.github.io/rust/gui/2019/06/21/smooth-resize-test.html)

## Current committed state (do NOT revert these)

On `main`, all solid and CDP/PrintWindow-verified for normal-speed resize:
`fix(terminal): fill the sub-cell remainder…` (the scale-fill), `chore(deps): drop
@xterm/addon-fit`, plus the earlier pile (xterm 6.1 beta, per-frame reflow, the
four App.svelte fixes, window-visible). Working tree: only `agents.rs` (user's
async-detect WIP — leave it) is modified; this doc is untracked. `tauri.conf.json`
and `window.rs` are back to opaque (transparent reverted).

## Still-unbuilt feature (carried from the previous handoff)

**Resume via native `claude --continue`** + a context-aware green tab-dot. Add a
per-agent `resume` arg to `REGISTRY` in `agents.rs` (mirror `oneshot`), used only
for resumed sessions (`workspaceRelocate.ts:107` seeds `initialPrompt:"continue\r"`,
losing context). Green dot in `SessionTabs.svelte`: click soft-terminates (ESC
`\x1b`) below `CONTEXT_HANDOFF_PCT (90)`, else handoff (reuse `createAutoHandoff`
in `stores/handoff.svelte.ts`) then restart in-tab from the handoff doc.
