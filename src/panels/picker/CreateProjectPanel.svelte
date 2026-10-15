<script lang="ts">
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { realAgents as toRealAgents } from "@/lib/types";
  import type { Agent, OpenTarget } from "@/lib/types";
  import { childPath } from "@/lib/util/paths";
  import { nameError, parseInput, ProjectName } from "@/lib/validate";
  import AgentChips from "@/panels/picker/AgentChips.svelte";
  import type { CreateTarget } from "@/panels/picker/new-project";
  import { startTempWorkspace, submitNewProject } from "@/panels/picker/new-project";
  import ProjectLocationField from "@/panels/picker/ProjectLocationField.svelte";

  // Get started's New tab: create a project in a root, or fall through to a
  // throwaway temp workspace when the name stays blank. The chosen project path
  // and the optional first prompt go back to the app through `onopen`.
  let { roots, agents, defaultAgent, onopen, onnewroot, createIn = $bindable("") }: {
    roots: string[];
    agents: Agent[];
    defaultAgent: string | null;
    onopen: (target: OpenTarget) => void;
    onnewroot: () => void;
    createIn?: string;
  } = $props();

  // The installed coding agents (the shell fallback isn't a choice), and whether
  // there's more than one - the Agent row only earns its space when a pick is
  // actually on offer. The chosen agent defaults to the workspace default (or the
  // best-installed), overridable per-create through the chips.
  const realAgents = $derived(toRealAgents(agents));
  const newAgentMulti = $derived(realAgents.length > 1);
  let chosenAgent = $state<string | null>(null);
  const selectedAgent = $derived(chosenAgent ?? defaultAgent ?? realAgents[0]?.id ?? null);

  let createName = $state("");
  let createPrompt = $state("");

  // Live name validation: surface the schema's message and gate the submit on
  // the same check, so an invalid name can't reach a create() that would
  // silently no-op. An empty name is fine - it means "temp workspace".
  const createNameError = $derived(nameError(createName));
  const hasTypedName = $derived(createName.trim().length > 0);

  // Live disk check on the create target (probing the filesystem, never a
  // regex): the backend's create_dir_all succeeds on an existing folder, so
  // without this a name colliding with a real project would silently open that
  // project as if freshly created. Anything already on disk under that name -
  // directory or file - disables the create with the reason shown inline. The
  // probe fires on every keystroke (it's one local stat, cheap) and its answer
  // is tagged with the path it described, so an out-of-order reply never gates
  // the current text.
  let createTarget = $state<CreateTarget>({
    path: "",
    isDir: false,
    isFile: false
  });
  const createTargetPath = $derived.by(() => {
    const name = parseInput({
      schema: ProjectName,
      raw: createName
    });
    if (!createIn || !name) {
      return "";
    }

    return childPath({
      parent: createIn,
      name
    });
  });
  $effect(() => {
    const target = createTargetPath;
    if (target === "") {
      createTarget = {
        path: "",
        isDir: false,
        isFile: false
      };
      return;
    }

    async function probeTarget() {
      try {
        const probe = await sendMessage(Command.workspaceProbePath, { path: target });
        // Only believe an answer that describes the text still in the field.
        if (target !== createTargetPath) {
          return;
        }

        createTarget = {
          path: target,
          isDir: probe.isDir,
          isFile: probe.isFile
        };
      } catch {
        // An unreachable probe leaves the plain create path - the backend
        // still validates the final create.
        createTarget = {
          path: target,
          isDir: false,
          isFile: false
        };
      }
    }
    probeTarget();
  });
  const createTargetSettled = $derived(
    createTargetPath !== "" && createTarget.path === createTargetPath
  );
  const createTargetExists = $derived(createTargetSettled && createTarget.isDir);
  const createTargetIsFile = $derived(createTargetSettled && createTarget.isFile);

  // The settle state never gates the button - that would flash it disabled on
  // every keystroke until the debounced probe answers. Typing stays enabled;
  // the submit re-probes authoritatively before creating.
  const createDisabled = $derived(
    hasTypedName &&
    (!createIn || createNameError !== null || createTargetExists || createTargetIsFile)
  );
</script>

<form
  class="panel"
  onsubmit={async e => {
    e.preventDefault();
    await submitNewProject({
      root: createIn,
      rawName: createName,
      rawPrompt: createPrompt,
      agent: selectedAgent,
      onopen,
      onprobe: target => (createTarget = target)
    });
  }}
>
  <p class="hint">
    Name a folder to create a project - or leave the name blank for a
    throwaway temp workspace, auto-named once the agent starts working.
  </p>

  <ProjectLocationField
    isExistingFolder={createTargetExists}
    isTakenByFile={createTargetIsFile}
    nameError={createNameError}
    {onnewroot}
    {roots}
    bind:createIn
    bind:name={createName}
  />

  <div class="new-project-field">
    <label class="new-project-label" for="new-project-prompt">
      First prompt <span class="new-project-optional">- optional</span>
    </label>
    <textarea
      id="new-project-prompt"
      class="new-project-prompt"
      placeholder="e.g. scaffold a SvelteKit app with Tailwind"
      rows="2"
      bind:value={createPrompt}
    ></textarea>
  </div>

  {#if newAgentMulti}
    <div class="new-project-field">
      <span class="new-project-label">Agent</span>
      <AgentChips
        agents={realAgents}
        ariaLabel="Agent to launch"
        isCompact
        onpick={id => (chosenAgent = id)}
        selected={selectedAgent}
      />
    </div>
  {/if}

  <div class="actions">
    <button class="go" disabled={createDisabled} type="submit">
      Create &amp; open
    </button>
    <button
      class="temp-link"
      onclick={async () => await startTempWorkspace({
        rawPrompt: createPrompt,
        agent: selectedAgent,
        onopen
      })}
      type="button"
    >
      …or start a throwaway temp workspace
    </button>
  </div>
</form>

<style>
  .new-project-prompt {
    inline-size: 100%;
    padding: 9px 12px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-2);
    color: var(--on-surface);
    font: inherit;
    font-size: 13px;
    line-height: 1.5;
    resize: vertical;

    /* theme.css rings every `:focus-visible` two pixels OUTSIDE the box, which
       on a field that already carries a frame draws a second box around the
       first. Cut the ring into the field's own edge instead: the plate reads as
       one shape, thickened while focused - and the glow, which would ring the
       outside again, stays off. */
    &:focus-visible {
      outline-offset: -2px;
      box-shadow: none;
    }
  }

  /* Quiet text-button escape hatch beside the primary action. */
  .temp-link {
    padding: 6px 0;
    border: none;
    background: transparent;
    color: var(--on-surface-variant);
    font: inherit;
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
    transition: color 150ms var(--ease);

    &:hover {
      color: var(--primary);
      filter: none;
    }
  }
</style>
