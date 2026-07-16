<script lang="ts">
  import BrandMark from "@/lib/BrandMark.svelte";
  import { dirs, ide, workspace } from "@/lib/bridge";
  import ConfirmDialog from "@/lib/ConfirmDialog.svelte";
  import { displayName, isTemporaryWorkspace, normalizePath, parentDir } from "@/lib/paths";
  import { AddRootStatus, StartMode } from "@/lib/types";
  import type {
    AddRootOutcome,
    Agent,
    EditorKind,
    Ide,
    ProjectEntry,
    Settings
  } from "@/lib/types";
  import AgentsSection from "@/panels/picker/AgentsSection.svelte";
  import "@/panels/picker/chrome.css";
  import EditorsSection from "@/panels/picker/EditorsSection.svelte";
  import { createWorkspaceLifecycle } from "@/panels/picker/lifecycle.svelte";
  import OnLaunchSection from "@/panels/picker/OnLaunchSection.svelte";
  import QuickStartSection from "@/panels/picker/QuickStartSection.svelte";
  import RecentSection from "@/panels/picker/RecentSection.svelte";
  import RootsSection from "@/panels/picker/RootsSection.svelte";
  import { onDestroy, onMount } from "svelte";

  // Shown when the app wasn't launched inside a project. Manage root folders,
  // browse the projects inside them, open one, or create a new one — then hand
  // the chosen project path (and optional first prompt) back to the app.
  const {
    agents,
    hasActiveProject,
    onopen,
    onmove,
    onrename,
    ondelete
  }: {
    agents: Agent[];
    /** Whether the picker was opened from an active project (the workspace). Only
        then is tagging "this project"'s editor rule meaningful — a bare launch or
        the onboarding step has no working project to point at. */
    hasActiveProject: boolean;
    onopen: (target: {
      path: string;
      initialPrompt?: string;
    }) => void;
    onmove: (target: {
      from: string;
      destDir: string;
    }) => Promise<string>;
    onrename: (target: {
      from: string;
      newName: string;
    }) => Promise<string>;
    /** Deletes through the app's relocator: kills the sessions holding the
     *  folder as cwd (else the OS blocks the removal), then removes it. */
    ondelete: (path: string) => Promise<Settings>;
  } = $props();

  let settings = $state<Settings>({
    roots: [],
    defaultAgent: null,
    projectAgents: {},
    recentProjects: [],
    ownedWorkspaces: [],
    labels: {},
    prefs: {}
  });
  let projectsByRoot = $state<Record<string, ProjectEntry[]>>({});
  let ides = $state<Ide[]>([]);
  // The project kinds the rules engine shows, straight from the backend registry
  // (its single home) in render/priority order.
  let kinds = $state<EditorKind[]>([]);
  // Editor ids that suit each project kind (kind → ordered), so a per-kind editor
  // menu offers only fitting editors rather than every installed one.
  let kindOptions = $state<Record<string, string[]>>({});
  // Primary detected kind of the current dir, so we can tag "this project"'s row.
  let currentKind = $state<string | null>(null);

  async function setEditorRule({ kind, editorId }: {
    kind: string;
    editorId: string;
  }) {
    settings = await workspace.setPrefs({
      ...settings.prefs,
      ideRules: {
        ...settings.prefs.ideRules,
        [kind]: editorId
      }
    });
  }
  async function setEditorFallback(editorId: string) {
    settings = await workspace.setPrefs({
      ...settings.prefs,
      ideFallback: editorId
    });
  }
  // Add an editor by executable path. The backend validates the executable and
  // persists it; we refresh the detected list so it appears in every menu.
  // Returns the new editor's label on success, or the rejection message.
  async function addEditor(path: string): Promise<{
    label: string;
  } | {
    error: string;
  }> {
    try {
      settings = await ide.addEditor(path);
      ides = await ide.detect();
      const added = settings.prefs.addedEditors?.find(editor => editor.path === path.trim());
      return { label: added?.label ?? "Editor" };
    } catch (error) {
      return { error: typeof error === "string" ? error : "Couldn’t add that editor." };
    }
  }

  async function refresh() {
    const detectedKind = hasActiveProject ? ide.projectKind().catch(() => null) : Promise.resolve(null);
    [settings, ides, kinds, kindOptions, currentKind] = await Promise.all([
      // prune, not settings: a folder deleted outside PADE is forgotten here, so
      // its row leaves the page (collapsing out) instead of lingering as a link
      // to nothing.
      workspace.prune(),
      ide.detect(),
      ide.kinds(),
      ide.kindOptions(),
      detectedKind
    ]);
    projectsByRoot = Object.fromEntries(
      await Promise.all(settings.roots.map(async root => [root, await scan(root)] as const))
    );
  }

  async function setStartMode(mode: StartMode) {
    settings = await workspace.setPrefs({
      ...settings.prefs,
      startMode: mode
    });
  }
  async function setAutoName(on: boolean) {
    settings = await workspace.setPrefs({
      ...settings.prefs,
      autoNameTemp: on
    });
  }

  function scan(root: string): Promise<ProjectEntry[]> {
    return workspace.scan(root).catch((): ProjectEntry[] => []);
  }

  // The create-form's chosen root, owned here so selecting a root anywhere in
  // the picker (adding one in Root folders) fills the create location too.
  let createRoot = $state("");

  // The Root folders section instance — the create-form's "New root folder…"
  // option jumps to its add field through this handle.
  let rootsSection = $state<{ focusAddRoot: () => void } | null>(null);

  // The form's best default root: a lone root wins outright; otherwise the one
  // most recent projects live in (ties keep the earlier root).
  function popularRoot(): string {
    const { roots, recentProjects } = settings;
    const [firstRoot] = roots;
    if (firstRoot === undefined || roots.length === 1) {
      return firstRoot ?? "";
    }

    const recentParents = recentProjects
      .map(parentDir)
      .filter((parent): parent is string => parent !== null)
      .map(normalizePath);
    function usage(root: string): number {
      return recentParents.filter(parent => parent === normalizePath(root)).length;
    }
    return roots.reduce((best, root) => (usage(root) > usage(best) ? root : best));
  }

  // Stays the single settings owner: only an `added` outcome adopts the returned
  // settings and scans the new root — `missing`/`notADirectory` are handed back
  // untouched so the add-row can prompt to create the folder or show an error.
  async function addRoot(path: string, { create }: {
    create: boolean;
  }): Promise<AddRootOutcome> {
    const outcome = await workspace.addRoot({
      path,
      create
    });
    if (outcome.status === AddRootStatus.enum.added) {
      settings = outcome.settings;
      projectsByRoot = {
        ...projectsByRoot,
        [path]: await scan(path)
      };

      createRoot = path;
    }

    return outcome;
  }
  async function removeRoot(path: string) {
    settings = await workspace.removeRoot(path);
    const { [path]: _drop, ...rest } = projectsByRoot;
    projectsByRoot = rest;

    if (createRoot === path) {
      createRoot = popularRoot();
    }
  }

  async function clearRecent() {
    settings = await workspace.clearRecent();
  }

  async function setMaster(agentId: string) {
    settings = await workspace.setDefaultAgent(agentId);
  }

  function isOwned(path: string): boolean {
    // Temp dirs are ADE-created even if predating owned-workspace tracking.
    return settings.ownedWorkspaces.includes(path) || isTemporaryWorkspace(path);
  }

  // Owned-workspace lifecycle (rename/move/delete + the inline-rename form
  // state) — one instance shared by the Recent rows and the root-project rows.
  const lifecycle = createWorkspaceLifecycle({
    isOwned,
    onmove: target => onmove(target),
    onrename: target => onrename(target),
    ondelete: path => ondelete(path),
    applySettings(next) {
      settings = next;
    },
    refresh
  });

  // ── Watching the folders behind the rows ───────────────────────────────────
  // The page shouldn't go stale while it's open: a workspace deleted in Explorer
  // or from a terminal has to leave the list too. We watch each row's PARENT (a
  // watch holds a handle on the folder it watches, and a handle on a project is
  // exactly what would stop that project from being deletable), plus every root,
  // whose children are the projects it lists.
  const watchedDirs = $derived([...new Set(
    [
      ...settings.roots,
      ...settings.roots.map(parentDir),
      ...settings.recentProjects.map(parentDir)
    ].filter((dir): dir is string => dir !== null)
  )]);

  // Re-armed whenever that list changes; the backend replaces the previous set.
  // Nothing downstream waits on it, so it's a genuine fire-and-forget.
  $effect(() => void dirs.watch(watchedDirs));

  // The filesystem answers in bursts (one delete lands as several events), so the
  // rescan is debounced — one refresh per settled change, not one per event.
  let rescan: ReturnType<typeof setTimeout> | undefined;
  let unlisten: (() => void) | undefined;

  function rescanSoon() {
    clearTimeout(rescan);
    rescan = setTimeout(refresh, 250);
  }

  onMount(async () => {
    await refresh();
    createRoot ||= popularRoot();
    unlisten = await dirs.onChange(rescanSoon);
  });

  onDestroy(() => {
    clearTimeout(rescan);
    unlisten?.();
    // Let the folders go: nothing should hold a handle on them once the picker is
    // no longer on screen.
    void dirs.watch([]);
  });
</script>

<div class="picker">
  <div class="inner">
    <header>
      <BrandMark />
      <h1>Open a project</h1>
      <p class="lede">
        Pick up where you left off, drop into a throwaway workspace, or point
        PADE at your code.
      </p>
    </header>

    <QuickStartSection
      onnewroot={() => rootsSection?.focusAddRoot()}
      {onopen}
      roots={settings.roots}
      bind:createIn={createRoot}
    />

    <OnLaunchSection onautoname={setAutoName} onstartmode={setStartMode} prefs={settings.prefs} />

    <RecentSection
      {ides}
      labels={settings.labels}
      {lifecycle}
      onclear={clearRecent}
      {onopen}
      recentProjects={settings.recentProjects}
    />

    <AgentsSection {agents} defaultAgent={settings.defaultAgent} onpick={setMaster} />

    <EditorsSection
      {currentKind}
      {ides}
      {kindOptions}
      {kinds}
      onaddeditor={addEditor}
      onfallback={setEditorFallback}
      onrule={setEditorRule}
      prefs={settings.prefs}
    />

    <RootsSection
      bind:this={rootsSection}
      {ides}
      {lifecycle}
      onadd={addRoot}
      {onopen}
      onremove={removeRoot}
      {projectsByRoot}
      roots={settings.roots}
    />
  </div>
</div>

<!-- Delete confirmation for an owned workspace, raised from either list's row
     menu. It lives here (outside .picker) as the lifecycle's single dialog. -->
{#if lifecycle.deleteTarget}
  <ConfirmDialog
    busy={lifecycle.deleting}
    busyLabel="Deleting…"
    confirmLabel="Delete workspace"
    danger
    error={lifecycle.deleteError}
    icon="trash"
    oncancel={() => lifecycle.cancelDelete()}
    onconfirm={async () => await lifecycle.confirmDelete()}
    title="Delete this workspace?"
  >
    <div class="delete-body">
      <p>The folder and everything inside it is removed from disk. This can’t be undone.</p>
      <p class="target">
        <span class="target-name">{displayName(lifecycle.deleteTarget, settings.labels)}</span>
        <code>{lifecycle.deleteTarget}</code>
      </p>
      <p class="tip">Hold <kbd>Shift</kbd> when clicking Delete to skip this next time.</p>
    </div>
  </ConfirmDialog>
{/if}

<style>
  .picker {
    /* Programmatic jumps (focusing the add-root field) glide instead of snap;
       focus() scrolls with behavior "auto", which follows this property. */
    scroll-behavior: smooth;
    overflow-y: auto;
    block-size: 100%;
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

  header {
    animation: rise 300ms var(--ease);

    h1 {
      margin-block: 10px 0;
      margin-inline: 0;
      font-weight: 800;
      font-size: clamp(24px, 4vw, 36px);
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

  /* The "Start something new" cards live in picker/QuickStartSection.svelte. */

  /* "On launch" toggles live in picker/OnLaunchSection.svelte. */

  /* The Recent list lives in picker/RecentSection.svelte. */

  /* Default-agent chips live in picker/AgentsSection.svelte. */

  /* Root folders live in picker/RootsSection.svelte. */

  /* Editor-rules rows live in picker/EditorsSection.svelte. */

  /* Body of the delete-confirmation dialog (its chrome is ConfirmDialog's). */
  .delete-body {
    p {
      margin: 0;
    }

    .target {
      display: flex;
      flex-direction: column;
      gap: 2px;
      margin-block-start: 14px;
      padding: 10px 12px;
      border-radius: var(--radius-medium);
      background: var(--surface-2);
    }

    .target-name {
      color: var(--on-surface);
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 13px;
    }

    code {
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 11px;
      overflow-wrap: anywhere;
    }

    .tip {
      margin-block-start: 12px;
      font-size: 12px;
    }

    kbd {
      padding-block: 2px;
      padding-inline: 6px;
      border-radius: var(--radius-small);
      background: var(--surface-3);
      color: var(--on-surface);
      font-family: var(--font-ui);
      font-weight: 600;
      font-size: 11px;
    }
  }
</style>
