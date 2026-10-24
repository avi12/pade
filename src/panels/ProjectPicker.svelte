<script lang="ts">
  import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
  import { updatePrefs } from "@/lib/settings/prefs.svelte";
  import { addRoot as requestAddRoot, sendSettingsMessage } from "@/lib/settings/settings-requests";
  import { settings } from "@/lib/settings/settings.svelte";
  import { AddRootStatus } from "@/lib/types";
  import type { Agent, OpenTarget, ProjectEntry, Settings } from "@/lib/types";
  import BrandMark from "@/lib/ui/BrandMark.svelte";
  import { isTemporaryWorkspace } from "@/lib/util/paths";
  import { settledValue } from "@/lib/util/settled";
  import type { Relocator } from "@/lib/workspace/relocate/workspace-relocate";
  import AgentsSection from "@/panels/picker/AgentsSection.svelte";
  import "@/panels/picker/chrome.css";
  import { createEditorCatalog } from "@/panels/picker/editorCatalog.svelte";
  import EditorsSection from "@/panels/picker/EditorsSection.svelte";
  import HomeSection from "@/panels/picker/HomeSection.svelte";
  import { createWorkspaceLifecycle } from "@/panels/picker/lifecycle.svelte";
  import OnLaunchSection from "@/panels/picker/OnLaunchSection.svelte";
  import { popularRoot, watchedDirectories } from "@/panels/picker/project-roots";
  import RecentSection from "@/panels/picker/RecentSection.svelte";
  import RootsSection from "@/panels/picker/RootsSection.svelte";
  import WorkspaceDialogs from "@/panels/picker/WorkspaceDialogs.svelte";
  import { onDestroy, onMount } from "svelte";

  // Shown when the app wasn't launched inside a project. Manage root folders,
  // browse the projects inside them, open one, or create a new one - then hand
  // the chosen project path (and optional first prompt) back to the app.
  const {
    agents,
    onopen,
    onmove,
    onrename,
    onrenamefolder,
    ondelete,
    ondeleteproject
  }: {
    agents: Agent[];
    onopen: (target: OpenTarget) => void;
    onmove: (target: {
      from: string;
      destDir: string;
    }) => Promise<string>;
    onrename: (target: {
      from: string;
      newName: string;
    }) => Promise<string>;
    /** Renames any project's folder where it stands, through the relocator. */
    onrenamefolder: Relocator["renameFolder"];
    /** Deletes an owned workspace through the app's relocator: kills the
     *  sessions holding the folder as cwd (else the OS blocks the removal),
     *  then removes it. */
    ondelete: (path: string) => Promise<Settings>;
    /** Deletes any other project's folder through the app's relocator, with
     *  the same session handling. */
    ondeleteproject: (path: string) => Promise<Settings>;
  } = $props();

  const RESCAN_DEBOUNCE_MILLISECONDS = 250;

  let projectsByRoot = $state<Record<string, ProjectEntry[]>>({});
  // Whether the first settings load has landed. The sections render only then,
  // so every control is born showing the persisted preference - never a
  // defaults flash that snaps to the real values a beat later.
  let isLoaded = $state(false);
  // The machine's editors and project kinds - shared across the picker, so a
  // re-detect from the Editors "Reload" button updates every section at once.
  const editors = createEditorCatalog();

  // Re-scan every root's projects into projectsByRoot - the cheap part of a
  // reload (a handful of directory reads), shared by both refresh paths.
  async function scanRoots() {
    const scanned = await Promise.all(settings.roots.map(async root => [root, await scan(root)] as const));
    return Object.fromEntries(scanned);
  }

  // Light reload: the project lists only - prune the settings, re-scan the roots.
  // A workspace delete/rename/move, and a folder appearing or vanishing under a
  // watched root, change the projects but never the machine's installed editors,
  // so re-detecting those here (≈1.8s of registry/PATH probing - `ide_detect` +
  // `ide_kind_options`) would stall the picker for nothing. The lifecycle ops and
  // the folder watcher use this; only mount and the Editors "Reload" button pay
  // for a full editor re-detect.
  async function refreshProjects() {
    // prune, not settings: a folder deleted outside PADE is forgotten here, so
    // its row leaves the page (collapsing out) instead of lingering as a link
    // to nothing.
    await sendSettingsMessage(Command.workspacePrune);
    projectsByRoot = await scanRoots();
  }

  // Full reload: everything, including the machine's editors + project kinds.
  async function refresh() {
    await Promise.all([
      sendSettingsMessage(Command.workspacePrune),
      editors.detect()
    ]);
    projectsByRoot = await scanRoots();
  }

  // A root that can't be read lists no projects.
  async function scan(root: string) {
    return await settledValue<ProjectEntry[]>({
      promise: sendMessage(Command.workspaceScan, { root }),
      fallback: []
    });
  }

  // The create-form's chosen root, owned here so selecting a root anywhere in
  // the picker (adding one in Root folders) fills the create location too.
  let createRoot = $state("");

  // The Root folders section instance - the create-form's "New root folder…"
  // option jumps to its add field through this handle.
  let rootsSection = $state<{ focusAddRoot: () => void } | null>(null);

  function isOwned(path: string) {
    // Temp dirs are ADE-created even if predating owned-workspace tracking.
    return settings.ownedWorkspaces.includes(path) || isTemporaryWorkspace(path);
  }

  // Workspace lifecycle (rename/move/delete + the inline-rename form
  // state) - one instance shared by the Recent rows and the root-project rows.
  const lifecycle = createWorkspaceLifecycle({
    isOwned,
    onmove: target => onmove(target),
    onrename: target => onrename(target),
    ondelete: path => ondelete(path),
    ondeleteproject: path => ondeleteproject(path),
    onrenamefolder: target => onrenamefolder(target),
    openWindows: () => sendMessage(Command.windowList),
    // A rename/move/delete changes the project lists, not the installed editors -
    // so catch the lists up cheaply, without re-probing the machine for editors.
    refresh: refreshProjects
  });

  // ── Watching the folders behind the rows ───────────────────────────────────
  // The page shouldn't go stale while it's open: a workspace deleted in Explorer
  // or from a terminal has to leave the list too. Re-armed whenever the watched
  // list changes; the backend replaces the previous set. Nothing downstream
  // waits on it, so it's a genuine fire-and-forget.
  $effect(() => {
    sendMessage(Command.watchDirs, { dirs: watchedDirectories(settings) });
  });

  // The filesystem answers in bursts (one delete lands as several events), so the
  // rescan is debounced - one refresh per settled change, not one per event.
  let rescan: ReturnType<typeof setTimeout> | undefined;
  let unlisten: (() => void) | undefined;

  onMount(async () => {
    await refresh();
    createRoot ||= popularRoot(settings);
    isLoaded = true;
    unlisten = await onMessage(EventType.directoriesChanged, () => {
      clearTimeout(rescan);
      // A folder appearing/vanishing under a watched root only moves projects
      // around - the light refresh, not a full editor re-detect.
      rescan = setTimeout(refreshProjects, RESCAN_DEBOUNCE_MILLISECONDS);
    });
  });

  // Which projects other windows hold, kept live so their rows never offer a
  // folder rename this window couldn't carry out.
  let unlistenWindows: (() => void) | undefined;
  onMount(async () => {
    await lifecycle.loadOpenWindows();
    unlistenWindows = await onMessage(EventType.windowsChanged, () => lifecycle.loadOpenWindows());
  });

  onDestroy(() => {
    clearTimeout(rescan);
    unlisten?.();
    unlistenWindows?.();
    // Let the folders go: nothing should hold a handle on them once the picker is
    // no longer on screen.
    sendMessage(Command.watchDirs, { dirs: [] });
  });
</script>

<div class="picker">
  <div class="inner">
    <header>
      <BrandMark />
      <h1>Open a project</h1>
      <p class="lede">
        Pick up where you left off, drop into a throwaway workspace, or point
        PADE at your code
      </p>
    </header>

    <!-- Painted only once the first settings load lands, so every control is
         born prefilled with the persisted preferences (no defaults flash). -->
    {#if isLoaded}
      <div class="sections">
        <HomeSection
          {agents}
          defaultAgent={settings.defaultAgent}
          labels={settings.labels}
          onnewroot={() => rootsSection?.focusAddRoot()}
          {onopen}
          {projectsByRoot}
          recentProjects={settings.recentProjects}
          roots={settings.roots}
          bind:createIn={createRoot}
        />

        <OnLaunchSection
          onainaming={async on => await updatePrefs({ aiSessionNaming: on })}
          onautoname={async on => await updatePrefs({ autoNameTemp: on })}
          onstartmode={async mode => await updatePrefs({ startMode: mode })}
          ontempcleanup={async days => {
            await updatePrefs({ tempCleanupDays: days });
            await refreshProjects();
          }}
          prefs={settings.prefs}
        />

        <RecentSection
          ides={editors.ides}
          labels={settings.labels}
          {lifecycle}
          onclear={async () => await sendSettingsMessage(Command.workspaceClearRecent)}
          {onopen}
          recentProjects={settings.recentProjects}
        />

        <AgentsSection
          {agents}
          defaultAgent={settings.defaultAgent}
          onpick={async agentId => await sendSettingsMessage(Command.setDefaultAgent, { agent: agentId })}
        />

        <EditorsSection
          ides={editors.ides}
          kindOptions={editors.kindOptions}
          kinds={editors.kinds}
          onaddeditor={path => editors.add(path)}
          onfallback={async editorId => await updatePrefs({ ideFallback: editorId })}
          onremoveeditor={id => editors.remove(id)}
          onrescan={() => editors.rescan()}
          onrule={async ({ kind, editorId }) => await updatePrefs({
            ideRules: {
              ...settings.prefs.ideRules,
              [kind]: editorId
            }
          })}
          prefs={settings.prefs}
        />

        <RootsSection
          bind:this={rootsSection}
          ides={editors.ides}
          {lifecycle}
          onadd={async (path, { create }) => {
            // Only an `added` outcome carries settings to adopt and scans the new
            // root - `missing`/`notADirectory` are handed back untouched so the
            // add-row can prompt to create the folder or show an error.
            const outcome = await requestAddRoot({
              path,
              create
            });
            if (outcome.status !== AddRootStatus.enum.added) {
              return outcome;
            }

            projectsByRoot = {
              ...projectsByRoot,
              [path]: await scan(path)
            };
            createRoot = path;
            return outcome;
          }}
          {onopen}
          onremove={async path => {
            await sendSettingsMessage(Command.workspaceRemoveRoot, { path });
            const { [path]: _drop, ...rest } = projectsByRoot;
            projectsByRoot = rest;

            if (createRoot === path) {
              createRoot = popularRoot(settings);
            }
          }}
          {projectsByRoot}
          roots={settings.roots}
        />
      </div>
    {/if}
  </div>
</div>

<!-- The lifecycle's prompts, raised from either list's row menu - outside .picker
     so they never scroll with the page. -->
<WorkspaceDialogs labels={settings.labels} {lifecycle} />

<style>
  .picker {
    flex: 1;

    /* Programmatic jumps (focusing the add-root field) glide instead of snap;
       focus() scrolls with behavior "auto", which follows this property. */
    scroll-behavior: smooth;
    overflow-y: auto;
    block-size: 100%;
    min-inline-size: 0;
    background: radial-gradient(120% 100% at 50% 0%, var(--surface-1), var(--surface));
  }

  @media (prefers-reduced-motion: reduce) {
    .picker {
      scroll-behavior: auto;
    }
  }

  .inner {
    display: flex;
    flex-direction: column;
    gap: 28px;
    inline-size: min(680px, 100%);
    margin-inline: auto;
    padding-block: 48px 80px;
    padding-inline: 24px;
    animation: rise 420ms var(--ease);
  }

  /* The settings-fed sections, mounted together once loaded - same column
     rhythm as .inner, rising in as one block. */
  .sections {
    display: flex;
    flex-direction: column;
    gap: 28px;
    animation: rise 420ms var(--ease);
  }

  header {
    animation: rise 300ms var(--ease);

    h1 {
      margin-block: 10px 0;
      margin-inline: 0;
      font-weight: 800;
      font-size: clamp(1.5rem, 4vw, 2.25rem);
      letter-spacing: -0.02em;
      text-wrap: balance;
    }

    .lede {
      max-inline-size: 52ch;
      margin-block: 8px 0;
      margin-inline: 0;
      color: var(--on-surface-variant);
    }
  }

  /* Shared page chrome (eyebrows, base fields/buttons, rows, kebab menus) lives
     in picker/chrome.css so the section components share one copy. */

  /* The home composer lives in picker/HomeSection.svelte. */

  /* "On launch" toggles live in picker/OnLaunchSection.svelte. */

  /* The Recent list lives in picker/RecentSection.svelte. */

  /* Default-agent chips live in picker/AgentsSection.svelte. */

  /* Root folders live in picker/RootsSection.svelte. */

  /* Editor-rules rows live in picker/EditorsSection.svelte. */

  /* The delete + relabel prompts live in picker/WorkspaceDialogs.svelte. */
</style>
