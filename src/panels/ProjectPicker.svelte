<script lang="ts">
  import BrandMark from "@/lib/BrandMark.svelte";
  import { dirs, ide, workspace } from "@/lib/bridge";
  import ConfirmDialog from "@/lib/ConfirmDialog.svelte";
  import { displayName, isTemporaryWorkspace, parentDir } from "@/lib/paths";
  import { StartMode } from "@/lib/types";
  import type { Agent, Ide, ProjectEntry, Settings } from "@/lib/types";
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
    onopen,
    onmove,
    onrename,
    ondelete
  }: {
    agents: Agent[];
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
    [settings, ides, currentKind] = await Promise.all([
      // prune, not settings: a folder deleted outside PADE is forgotten here, so
      // its row leaves the page (collapsing out) instead of lingering as a link
      // to nothing.
      workspace.prune(),
      ide.detect(),
      ide.projectKind().catch(() => null)
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

  async function addRoot(path: string) {
    settings = await workspace.addRoot(path);
    projectsByRoot = {
      ...projectsByRoot,
      [path]: await scan(path)
    };
  }
  async function removeRoot(path: string) {
    settings = await workspace.removeRoot(path);
    const { [path]: _drop, ...rest } = projectsByRoot;
    projectsByRoot = rest;
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

    <QuickStartSection {onopen} roots={settings.roots} />

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
      onaddeditor={addEditor}
      onfallback={setEditorFallback}
      onrule={setEditorRule}
      prefs={settings.prefs}
    />

    <RootsSection
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
    overflow-y: auto;
    block-size: 100%;
    background: radial-gradient(120% 100% at 50% 0%, var(--surface-1), var(--surface));
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
