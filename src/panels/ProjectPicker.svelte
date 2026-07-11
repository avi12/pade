<script lang="ts">
  import BrandMark from "@/lib/BrandMark.svelte";
  import { ide, workspace } from "@/lib/bridge";
  import { isTemporaryWorkspace } from "@/lib/paths";
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
  import { onMount } from "svelte";

  // Shown when the app wasn't launched inside a project. Manage root folders,
  // browse the projects inside them, open one, or create a new one — then hand
  // the chosen project path (and optional first prompt) back to the app.
  const {
    agents,
    onopen,
    onmove,
    onrename
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

  async function refresh() {
    [settings, ides, currentKind] = await Promise.all([
      workspace.settings(),
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
    applySettings(next) {
      settings = next;
    },
    refresh
  });

  onMount(() => void refresh());
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
</style>
