<script lang="ts">
  import { agentIconName } from "@/lib/agent/agent-icon";
  import Icon from "@/lib/Icon.svelte";
  import type { Agent, OpenTarget, ProjectEntry } from "@/lib/types";
  import { realAgents as toRealAgents } from "@/lib/types";
  import { displayName } from "@/lib/util/paths";
  import { nameError } from "@/lib/validate";
  import { resolveHomeStart } from "@/panels/picker/home-composer";
  import { submitNewProject } from "@/panels/picker/new-project";
  import type { CreateTarget } from "@/panels/picker/new-project";
  import ProjectLocationField from "@/panels/picker/ProjectLocationField.svelte";
  import { tick } from "svelte";

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
  const hasOptions = $derived(recentOptions.length > 0 || scannedOptions.length > 0);
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

  // The inline create form: name + location (root), shown after picking
  // "New project…" in the menu. Collisions surface from the submit's own
  // authoritative probe (tagged with the name + root it described, so typing on
  // never shows a stale verdict).
  let isCreating = $state(false);
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
      isCreating,
      root: createIn,
      rawName: createName,
      rawPrompt: prompt,
      agent: selectedAgent
    })
  );
  let isStarting = $state(false);
  const startDisabled = $derived(resolution.kind === "none" || isStarting);
  const startLabel = $derived.by(() => {
    if (isStarting) {
      return "Starting";
    }

    return isCreating ? "Create and open" : "Start";
  });
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
          title={selectedPath || undefined}
          type="button"
        >
          <Icon name="folder" size={15} />
          <span class="current">
            {#if isCreating}
              {#if trimmedName}
                New: {trimmedName}
              {:else}
                New project
              {/if}
            {:else}
              {selectedName ?? "Choose a project"}
            {/if}
          </span>
          <span class="caret" aria-hidden="true">▾</span>
        </button>
        <ul id={PROJECT_MENU_ID} style:position-anchor="--{PROJECT_MENU_ID}" class="menu popover-menu" popover>
          {#each menuGroups as group (group.head)}
            <li class="head" class:separator={group.isSeparated} aria-hidden="true">{group.head}</li>
            {#each group.options as option (option.path)}
              {@const isPicked = !isCreating && selectedPath === option.path}
              <li>
                <button
                  class="menu-item"
                  class:picked={isPicked}
                  aria-current={isPicked}
                  onclick={() => {
                    selectedPath = option.path;
                    isCreating = false;
                  }}
                  popovertarget={PROJECT_MENU_ID}
                  popovertargetaction="hide"
                  title={option.path}
                  type="button"
                >
                  <span class="menu-item-text">{option.name}</span>
                  {#if isPicked}
                    <span class="tick" aria-hidden="true">✓</span>
                  {/if}
                </button>
              </li>
            {/each}
          {/each}
          {#if !hasOptions}
            <li class="none">No projects yet - name one below</li>
          {/if}
          <li class:separator={hasOptions}>
            <button
              class="menu-item create-option"
              onclick={async e => {
                // Hide the menu imperatively (like RootSelect's jump-to-add):
                // the declarative hide would run after this handler and cancel
                // the focus into the name field.
                e.currentTarget.closest("ul")?.hidePopover();
                selectedPath = "";
                isCreating = true;
                await tick();
                document.getElementById("home-project-name")?.focus();
              }}
              type="button"
            >
              <Icon name="folderPlus" size={15} />
              <span>New project…</span>
            </button>
          </li>
        </ul>
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
          <span class="current">{selectedAgentLabel ?? "No LLMs found"}</span>
          <span class="caret" aria-hidden="true">▾</span>
        </button>
        <ul id={AGENT_MENU_ID} style:position-anchor="--{AGENT_MENU_ID}" class="menu popover-menu" popover>
          {#each realAgents as agent (agent.id)}
            {@const isPicked = selectedAgent === agent.id}
            <li>
              <button
                class="menu-item"
                class:picked={isPicked}
                aria-current={isPicked}
                onclick={() => (chosenAgent = agent.id)}
                popovertarget={AGENT_MENU_ID}
                popovertargetaction="hide"
                type="button"
              >
                <Icon name={agentIconName(agent.id)} size={15} />
                <span class="menu-item-text">{agent.label}</span>
                {#if isPicked}
                  <span class="tick" aria-hidden="true">✓</span>
                {/if}
              </button>
            </li>
          {/each}
        </ul>
      </span>

      <button
        class="go"
        disabled={startDisabled}
        onclick={async () => {
          if (startDisabled) {
            return;
          }

          if (resolution.kind === "open") {
            onopen(resolution.target);
            return;
          }

          if (resolution.kind === "create") {
            isStarting = true;
            try {
              await submitNewProject({
                root: resolution.root,
                rawName: createName,
                rawPrompt: prompt,
                agent: selectedAgent,
                onopen,
                onprobe: target =>
                  (lastProbe = {
                    ...target,
                    root: resolution.root,
                    name: trimmedName
                  })
              });
            } finally {
              isStarting = false;
            }
          }
        }}
        type="button"
      >
        {startLabel}
      </button>
    </div>

    {#if isCreating}
      <ProjectLocationField
        fieldId="home-project-name"
        isExistingFolder={createTargetExists && (lastProbe?.isDir ?? false)}
        isTakenByFile={createTargetExists && (lastProbe?.isFile ?? false)}
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
    font-size: 12px;
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
      font-size: 9px;
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

  .go {
    flex: none;
  }

  /* The create action reads primary among the plain project rows. */
  .create-option {
    color: var(--primary);
    font-weight: 600;
  }

  .none {
    padding: 8px 10px;
    color: var(--on-surface-variant);
    font-size: 13px;
  }
</style>
