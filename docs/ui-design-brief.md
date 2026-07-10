# PADE — UI Design Brief

A complete visual + interaction spec of PADE's UI, written to hand to an AI design
tool. **The entire product must be designed in Material 3 Expressive** (see §1 — it
is non-negotiable and drives every color, shape, type, and motion choice below).

---

## 0. What PADE is

PADE ("Power ADE" — an **Agentic Development Environment**) is a desktop app that
wraps an AI coding-agent CLI (Claude Code, Codex, …) running **unmodified in a real
terminal**, and builds a comprehension-first GUI around it. The user stays the
owner while the agent writes: a live terminal on the left, glanceable review panels
on the right. Calm, focused, developer-grade — never noisy.

**Platform:** desktop (Windows-first), a single resizable window, min 720×480,
default 1200×800. Dense but breathable. Keyboard- and mouse-driven.

---

## 1. Design language — Material 3 Expressive (REQUIRED)

Hand-authored M3 Expressive tokens. Light + dark schemes derived from **one blue
seed hue (210°)**; the app follows the OS theme automatically (no toggle). Use
**tonal surfaces, not borders/shadows**, for separation; full-pill buttons/chips;
large rounded cards; an expressive type scale; and emphasized easing for motion.

### 1.1 Color roles

All values are HSL. Two full schemes.

| Role | Light | Dark | Used for |
| --- | --- | --- | --- |
| **primary** | `hsl(210 90% 45%)` | `hsl(210 90% 68%)` | brand mark, active/selected, run buttons, focus ring, working state |
| **on-primary** | `#ffffff` | `hsl(210 90% 12%)` | text/icon on primary |
| **primary-container** | `hsl(210 90% 92%)` | `hsl(210 55% 22%)` | selected chip/tab/row background |
| **on-primary-container** | `hsl(210 90% 18%)` | `hsl(210 90% 88%)` | text on primary-container |
| **tertiary** (green) | `hsl(160 55% 38%)` | `hsl(160 55% 62%)` | success / "ready" / additions / created files |
| **warn** (amber) | `hsl(28 90% 46%)` | `hsl(28 90% 62%)` | high usage (≥80%), cautions |
| **crit** (red) | `hsl(352 72% 50%)` | `hsl(352 80% 70%)` | deletions, destructive, close-hover |
| **surface** | `hsl(210 30% 98%)` | `hsl(214 30% 8%)` | app background |
| **surface-1** | `hsl(210 28% 96%)` | `hsl(214 28% 11%)` | top bars, cards |
| **surface-2** | `hsl(210 26% 93%)` | `hsl(214 26% 14%)` | chips, hovers, inset controls |
| **surface-3** | `hsl(210 24% 89%)` | `hsl(214 24% 19%)` | pills, tags, tracks, tooltips |
| **on-surface** | `hsl(214 30% 14%)` | `hsl(210 24% 92%)` | primary text |
| **on-surface-var** | `hsl(214 14% 40%)` | `hsl(214 14% 66%)` | secondary text, metadata |
| **outline** | `hsl(214 15% 82%)` | `hsl(214 15% 28%)` | hairline dividers (1px), used sparingly |
| **code-bg / code-fg** | `hsl(210 30% 96%)` / `hsl(214 30% 20%)` | `hsl(214 40% 6%)` / `hsl(210 20% 85%)` | terminal + diff/code surfaces |

Neutrals are **blue-tinted** (210–214° hue), never pure grey — this is deliberate,
part of the M3 tonal system seeded from the blue primary. Prefer stepping between
`surface`→`surface-1`→`surface-2`→`surface-3` for elevation instead of drop shadows
(shadows appear only on floating popovers).

### 1.2 Shape scale

Pill everywhere it reads as an action; generous radii on containers.

- **small** `8px` — rows, small tags, tooltips, inline chips
- **medium** `14px` — panels' inner cards, code/diff blocks, inputs
- **large** `20px` — primary cards (agent options, temp-start card)
- **x-large** `28px` — hero/onboarding card
- **full pill** `999px` — buttons, chips, tabs, badges, usage meter, segmented control, icon buttons, count bubbles

### 1.3 Typography

- **UI font:** `Google Sans`, then `Segoe UI`, `system-ui` (expressive, friendly geometric sans).
- **Mono font:** `JetBrains Mono`, then `Cascadia Code`, `ui-monospace` — for paths, code, commands, filenames, SHAs, the terminal.
- **Expressive display:** page titles use a large, tight, bold clamp — `clamp(24px, 4vw, 36px)`, weight 700–800, letter-spacing `-0.02em`, `text-wrap: balance`.
- Body ~14px; secondary 12–13px; metadata 11px. Section eyebrows/labels: 11–12px, UPPERCASE, letter-spacing `0.06–0.08em`, in `on-surface-var`.
- Numbers that align in columns use `tabular-nums`.

### 1.4 Motion

- **Emphasized easing:** `cubic-bezier(0.2, 0, 0, 1)` for all transitions.
- Durations: 120ms (tooltips), 150–200ms (hover/color), 250–300ms (enter/reveal, meter fill).
- Signature motions: cards/list items **rise** in (fade + 4–8px upward translate); the "working" status dot **pulses** (opacity 100%↔35%, 1.1s); a "ready" dot gets a soft tertiary halo (4px ring). The send-to-agent button **pops** in.
- **Respect `prefers-reduced-motion`:** all transitions/animations off.

### 1.5 Density, spacing, misc

- Layout via flex/grid + `gap` (8–14px typical; 24–28px between page sections).
- Thin, subtle scrollbars (10px, translucent thumb); the **terminal shows no scrollbar** (wheel-scroll only).
- Pure-CSS tooltips on hover/focus (`surface-3` bubble, 11px, 6px below the anchor).
- Visible focus ring everywhere: 2px `primary` outline, 2px offset.
- Icons: Lucide-style 1.5–2px stroke, `currentColor`, ~16px, inline with text.

---

## 2. App shell & global layout

Three top-level "phases" render full-window, one at a time:

1. **Onboarding** — pick an agent (only when several are installed).
2. **Project picker** — choose/create a project (on demand, or on launch if opted in).
3. **Workspace (ready)** — the main working screen.

The **Workspace** is a vertical stack: a **top bar** (fixed height) over a **body**
that is a two-column grid — a **terminal pane** (flexible, left) and an optional
**side panel** (right, `minmax(320px, 420px)`). On narrow widths (<720px) the side
panel drops below the terminal (rows `1fr 40%`).

---

## 3. Screens

### 3.1 Onboarding — "Choose an agent"

Full-window, centered, on a soft radial gradient (`surface-1`→`surface`). A single
**x-large (28px) card** on `surface-1`, max 560px:

- Brand eyebrow **`◆ PADE`** in primary, bold.
- Expressive title **"Choose an agent to start."**
- One-sentence lede in `on-surface-var` (max 46ch): several agents are installed;
  pick one, switch/combine later.
- A vertical list of **agent option cards** (large 20px radius, `surface-2`): each
  row shows the **agent name** (16px, weight 600) on the left and its **command**
  (mono, 12px, `on-surface-var`) pushed to the right. Hover fills the card with
  `primary-container`, text→`on-primary-container`, adds a 1px primary border, and
  animates over 200ms.

### 3.2 Project picker — "Open a project"

Full-window, scrollable, same radial-gradient ground. A centered column
(`min(680px, 100%)`), sections stacked with 28px gaps:

- **Header:** `◆ PADE` eyebrow, expressive H1 "Open a project", a lede.
- **Quick-start section:**
  - A prominent **"Start in a temp workspace"** card (large radius, filled with
    `primary-container`, 1px primary border, a `✦` glyph, a strong title + small
    subtitle). Hover brightens.
  - **"On launch with no project"** — a small label + a **pill segmented toggle**
    with two options: *Temp workspace* | *This picker* (selected segment filled
    `primary-container`).
  - Two **checkbox rows** (accent-colored checkboxes, `primary`): "Auto-name temp
    workspaces once the agent starts working" and (Windows only) "Add 'Open in
    PADE' to the folder right-click menu".
- **Recent section** (if any): header "Recent" + a small ghost **"Clear"** button
  (trash icon, turns `crit` on hover). A list of **recent rows**: each is a wide
  ghost button showing an optional small **`temp`** pill tag, the **display name**
  (mono, 13px — a friendly auto-derived label when present, else the folder name),
  and the **full path** (11px, `on-surface-var`, truncates). Hover fills
  `surface-2`. A trailing **kebab (⋯) button** opens a native popover menu:
  *Open in Files*, *Open in Terminal*, *Open in <IDE>*, and — for PADE-owned
  workspaces — a divider then *Rename to a project*, *Move…*, and a `crit`-tinted
  *Delete workspace*. Inline rename swaps the row for a small text field + Save/Cancel.
- **Default agent section** (if >1 real agent): title + hint + a row of **choice
  chips** (pills); the selected one is `primary-container` with a primary border.
- **Root folders section:** an add row — a mono text input (placeholder
  `C:\repositories  ·  paste a folder path`), a **"Browse…"** outline button
  (folder icon), and a filled **"Add root"** button (disabled until non-empty).
  Below, each root shows a mono code path + a small **×** remove button, and a list
  of detected **project rows** (filled `surface-2` cards; project name bold; a
  small uppercase tertiary **`git`** tag on git repos; hover→`primary-container`).
  Each project row also has the same kebab menu. Empty roots read "No projects
  found in this folder."
- **New project section** (if any root): a root `<select>`, a `project-name` input,
  a 3-row textarea ("First prompt for the agent (optional) — …"), and a filled
  **"Create & open"** button.

All inputs: 14px, `surface-2` fill, 1px `outline`, 14px radius, mono for path fields.
All primary buttons: filled `primary`/`on-primary`, pill or 14px radius, 600 weight,
50% opacity when disabled.

### 3.3 Workspace — the main screen

#### Top bar

A single-row bar on `surface-1` with a 1px `outline` bottom divider,
`padding: 8px 16px`, `gap: 12px`, items vertically centered, horizontally scrollable
if cramped. Left→right:

1. **Brand** `◆ PADE` — primary, bold, letter-spacing 0.02em.
2. **Project name button** — a ghost button (mono, 13px) separated by a left 1px
   divider. Shows an optional small **`temp`** pill (uppercase, `surface-3`), the
   **directory name or friendly label** (truncates at ~32ch), and a **`switch`**
   hint in primary that fades in on hover (opens the Project picker). Tooltip = full path.
3. **Session tabs** (`nav`, "Agent sessions") — one **pill tab** per running agent
   session: the active tab is filled `primary-container` with 600-weight
   `on-primary-container` text; inactive tabs are `surface-2`. Each tab = a label
   button + a **× close** button (turns `crit` on hover).
   - A round **`+` add button** (30px pill, `surface-2`) opens a **native popover
     menu** listing every installed agent to launch, and — when the project is a
     git repo — a divider "On a branch — new worktree" followed by each branch (git
     icon) to spawn an agent on its own worktree.
4. **Spacer** (pushes the rest right).
5. **Usage meter** — see §4.6.
6. **Design menu** — a pill button `✦ Design ▾` (`surface-2`); see §4.4.
7. **IDE menu** — a pill button `⇱ Open in <best-fit IDE> ▾` (`surface-2`); opens a
   popover of installed editors, the project-type best-match tagged "best fit".
8. **Side-panel segmented control** — a pill-grouped `role="tablist"` of four
   tabs, each an icon+label: **Change Feed**, **Git**, **Tasks**, **Config**. The
   selected tab is filled `primary-container`. Clicking the active one closes the panel.

#### Body — terminal pane (left)

Fills the remaining space. Contains the active agent's **terminal** (xterm.js):

- A thin **session bar** on top (`surface-1`, 1px bottom divider) holding a
  **Session badge** (§4.7): a colored status dot + the agent label (mono, bold) +
  a state phrase.
- The terminal itself sits on `code-bg` (theme-matched), full-bleed to the pane
  edges (only 8px top/left inset), mono, GPU-rendered, no scrollbar. All sessions
  stay mounted; only the active one is visible so scrollback survives switching.

#### Body — side panel (right)

When open, an `aside` on `surface` with a 1px left divider, showing one of four
panels (§4.1–4.3, §4.5). Each panel has a small header (`h2`, 15px) + a scroll area.

#### Send-to-agent FAB

When the user selects text inside a side panel, a **floating pill button** pops up,
bottom-center: filled `primary`, "◆ Send to agent" + a truncated mono preview of the
selection, with a primary-tinted shadow. Clicking injects the selection into the
active terminal's input.

---

## 4. Components (detail)

### 4.1 Change Feed panel

Header "Change Feed" + a small count **bubble** (pill, `primary-container`). Empty
state: a calm sentence ("Waiting for edits…"). Otherwise a scrolling list of
**event cards** (newest first, `surface-1`, 14px radius) each with a **3px left
accent stripe** colored by kind: **created → tertiary**, **modified → primary**,
**deleted → crit**. Card content: a colored dot + filename (mono, bold, truncates) +
relative time ("12s ago"); a plain-language summary line ("Grew App.svelte by 8
lines"); a meta row with the directory (mono, 11px) and a stat (`+N` tertiary /
`−N` crit). Cards **rise** in on arrival.

### 4.2 Git / Version-control panel

Header "Version control" + a round **⟳ refresh** icon button. If not a repo: a calm
empty line. Otherwise a scroll area with grouped sections:

- **Unreviewed** (unstaged) — header with a **tertiary dot** + count pill. Rows: a
  small **status square** (18px, 8px radius, white glyph — `C` created/untracked =
  tertiary, `M`/`R` modified/renamed = primary, `D` deleted = crit) + filename
  (mono). Selected row = `primary-container`.
- **Staged** — same, header with a **primary dot**.
- **Diff viewer** (when a file is selected): a 14px-radius bordered block — a mono
  title bar (`surface-2`) over a `code-bg` `<pre>` where **added lines** get a
  translucent tertiary wash, **removed** a translucent crit wash, and **hunk/meta**
  lines read in `on-surface-var`. Max-height ~320px, scrolls.
- **Recent commits** — a list: short **SHA** (mono, primary), summary, and
  `author · relative-time` (11px, `on-surface-var`).

"Working tree clean." when empty.

### 4.3 Tasks panel

Header "Tasks" + a **⟳ refresh** icon button. Empty state explains: add a manifest
(package.json / Cargo.toml / Makefile / pyproject.toml) and its tasks appear.
Otherwise scroll area grouped **by manifest**: each group header shows a **kind
pill** (`npm` = `primary-container`; `cargo` = tertiary-tinted; others = `surface-3`)
+ the manifest filename (mono, truncates, tooltip = its directory). Each task **row**
(hover→`surface-2`): the task **name** (bold, 13px), its **command** (mono, 11px,
`on-surface-var`, truncates), and a trailing filled **"Run"** pill button (primary)
that launches it in a new terminal tab. Auto-refreshes when a manifest changes.

### 4.4 Design menu

Pill button `✦ Design ▾`. Opens a **native popover** (rounded, `surface-2`, soft
shadow): a small hint "Open a design-to-code tool", then a ranked list of AI
UI-generation tools — **Claude, Google Stitch, Vercel v0, Figma Make**. Each item
shows the tool **name** + its **vendor** (11px, right-aligned). The tool whose
vendor matches the **active agent** is pinned first and tagged **"best fit"**
(tertiary, uppercase). Picking one opens that tool's live UI in a companion PADE
window.

### 4.5 Config panel

Header "Agent config". A list of the active agent's config files, each row: a small
uppercase **kind pill** (`instructions` = tertiary-tinted, `mcp` = primary-tinted,
`settings` = `surface-3`) + the relative path (mono); absent files are dimmed (45%)
and disabled, tagged "absent". Selecting an existing file shows a read-only mono
viewer on `code-bg` (14px radius) + a note "Read-only in the MVP — edits will write
back to this same file."

### 4.6 Usage meter (top bar)

A compact pill (`surface-2`). When a precise percent is known: a small rounded
**track** (46×6px, `surface-3`) with a **fill** (primary; turns **warn/amber** at
≥80%), then `NN%` (tabular-nums) and an optional ` · resets 3d/5h`. When only a plan
tier is known: just the tier label (e.g. "Claude max"). When nothing is known: a
muted **"usage —"** with a tooltip explaining why. Fill animates on change (300ms).

### 4.7 Session badge (status)

An inline row: a **status dot** + optional mono label + a state phrase.
- **starting** — neutral dot, "Starting…"
- **working** — **primary** dot, **pulsing**, "Working"
- **ready** — **tertiary** dot with a soft halo, "Ready — waiting for you" (phrase in tertiary, bold)
- **exited** — neutral dot, "Done — session ended"

### 4.8 Popover menus & tooltips (shared)

All dropdowns are **native popovers** anchored to their trigger (light-dismiss on
outside click): rounded (`14px`), `surface-2`, 1px `outline`, soft shadow, ~180–220px
min width; items are full-width ghost buttons (8px radius) that fill
`primary-container` on hover; optional uppercase section separators. Tooltips: tiny
`surface-3` bubble below the anchor, fade in on hover/focus.

---

## 5. Interaction & state summary

- **Selection → action:** selecting text in any side panel reveals the send-to-agent FAB.
- **Everything glanceable:** color encodes meaning consistently — tertiary = good/created/ready, primary = active/modified/working, crit = deleted/destructive, warn = high usage.
- **Selected/active** anything = `primary-container` fill.
- **Hover** ghost items = `surface-2`; hover menu items = `primary-container`.
- **Empty states** are a single calm sentence in `on-surface-var`, never an error tone unless truly an error.
- **Loading** is implicit/fast; panels populate in place.

## 6. Accessibility

- Semantic elements first: real `<button>`, `<nav>`, `<output>` (status/live regions), headings, lists; ARIA only to fill gaps.
- Every icon-only control has an accessible label + tooltip.
- Visible focus ring (2px primary, 2px offset) on all interactive elements.
- Full light/dark support following the OS; sufficient contrast in both.
- `prefers-reduced-motion` disables all animation.

---

### Quick recap for the generator

> Design a calm, developer-grade desktop **Agentic Development Environment** in
> **Material 3 Expressive**, seeded from a **blue (210°) primary**, with a
> green **tertiary** and red **crit** as semantic accents, blue-tinted tonal
> surfaces (no hard shadows except floating menus), **full-pill** buttons/chips/tabs,
> **20–28px** rounded cards, an expressive bold display type paired with
> **JetBrains Mono** for all code/paths, and **emphasized-easing** motion (pulse for
> "working", halo for "ready", rise-in cards). Auto light/dark from the OS. Screens:
> an **agent-picker onboarding card**, a **project picker**, and the main
> **workspace** = top bar (brand, project name, agent-session pill tabs + add menu,
> usage meter, Design & IDE menus, a 4-tab segmented control) over a **terminal**
> beside a swappable **side panel** (Change Feed / Git diff / Tasks / Config), plus a
> floating **"Send to agent"** action.
