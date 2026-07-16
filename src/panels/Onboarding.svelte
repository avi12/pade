<script lang="ts">
  import BrandMark from "@/lib/BrandMark.svelte";
  import Icon from "@/lib/Icon.svelte";
  import type { Agent } from "@/lib/types";

  // Shown only when more than one agent is installed — the user picks which to
  // launch. `path` is the already-chosen workspace the agent will start in, shown
  // up front so the user knows where they're about to run. They can switch or add
  // more agents later from the session bar.
  const { agents, path, onpick, onswitchproject }: {
    agents: Agent[];
    path: string;
    onpick: (a: Agent) => void;
    onswitchproject: () => void;
  } = $props();
</script>

<div class="onboarding">
  <div class="card">
    <BrandMark />
    <h1>Choose an agent to start.</h1>
    <p class="lede">
      Several agents are installed on this machine. Pick one to begin — you can
      switch or run more side by side later.
    </p>

    <button class="cwd" onclick={onswitchproject}>
      <span class="lead"><Icon name="folder" /></span>
      <span class="stack">
        <span class="eyebrow">Working directory</span>
        <span class="path">{path}</span>
      </span>
      <span class="switch">Switch project</span>
    </button>

    <ul class="agents">
      {#each agents as a (a.id)}
        <li>
          <button class="agent" onclick={() => onpick(a)}>
            <span class="name">{a.label}</span>
            <code class="cmd">{a.command}</code>
          </button>
        </li>
      {/each}
    </ul>
  </div>
</div>

<style>
  .onboarding {
    display: grid;
    place-items: center;
    block-size: 100%;
    padding: 24px;
    background: radial-gradient(120% 120% at 50% 0%, var(--surface-1), var(--surface));
  }

  .card {
    inline-size: min(560px, 100%);
    padding: 40px;
    border-radius: var(--radius-extra-large);
    background: var(--surface-1);

    /* Floating hero surface — soft depth shadow (allowed on elevated cards),
       tinted from the theme's shadow token so it tracks light/dark. */
    box-shadow: 0 24px 60px var(--shadow-color);
    animation: rise 350ms var(--ease);

    h1 {
      margin-block: 14px 0;
      margin-inline: 0;
      font-weight: 800;
      font-size: clamp(24px, 4vw, 36px);
      line-height: 1.1;
      letter-spacing: -0.02em;
      text-wrap: balance;
    }

    .lede {
      max-inline-size: 46ch;
      margin-block: 12px 0;
      margin-inline: 0;
      color: var(--on-surface-variant);
      font-size: 14px;
      line-height: 1.5;
    }
  }

  /* The workspace the agent will start in — click to switch project. */
  .cwd {
    display: flex;
    gap: 10px;
    align-items: center;
    inline-size: 100%;
    margin-block: 20px 0;
    padding: 10px 13px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-2);
    color: var(--on-surface);
    font: inherit;
    text-align: start;
    cursor: pointer;
    transition: background 200ms var(--ease), border-color 200ms var(--ease);

    &:hover {
      border-color: var(--primary);
      background: var(--surface-3);
    }

    .lead {
      display: inline-flex;
      flex-shrink: 0;
      color: var(--primary);
    }

    .stack {
      display: flex;
      flex: 1;
      flex-direction: column;
      min-inline-size: 0;
      line-height: 1.3;
    }

    /* Fades in on hover to signal the card navigates to the picker. */
    .switch {
      flex-shrink: 0;
      color: var(--primary);
      font-weight: 600;
      font-size: 12px;
      opacity: 0%;
      transition: opacity 200ms var(--ease);
    }

    &:hover .switch {
      opacity: 100%;
    }

    .eyebrow {
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 9px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
    }

    .path {
      overflow: hidden;
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 12px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
  }

  .agents {
    display: grid;
    gap: 10px;
    margin-block: 26px 0;
    margin-inline: 0;
    padding: 0;
    list-style: none;
  }

  .agent {
    display: flex;
    gap: 16px;
    justify-content: space-between;
    align-items: center;
    inline-size: 100%;
    padding: 18px 20px;
    border: 1px solid transparent;
    border-radius: var(--radius-large);
    background: var(--surface-2);

    /* Buttons don't inherit the page text color — set it explicitly. */
    color: var(--on-surface);
    font: inherit;
    text-align: start;
    cursor: pointer;
    transition:
      background 200ms var(--ease),
      border-color 200ms var(--ease),
      color 200ms var(--ease);

    &:hover {
      border-color: var(--primary);
      background: var(--primary-container);
      color: var(--on-primary-container);
    }

    .name {
      font-weight: 600;
      font-size: 16px;
    }

    .cmd {
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 12px;
    }

    &:hover .cmd {
      color: var(--on-primary-container);
    }
  }
</style>
