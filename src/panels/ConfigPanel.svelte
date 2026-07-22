<script lang="ts">
  import { config } from "@/lib/bridge";
  import { collectVars } from "@/lib/colors";
  import ColorText from "@/lib/ColorText.svelte";
  import { MAXIMUM_HANDOFF_PCT, MINIMUM_HANDOFF_PCT } from "@/lib/context-level";
  import { formatPercent } from "@/lib/format";
  import Icon, { type IconName } from "@/lib/Icon.svelte";
  import { effective, prefs, updatePrefs } from "@/lib/prefs.svelte";
  import { setPanelHeader } from "@/lib/stores/sidePanel.svelte";
  import { type ConfigFile, ThemeMode } from "@/lib/types";

  // Only the config files relevant to the active agent are listed.
  const { agent }: { agent: string } = $props();

  // ── Appearance card ─────────────────────────────────────────────────────────
  // Theme mode, terminal font, and UI zoom — each bound to a persisted pref via
  // updatePrefs (merge → apply → save through the shared store).
  const themeOptions = [
    {
      mode: ThemeMode.enum.light,
      label: "Light",
      icon: "sun"
    },
    {
      mode: ThemeMode.enum.dark,
      label: "Dark",
      icon: "moon"
    },
    {
      mode: ThemeMode.enum.system,
      label: "Auto",
      icon: "monitor"
    }
  ] as const satisfies readonly {
    mode: ThemeMode;
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
    }
  ] as const satisfies readonly {
    name: string;
    value: string;
    family: string;
  }[];

  const selectedMonoFont = $derived(prefs.monoFont ?? "");

  const MINIMUM_UI_SCALE = 0.85;
  const MAXIMUM_UI_SCALE = 1.3;
  const UI_SCALE_STEP = 0.05;
  const scalePercent = $derived(formatPercent(effective.uiScale * 100));

  async function stepScale(delta: number): Promise<void> {
    const clamped = Math.min(MAXIMUM_UI_SCALE, Math.max(MINIMUM_UI_SCALE, effective.uiScale + delta));
    // Round to the step grid so accumulated float drift never leaks into the pref.
    await updatePrefs({ uiScale: Math.round(clamped * 100) / 100 });
  }

  const HANDOFF_PCT_STEP = 5;

  async function stepHandoff(delta: number): Promise<void> {
    const clamped = Math.min(
      MAXIMUM_HANDOFF_PCT,
      Math.max(MINIMUM_HANDOFF_PCT, effective.handoffPct + delta)
    );
    await updatePrefs({ handoffPct: clamped });
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
      const text = await config.read(requested.rel);
      if (selected?.rel === requested.rel) {
        content = text;
      }
    } catch {
      if (selected?.rel === requested.rel) {
        content = "";
      }
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
      const listed = await config.list(requestedAgent);
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

  // Config has no count or refresh — clear those slots in the shared header.
  $effect(() => {
    setPanelHeader({
      count: null,
      refresh: null
    });
  });
</script>

<div class="cfg">
  <div class="scroll">
    <section class="appearance">
      <h3 class="card-label">Appearance</h3>

      <div class="field">
        <span class="field-label">Theme</span>
        <div class="segmented" aria-label="Theme" role="group">
          {#each themeOptions as option (option.mode)}
            <button
              class="opt"
              class:on={effective.themeMode === option.mode}
              aria-pressed={effective.themeMode === option.mode}
              onclick={() => updatePrefs({ themeMode: option.mode })}
            >
              <Icon name={option.icon} />
              <span>{option.label}</span>
            </button>
          {/each}
        </div>
      </div>

      <div class="field">
        <span class="field-label">Terminal font</span>
        <div class="font-grid">
          {#each fontOptions as option (option.name)}
            <button
              class="font-card"
              class:on={selectedMonoFont === option.value}
              aria-pressed={selectedMonoFont === option.value}
              onclick={() => updatePrefs({ monoFont: option.value })}
            >
              <span class="font-text">
                <span class="font-name">{option.name}</span>
                <span style:font-family={option.family} class="font-preview">Ag0 &lt;/&gt; 1l</span>
              </span>
              {#if selectedMonoFont === option.value}
                <Icon name="check" />
              {/if}
            </button>
          {/each}
        </div>
      </div>

      <div class="field-row">
        <span class="field-text">
          <span class="field-label">Font size</span>
          <span class="field-hint">Applies to UI and terminal</span>
        </span>
        <div class="stepper">
          <button
            class="step"
            aria-label="Decrease font size"
            disabled={effective.uiScale <= MINIMUM_UI_SCALE}
            onclick={() => stepScale(-UI_SCALE_STEP)}
          >A−</button>
          <output class="scale-value">{scalePercent}</output>
          <button
            class="step step-up"
            aria-label="Increase font size"
            disabled={effective.uiScale >= MAXIMUM_UI_SCALE}
            onclick={() => stepScale(UI_SCALE_STEP)}
          >A+</button>
        </div>
      </div>

      <div class="field-row">
        <span class="field-text">
          <span class="field-label">Auto-handoff at</span>
          <span class="field-hint">Context fill that cycles the agent to a fresh one</span>
        </span>
        <div class="stepper">
          <button
            class="step"
            aria-label="Lower the handoff threshold"
            disabled={effective.handoffPct <= MINIMUM_HANDOFF_PCT}
            onclick={() => stepHandoff(-HANDOFF_PCT_STEP)}
          >−</button>
          <output class="scale-value">{formatPercent(effective.handoffPct)}</output>
          <button
            class="step step-up"
            aria-label="Raise the handoff threshold"
            disabled={effective.handoffPct >= MAXIMUM_HANDOFF_PCT}
            onclick={() => stepHandoff(HANDOFF_PCT_STEP)}
          >+</button>
        </div>
      </div>
    </section>

    {#if files.length}
      <span class="card-label">Project files</span>
    {/if}

    {#each files as f (f.rel)}
      <button
        class="row"
        class:sel={selected?.rel === f.rel}
        disabled={!f.exists}
        onclick={() => open(f)}
      >
        <span class="kind {f.kind}">{f.kind}</span>
        <span class="rel">{f.rel}</span>
        {#if !f.exists}
          <span class="missing">absent</span>
        {/if}
      </button>
    {/each}

    <section class="viewer">
      <div class="card">
        <h3 class:placeholder={!selected}>{selected?.rel ?? "Select a file to view"}</h3>
        <pre class="body"><ColorText text={content} vars={fileVars} /></pre>
      </div>
      <p class="note">Read-only in the MVP — edits will write back to this same file.</p>
    </section>
  </div>
</div>

<style>
  .cfg {
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
  .appearance {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 14px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);
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

  .field-label {
    color: var(--on-surface);
    font-weight: 600;
    font-size: 12px;
  }

  /* Theme — segmented single-select (buttons + aria-pressed, per the app's
     shared segmented pattern). */
  .segmented {
    display: flex;
    gap: 4px;
    padding: 4px;
    border-radius: 12px;
    background: var(--surface-2);

    .opt {
      display: inline-flex;
      flex: 1;
      gap: 6px;
      justify-content: center;
      align-items: center;
      padding: 7px 8px;
      border: none;
      border-radius: 9px;
      background: transparent;
      color: var(--on-surface-variant);
      font: inherit;
      font-weight: 600;
      font-size: 12px;
      cursor: pointer;
      transition:
        background 200ms var(--ease),
        color 200ms var(--ease);

      &:hover:not(.on) {
        background: var(--surface-3);
        color: var(--on-surface);
      }
    }

    .on {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }

  /* Terminal font — two-column preview cards. */
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

  /* Font size — label + stepper. */
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

      &:hover:not(:disabled) {
        background: var(--surface-3);
      }

      &:disabled {
        opacity: 40%;
        cursor: default;
      }
    }

    .step-up {
      font-size: 17px;
    }

    .scale-value {
      min-inline-size: 44px;
      font-weight: 700;
      font-size: 12px;
      font-variant-numeric: tabular-nums;
      text-align: center;
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

    &:hover:not(:disabled) {
      background: var(--surface-2);
    }

    &:disabled {
      opacity: 45%;
      cursor: default;
    }

    &.sel {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }

  .kind {
    flex: none;
    padding-block: 2px;
    padding-inline: 9px;
    border-radius: 999px;
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

  .rel {
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
