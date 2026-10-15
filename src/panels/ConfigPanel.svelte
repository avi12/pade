<script lang="ts">
  import { globalNaming, projectNamingChoice, withProjectNamingChoice } from "@/lib/agent/ai-naming-scope";
  import { collectVars } from "@/lib/code/colors";
  import ColorText from "@/lib/code/ColorText.svelte";
  import { isMarkdownPath } from "@/lib/feed/preview";
  import Icon, { type IconName } from "@/lib/Icon.svelte";
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { UI_SCALE_MAXIMUM, UI_SCALE_MINIMUM, UI_SCALE_STEP } from "@/lib/settings/prefs-bounds";
  import { chooseThemeMode, effective, updatePrefs } from "@/lib/settings/prefs.svelte";
  import { prefs } from "@/lib/settings/settings.svelte";
  import { setPanelHeader } from "@/lib/stores/ui/sidePanel.svelte";
  import { type Agent, type ConfigFile, ThemeMode } from "@/lib/types";
  import Checkbox from "@/lib/ui/Checkbox.svelte";
  import Segmented from "@/lib/ui/Segmented.svelte";
  import { MAXIMUM_HANDOFF_PERCENTAGE, MINIMUM_HANDOFF_PERCENTAGE } from "@/lib/usage/context-level";
  import { formatPercent } from "@/lib/util/format";
  import { HandoffPercent, parseInput } from "@/lib/validate";
  import AgentModelDefaults from "@/panels/config/AgentModelDefaults.svelte";
  import TerminalColours from "@/panels/config/TerminalColours.svelte";
  import UsagePacingSettings from "@/panels/config/UsagePacingSettings.svelte";

  // Only the config files relevant to the active agent are listed. `project` is
  // what scopes the per-project settings below to this window's workspace.
  const { agent, agents, project }: {
    agent: string;
    /** The installed agents - the only ones a starting model is offered for. */
    agents: Agent[];
    project: string;
  } = $props();

  // ── Session tabs card ───────────────────────────────────────────────────────
  // The project scope of "name session tabs with AI". Three states, because the
  // absence of a project choice is itself a state: it follows the picker's global
  // default, and picking `Global` is the only way back to it.
  const namingChoice = $derived(
    projectNamingChoice({
      prefs,
      project
    })
  );
  const globalNamingWord = $derived.by(() => {
    if (globalNaming(prefs)) {
      return "on";
    }

    return "off";
  });
  const namingScopes = [
    {
      value: undefined,
      label: "Global"
    },
    {
      value: true,
      label: "On"
    },
    {
      value: false,
      label: "Off"
    }
  ] as const satisfies readonly {
    value: boolean | undefined;
    label: string;
  }[];

  async function chooseNamingScope(choice: boolean | undefined) {
    await updatePrefs({
      aiSessionNamingProjects: withProjectNamingChoice({
        prefs,
        project,
        choice
      })
    });
  }

  // ── Appearance card ─────────────────────────────────────────────────────────
  // Theme mode, terminal font, and UI zoom - each bound to a persisted pref via
  // updatePrefs (merge → apply → save through the shared store).
  const themeOptions = [
    {
      value: ThemeMode.enum.light,
      label: "Light",
      icon: "sun"
    },
    {
      value: ThemeMode.enum.dark,
      label: "Dark",
      icon: "moon"
    },
    {
      value: ThemeMode.enum.system,
      label: "Auto",
      icon: "monitor"
    },
    {
      value: ThemeMode.enum.cyberpunk,
      label: "Cyberpunk",
      icon: "bolt"
    }
  ] as const satisfies readonly {
    value: ThemeMode;
    label: string;
    icon: IconName;
  }[];

  // `value` is the persisted monoFont string ("" = system default); `family` is
  // the CSS stack the preview renders in that face.
  const fontOptions = [
    {
      name: "JetBrains Mono",
      value: "JetBrains Mono",
      family: "\"JetBrains Mono\", ui-monospace, monospace"
    },
    {
      name: "System Mono",
      value: "",
      family: "ui-monospace, monospace"
    },
    {
      name: "Courier",
      value: "Courier New",
      family: "\"Courier New\", monospace"
    },
    {
      name: "Fira Code",
      value: "Fira Code",
      family: "\"Fira Code\", ui-monospace, monospace"
    },
    {
      name: "Share Tech Mono",
      value: "Share Tech Mono",
      family: "\"Share Tech Mono\", ui-monospace, monospace"
    }
  ] as const satisfies readonly {
    name: string;
    value: string;
    family: string;
  }[];

  const selectedMonospaceFont = $derived(effective.monoFontName);

  const scalePercent = $derived(formatPercent(effective.uiScale * 100));

  async function stepScale(delta: number) {
    const clamped = Math.min(UI_SCALE_MAXIMUM, Math.max(UI_SCALE_MINIMUM, effective.uiScale + delta));
    // Round to the step grid so accumulated float drift never leaks into the pref.
    await updatePrefs({ uiScale: Math.round(clamped * 100) / 100 });
  }

  // Slider and number box both funnel through the input schema - the slider
  // can only emit in-band values, but the free-typed number is a trust
  // boundary like any other field.
  async function applyHandoffPercentage(raw: string) {
    const percent = parseInput({
      schema: HandoffPercent,
      raw
    });
    if (percent === null) {
      return;
    }

    await updatePrefs({ handoffPct: percent });
  }

  let files = $state<ConfigFile[]>([]);
  let selected = $state<ConfigFile | null>(null);
  let content = $state("");
  // Trace var(--x) swatches against the file's own token definitions first.
  const fileVars = $derived(collectVars(content));

  async function open(file: ConfigFile) {
    if (!file.exists) {
      return;
    }

    selected = file;
    // The read can resolve out of order relative to a later selection; only
    // apply it while this file is still the selected one.
    const requested = file;
    try {
      const text = await sendMessage(Command.configRead, { rel: requested.rel });
      if (selected?.rel !== requested.rel) {
        return;
      }

      content = text;
    } catch {
      if (selected?.rel !== requested.rel) {
        return;
      }

      content = "";
    }
  }

  // `agent` is a reactive prop and the panel is not remounted when the active
  // agent changes, so reload the file list whenever it does. Capture the agent
  // to discard responses from a superseded agent.
  $effect(() => {
    const requestedAgent = agent;
    selected = null;
    content = "";
    files = [];
    (async () => {
      const listed = await sendMessage(Command.configList, { agent: requestedAgent });
      if (requestedAgent !== agent) {
        return;
      }

      files = listed;
      const first = listed.find(file => file.exists);
      if (first) {
        await open(first);
      }
    })();
  });

  // Config has no count or refresh - clear those slots in the shared header.
  $effect(() => {
    setPanelHeader({
      count: null,
      refresh: null
    });
  });
</script>

<div class="configuration">
  <div class="scroll">
    <section class="appearance">
      <h3 class="card-label">Appearance</h3>

      <div class="field">
        <span class="field-label">Theme</span>
        <div class="theme-choices">
          <Segmented
            chosen={effective.themeMode}
            label="Theme"
            onchoose={chooseThemeMode}
            options={themeOptions}
          />
        </div>
      </div>

      <div class="field">
        <span class="field-label">Terminal font</span>
        <div class="font-grid">
          {#each fontOptions as option (option.name)}
            <button
              class="font-card"
              class:on={selectedMonospaceFont === option.value}
              aria-pressed={selectedMonospaceFont === option.value}
              onclick={() => updatePrefs({ monoFont: option.value })}
            >
              <span class="font-text">
                <span class="font-name">{option.name}</span>
                <span style:font-family={option.family} class="font-preview">Ag0 &lt;/&gt; 1l</span>
              </span>
              {#if selectedMonospaceFont === option.value}
                <Icon name="check" />
              {/if}
            </button>
          {/each}
        </div>
      </div>

      <TerminalColours />

      <div class="field-row">
        <span class="field-text">
          <span class="field-label">Font size</span>
          <span class="field-hint">Applies to UI and terminal</span>
        </span>
        <div class="stepper">
          <button
            class="step"
            aria-label="Decrease font size"
            disabled={effective.uiScale <= UI_SCALE_MINIMUM}
            onclick={() => stepScale(-UI_SCALE_STEP)}
          >A−</button>
          <output class="scale-value">{scalePercent}</output>
          <button
            class="step step-up"
            aria-label="Increase font size"
            disabled={effective.uiScale >= UI_SCALE_MAXIMUM}
            onclick={() => stepScale(UI_SCALE_STEP)}
          >A+</button>
        </div>
      </div>

      <div class="field-row">
        <span class="field-text">
          <span class="field-label">Auto-handoff at</span>
          <span class="field-hint">Context fill that cycles the agent to a fresh one</span>
        </span>
        <div class="handoff-control">
          <input
            class="handoff-slider"
            aria-label="Auto-handoff threshold"
            max={MAXIMUM_HANDOFF_PERCENTAGE}
            min={MINIMUM_HANDOFF_PERCENTAGE}
            oninput={e => applyHandoffPercentage(e.currentTarget.value)}
            type="range"
            value={effective.handoffPercentage}
          />
          <span class="handoff-value">
            <input
              class="handoff-number"
              aria-label="Auto-handoff threshold percent"
              max={MAXIMUM_HANDOFF_PERCENTAGE}
              min={MINIMUM_HANDOFF_PERCENTAGE}
              onchange={e => {
                applyHandoffPercentage(e.currentTarget.value);
                // A rejected entry (out of band, not a number) snaps the box
                // back to the persisted value instead of lying at rest.
                e.currentTarget.value = String(effective.handoffPercentage);
              }}
              type="number"
              value={effective.handoffPercentage}
            />
          </span>
        </div>
      </div>

      <UsagePacingSettings />
    </section>

    <section class="session-tabs">
      <h3 class="card-label">Session tabs</h3>
      <div class="naming-row">
        <span class="field-text">
          <span class="field-label">Name tabs with AI</span>
          <span class="field-hint">
            Each tab is renamed from what its agent is doing, refreshed while it works
          </span>
        </span>
        <div class="naming-scope">
          <Segmented
            chosen={namingChoice}
            label="Name tabs with AI, in this project"
            onchoose={async choice => await chooseNamingScope(choice)}
            options={namingScopes}
          />
        </div>
      </div>
      <p class="field-hint">
        {#if namingChoice === undefined}
          Following the global setting, which is {globalNamingWord}. Change it for every project in the
          project picker.
        {:else}
          Set for this project only - the global setting is {globalNamingWord}. Pick
          <strong>Global</strong> to follow it again.
        {/if}
      </p>
    </section>

    <section class="models">
      <h3 class="card-label">Starting model</h3>
      <p class="field-hint">
        New sessions in this workspace start on these - each tab can still switch from its model tag
      </p>
      <AgentModelDefaults {agents} {project} />
    </section>

    <section class="performance">
      <h3 class="card-label">Performance</h3>
      <div class="software-render-row">
        <label class="software-render-check">
          <Checkbox checked={effective.softwareRender} onchange={checked => updatePrefs({ softwareRender: checked })} />
          <span class="field-text">
            <span class="field-label">Software rendering</span>
            <span class="field-hint">Frees the GPU for games. Applies after restart</span>
          </span>
        </label>
        <button
          class="restart"
          onclick={async () => await sendMessage(Command.restart)}
          type="button"
        >Restart PADE</button>
      </div>
    </section>

    <section class="discord">
      <h3 class="card-label">Discord</h3>
      <label class="check">
        <Checkbox checked={effective.discordPresence} onchange={checked => updatePrefs({ discordPresence: checked })} />
        <span class="field-text">
          <span class="field-label">Show my activity</span>
          <span class="field-hint">Reports “Playing PADE” on your Discord profile</span>
        </span>
      </label>
      <label class="check" class:disabled={!effective.discordPresence}>
        <Checkbox
          checked={effective.discordShowProject}
          disabled={!effective.discordPresence}
          onchange={checked => updatePrefs({ discordShowProject: checked })}
        />
        <span class="field-text">
          <span class="field-label">Include the project name</span>
          <span class="field-hint">Adds the open project and its language icon</span>
        </span>
      </label>
      <p class="field-hint">Requires the Discord desktop app running on this computer</p>
    </section>

    {#if files.length}
      <span class="card-label">Project files</span>
    {/if}

    {#each files as file (file.rel)}
      <button
        class="row"
        class:selected={selected?.rel === file.rel}
        disabled={!file.exists}
        onclick={() => open(file)}
      >
        <span class="kind {file.kind}">{file.kind}</span>
        <span class="relative-path">{file.rel}</span>
        {#if !file.exists}
          <span class="missing">absent</span>
        {/if}
      </button>
    {/each}

    <section class="viewer">
      <div class="card">
        <h3 class:placeholder={!selected}>{selected?.rel ?? "Select a file to view"}</h3>
        <pre class="body"><ColorText
            markdown={isMarkdownPath(selected?.rel ?? "")}
            text={content}
            vars={fileVars}
          /></pre>
      </div>
      <p class="note">Read-only in the MVP - edits will write back to this same file</p>
    </section>
  </div>
</div>

<style>
  .configuration {
    display: flex;
    flex-direction: column;
    block-size: 100%;
  }

  .scroll {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 10px;
    overflow-y: auto;
    min-block-size: 0;
    padding: 10px;
    animation: panel-swap 280ms var(--ease);
  }

  /* ── Appearance card ──────────────────────────────────────────────────────── */
  .appearance,
  .session-tabs,
  .models,
  .performance,
  .discord {
    /* The cards answer their own layout questions: the side panel is
       drag-resizable, so anything that reflows on width asks the card
       (@container), never the viewport. */
    container-type: inline-size;
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 14px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);
  }

  .software-render-row {
    display: flex;
    gap: 12px;
    justify-content: space-between;
    align-items: center;
  }

  /* ── Session tabs card ────────────────────────────────────────────────────── */
  .session-tabs {
    /* The setting, its scope control and the line naming what it inherits read
       as one block, tighter than the 16px card rhythm. */
    gap: 10px;

    .naming-row {
      /* Label beside the control while the panel is wide; stacked once the drag
         handle takes that room away. Asked of the card, never the viewport. */
      @container (inline-size < 400px) {
        flex-direction: column;
        align-items: stretch;
      }

      display: flex;
      gap: 12px;
      justify-content: space-between;
      align-items: center;
    }

    /* Three equal scopes, always in one row - they are short words, and stacking
       them would read as three settings rather than one choice. */
    .naming-scope {
      flex: none;
      min-inline-size: 170px;
    }
  }

  .software-render-check {
    display: flex;
    gap: 10px;
    align-items: center;
    min-inline-size: 0;
    cursor: pointer;
  }

  .field-label {
    color: var(--on-surface);
    font-weight: 600;
    font-size: 12px;
  }

  /* ── Discord card ─────────────────────────────────────────────────────────── */
  .discord {
    /* Two checkbox rows and a note read tighter than the 16px card rhythm. */
    gap: 10px;

    .check {
      display: flex;
      gap: 10px;
      align-items: center;
      min-inline-size: 0;
      cursor: pointer;

      /* The dependent row dims until presence is on - it has no effect until
         then, and its checkbox is disabled to match. */
      &.disabled {
        cursor: default;

        .field-label {
          color: var(--on-surface-variant);
        }
      }
    }
  }

  .restart {
    flex: none;
    padding-block: 7px;
    padding-inline: 12px;
    border: none;
    border-radius: var(--radius-full);
    background: var(--primary-container);
    color: var(--on-primary-container);
    font: inherit;
    font-weight: 700;
    font-size: 11px;
    cursor: pointer;
    transition: filter 140ms var(--ease);

    &:hover {
      filter: brightness(0.96);
    }
  }

  .card-label {
    margin: 0;
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  /* Four themes: a 2×2 block, and one row once there is room for four labels.
     Both sides are container queries, mutually exclusive, because a plain
     fallback declaration would sit AFTER the nested at-rule and win over it.
     Asked of the card, not the viewport - the side panel is drag-resizable, so a
     media query would answer about the wrong box. */
  .theme-choices {
    @container (inline-size < 460px) {
      --segmented-columns: repeat(2, 1fr);
      --segmented-flow: row;
    }

    @container (inline-size >= 460px) {
      --segmented-columns: repeat(4, 1fr);
      --segmented-flow: row;
    }
  }

  /* Terminal font - two-column preview cards. */
  .font-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }

  .font-card {
    display: flex;
    gap: 8px;
    justify-content: space-between;
    align-items: center;
    padding: 8px 10px;
    border: 1px solid var(--outline);
    border-radius: 10px;
    background: transparent;
    color: var(--on-surface);
    font: inherit;
    text-align: start;
    cursor: pointer;
    transition:
      background 140ms var(--ease),
      border-color 140ms var(--ease);

    &:hover:not(.on) {
      background: var(--surface-2);
    }

    &.on {
      border-color: var(--primary);
      background: var(--primary-container);
      color: var(--on-primary-container);
    }

    .font-text {
      display: flex;
      flex-direction: column;
      gap: 1px;
      min-inline-size: 0;
      line-height: 1.3;
    }

    .font-name {
      overflow: hidden;
      font-weight: 600;
      font-size: 12px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .font-preview {
      font-size: 11px;
      opacity: 70%;
    }
  }

  /* Font size - label + stepper. */
  .field-row {
    display: flex;
    gap: 12px;
    justify-content: space-between;
    align-items: center;
  }

  .field-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    line-height: 1.3;
  }

  .field-hint {
    color: var(--on-surface-variant);
    font-size: 10px;
  }

  .stepper {
    display: inline-flex;
    flex: none;
    gap: 4px;
    align-items: center;
    padding: 4px;
    border-radius: 12px;
    background: var(--surface-2);

    .step {
      display: inline-flex;
      justify-content: center;
      align-items: center;
      block-size: 28px;
      inline-size: 28px;
      border: none;
      border-radius: 9px;
      background: transparent;
      color: var(--on-surface);
      font: inherit;
      font-weight: 800;
      font-size: 13px;
      cursor: pointer;
      transition: background 140ms var(--ease);

      &:disabled {
        opacity: 40%;
        cursor: default;
      }

      &:hover:not(:disabled) {
        background: var(--surface-3);
      }
    }

    .step-up {
      font-size: 17px;
    }

    .scale-value {
      min-inline-size: 44px;
      font-weight: 700;
      font-size: 12px;
      text-align: center;
    }
  }

  .handoff-control {
    display: inline-flex;
    flex: none;
    gap: 10px;
    align-items: center;

    .handoff-slider {
      inline-size: 110px;
      accent-color: var(--primary);
    }

    /* The percent sign lives visually inside the box, after the digits. */
    .handoff-value {
      position: relative;
      display: inline-flex;
      align-items: center;

      &::after {
        content: "%";
        position: absolute;
        inset-inline-end: 8px;
        color: var(--on-surface-variant);
        font-weight: 700;
        font-size: 12px;
        pointer-events: none;
      }
    }

    .handoff-number {
      appearance: textfield;

      /* Two digits plus the inset % suffix; ch tracks the real digit width so
         a zoomed UI can never clip "30" down to a lone digit. */
      inline-size: calc(3ch + 2.5rem);
      padding-block: 6px;
      padding-inline: 8px 1.375rem;
      border: none;
      border-radius: 12px;
      background: var(--surface-2);
      color: var(--on-surface);
      font: inherit;
      font-weight: 700;
      font-size: 12px;
      text-align: end;
    }
  }

  .row {
    display: flex;
    gap: 10px;
    align-items: center;
    inline-size: 100%;
    padding-block: 8px;
    padding-inline: 10px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface);
    text-align: start;
    cursor: pointer;
    transition: background 140ms var(--ease);

    &:disabled {
      opacity: 45%;
      cursor: default;
    }

    &:hover:not(:disabled) {
      background: var(--surface-2);
    }

    &.selected {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }

  .kind {
    flex: none;
    padding-block: 2px;
    padding-inline: 9px;
    border-radius: var(--radius-full);
    background: var(--surface-3);
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 10px;
    letter-spacing: 0.05em;
    text-transform: uppercase;

    &.instructions {
      background: var(--tertiary-wash);
      color: var(--tertiary);
    }

    &.mcp {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }

  .relative-path {
    overflow: hidden;
    font-family: var(--font-monospace);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .missing {
    margin-inline-start: auto;
    color: var(--on-surface-variant);
    font-style: italic;
    font-size: 10px;
  }

  .viewer {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-block-start: 6px;

    .card {
      overflow: hidden;
      border: 1px solid var(--outline);
      border-radius: var(--radius-medium);
    }

    h3 {
      margin: 0;
      padding-block: 8px;
      padding-inline: 12px;
      background: var(--surface-2);
      color: var(--on-surface);
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 12px;
    }

    .placeholder {
      color: var(--on-surface-variant);
      font-style: italic;
    }

    .body {
      overflow: auto;
      max-block-size: 280px;
      margin: 0;
      padding: 12px;
      background: var(--code-background);
      color: var(--code-foreground);
      font-family: var(--font-monospace);
      font-size: 12px;
      line-height: 1.55;
      white-space: pre-wrap;
    }

    .note {
      margin-block: 2px 0;
      margin-inline: 0;
      color: var(--on-surface-variant);
      font-style: italic;
      font-size: 11px;
    }
  }
</style>
