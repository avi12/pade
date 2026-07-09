<script lang="ts">
  import type { Agent } from "../lib/types";

  // Shown only when more than one agent is installed — the user picks which to
  // launch. They can switch or add more later from the session bar.
  let { agents, onpick }: { agents: Agent[]; onpick: (a: Agent) => void } = $props();
</script>

<div class="onboarding">
  <div class="card">
    <span class="brand">◆ ADE</span>
    <h1>Choose an agent to start</h1>
    <p class="lede">
      Several agents are installed. Pick one to launch now — you can switch
      between them or run more side by side at any time.
    </p>

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
    background:
      radial-gradient(120% 80% at 50% 0%, var(--surface-1), var(--surface));
  }
  .card {
    inline-size: min(560px, 100%);
    background: var(--surface-1);
    border-radius: var(--r-xl);
    padding: 40px;

    .brand {
      font-weight: 700;
      color: var(--primary);
      letter-spacing: 0.02em;
    }
    h1 {
      margin: 12px 0 8px;
      font-size: clamp(24px, 4vw, 34px);
      letter-spacing: -0.02em;
      text-wrap: balance;
    }
    .lede {
      margin: 0 0 24px;
      color: var(--on-surface-var);
      max-inline-size: 46ch;
    }
  }

  .agents {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 10px;
  }
  .agent {
    inline-size: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    text-align: start;
    font: inherit;
    padding: 16px 18px;
    background: var(--surface-2);
    border: 1px solid transparent;
    border-radius: var(--r-lg);
    cursor: pointer;
    transition:
      background 0.2s var(--ease),
      border-color 0.2s var(--ease);

    &:hover {
      background: var(--primary-container);
      border-color: var(--primary);
    }
    .name { font-weight: 600; font-size: 16px; }
    .cmd {
      margin-inline-start: auto;
      font-family: var(--font-mono);
      font-size: 12px;
      color: var(--on-surface-var);
    }
  }
</style>
