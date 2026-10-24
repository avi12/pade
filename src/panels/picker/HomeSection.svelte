<script lang="ts">
  import { agentIconName } from "@/lib/agent/agent-icon";
  import Icon from "@/lib/Icon.svelte";
  import type { Agent, OpenTarget, ProjectEntry } from "@/lib/types";
  import { realAgents as toRealAgents } from "@/lib/types";
  import { displayName, projectRootName } from "@/lib/util/paths";
  import { nameError } from "@/lib/validate";
  import RootedName from "@/lib/workspace/RootedName.svelte";
  import { ProjectChoice, projectTriggerLabel, resolveHomeStart } from "@/panels/picker/home-composer";
  import HomeAgentMenu from "@/panels/picker/HomeAgentMenu.svelte";
  import HomeProjectMenu from "@/panels/picker/HomeProjectMenu.svelte";
  import HomeStartButton from "@/panels/picker/HomeStartButton.svelte";
  import type { CreateTarget } from "@/panels/picker/new-project";
  import ProjectLocationField from "@/panels/picker/ProjectLocationField.svelte";

  // Home: the picker's composer card - one prompt field with a project menu on
  // its left and an LLM menu on its right, then Start. The project menu lists
  // recent and scanned projects plus a "New project…" entry that reveals the
  // name + location form inline; starting there creates (saves) the project
  // and opens it with the typed first prompt, while an existing project opens
  // straight into the workspace (recorded in recent history).
  let {
    agents,
    defaultAgent,
    roots,
    recentProjects,
    labels,
    projectsByRoot,
    onopen,
    onnewroot,
    createIn = $bindable("")
  }: {
    agents: Agent[];
    defaultAgent: string | null;
    roots: string[];
    recentProjects: string[];
    /** Friendly display labels per path. */
    labels: Record<string, string>;
    projectsByRoot: Record<string, ProjectEntry[]>;
    onopen: (target: OpenTarget) => void;
    onnewroot: () => void;
    createIn?: string;
  } = $props();

  const PROJECT_MENU_ID = "home-project-menu";
  const AGENT_MENU_ID = "home-agent-menu";
  const PROMPT_ID = "home-prompt";
  const NAME_FIELD_ID = "home-project-name";

  // The installed coding agents (the shell fallback isn't a choice).
  const realAgents = $derived(toRealAgents(agents));
  let chosenAgent = $state<string | null>(null);
  const selectedAgent = $derived(chosenAgent ?? defaultAgent ?? realAgents[0]?.id ?? null);
  const selectedAgentLabel = $derived(realAgents.find(agent => agent.id === selectedAgent)?.label);

  let prompt = $state("");
  let selectedPath = $state("");
  const selectedName = $derived(
    selectedPath ? displayName({
      path: selectedPath,
      labels
    }) : null
  );

  // The menu's project options: recents first, then every scanned project not
  // already listed - one home for the dedupe so the two groups never overlap.
  const recentOptions = $derived(
    recentProjects.map(path => ({
      path,
      name: displayName({
        path,
        labels
      })
    }))
  );
  const scannedOptions = $derived(
    Object.values(projectsByRoot)
      .flat()
      .filter(entry => !recentProjects.includes(entry.path))
      .map(entry => ({
        path: entry.path,
        name: displayName({
          path: entry.path,
          labels
        })
      }))
  );
  // The menu renders every non-empty group through one item block, so the
  // recent and scanned rows can never drift apart.
  const menuGroups = $derived.by(() => {
    const groups: {
      head: string;
      isSeparated: boolean;
      options: {
        path: string;
        name: string;
      }[];
    }[] = [];
    if (recentOptions.length > 0) {
      groups.push({
        head: "Recent",
        isSeparated: false,
        options: recentOptions
      });
    }

    if (scannedOptions.length > 0) {
      groups.push({
        head: "All projects",
        isSeparated: recentOptions.length > 0,
        options: scannedOptions
      });
    }

    return groups;
  });

  // What the project menu picked. A new project shows the inline create form:
  // name + location (root). Collisions surface from the submit's own
  // authoritative probe (tagged with the name + root it described, so typing on
  // never shows a stale verdict).
  let choice = $state<ProjectChoice>(ProjectChoice.Existing);
  let createName = $state("");
  const trimmedName = $derived(createName.trim());
  const createNameError = $derived(nameError(createName));
  let lastProbe = $state<(CreateTarget & {
    root: string;
    name: string;
  }) | null>(null);
  const createTargetExists = $derived(
    lastProbe !== null &&
    lastProbe.name === trimmedName &&
    lastProbe.root === createIn &&
    (lastProbe.isDir || lastProbe.isFile)
  );

  const resolution = $derived(
    resolveHomeStart({
      selectedPath,
      choice,
      root: createIn,
      rawName: createName,
      rawPrompt: prompt,
      agent: selectedAgent
    })
  );
  // The chosen project's root before its name, as the menu rows read; a new
  // project or a throwaway workspace has none yet.
  const triggerRoot = $derived(choice === ProjectChoice.Existing ? projectRootName(selectedPath) : "");
  const agentTriggerLabel = $derived(selectedAgentLabel ?? "No LLMs found");
  const isExistingFolder = $derived(createTargetExists && lastProbe?.isDir === true);
  const isTakenByFile = $derived(createTargetExists && lastProbe?.isFile === true);
  const triggerLabel = $derived(
    projectTriggerLabel({
      choice,
      typedName: trimmedName,
      selectedName
    })
  );
</script>

<section class="home" aria-label="Home">
  <h2>Home</h2>
  <div class="card">
    <label class="prompt-label" for={PROMPT_ID}>First prompt</label>
    <textarea
      id={PROMPT_ID}
      class="prompt"
      placeholder="What should the agent do first?"
      rows="3"
      bind:value={prompt}
    ></textarea>

    <div class="row">
      <span style:anchor-name="--{PROJECT_MENU_ID}" class="menu-host">
        <button
          class="select field-trigger"
          aria-label="Project"
          popovertarget={PROJECT_MENU_ID}
          type="button"
        >
          <Icon name="folder" size={15} />
          <span class="current"><RootedName name={triggerLabel} root={triggerRoot} /></span>
          <span class="caret" aria-hidden="true">▾</span>
        </button>
        <HomeProjectMenu
          {choice}
          groups={menuGroups}
          menuId={PROJECT_MENU_ID}
          nameFieldId={NAME_FIELD_ID}
          onnew={root => {
            selectedPath = "";

            if (root) {
              createIn = root;
            }

            choice = ProjectChoice.New;
          }}
          onpick={path => {
            selectedPath = path;
            choice = ProjectChoice.Existing;
          }}
          ontemp={() => {
            selectedPath = "";
            choice = ProjectChoice.Temp;
          }}
          {roots}
          {selectedPath}
        />
      </span>

      <span style:anchor-name="--{AGENT_MENU_ID}" class="menu-host">
        <button
          class="select field-trigger"
          aria-label="LLM"
          disabled={realAgents.length === 0}
          popovertarget={AGENT_MENU_ID}
          type="button"
        >
          {#if selectedAgent}
            <Icon name={agentIconName(selectedAgent)} size={15} />
          {/if}
          <span class="current">{agentTriggerLabel}</span>
          <span class="caret" aria-hidden="true">▾</span>
        </button>
        <HomeAgentMenu
          agents={realAgents}
          menuId={AGENT_MENU_ID}
          onpick={agentId => (chosenAgent = agentId)}
          {selectedAgent}
        />
      </span>

      <HomeStartButton
        agent={selectedAgent}
        {onopen}
        onprobe={target =>
          (lastProbe = {
            ...target,
            root: createIn,
            name: trimmedName
          })}
        rawName={createName}
        rawPrompt={prompt}
        {resolution}
      />
    </div>

    {#if choice === ProjectChoice.New}
      <ProjectLocationField
        fieldId={NAME_FIELD_ID}
        {isExistingFolder}
        {isTakenByFile}
        menuId="home-root-menu"
        nameError={createNameError}
        {onnewroot}
        {roots}
        bind:createIn
        bind:name={createName}
      />
    {/if}
  </div>
</section>

<style>
  /* Home composer - one card: the prompt field over a row with the project
     menu on the left, the LLM menu on the right, and Start last. */
  .card {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 18px 20px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-large);
    background: var(--surface-1);
  }

  .prompt-label {
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 0.75rem;
  }

  .prompt {
    inline-size: 100%;
    resize: vertical;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }

  /* The project / LLM triggers read as fields (the shared .field-trigger
     look), labels that ellipsis instead of pushing Start off the row. */
  .select {
    display: inline-flex;
    gap: 8px;
    align-items: center;
    min-inline-size: 0;
    max-inline-size: 100%;
    padding: 10px 14px;
    transition: border-color 150ms var(--ease);

    &:disabled {
      opacity: 60%;
      cursor: default;
    }

    .current {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .caret {
      flex: none;
      font-size: 0.5625rem;
      opacity: 70%;
    }
  }

  /* The project trigger takes the free width so its path prints in full. */
  .row .menu-host:first-child {
    display: inline-flex;
    flex: 1 1 180px;
    min-inline-size: 0;

    .select {
      flex: 1;
      min-inline-size: 0;
    }
  }
</style>
