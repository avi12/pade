<script lang="ts">
  import type { Agent } from "@/lib/types";

  // Shown only when more than one agent is installed — the user picks which to
  // launch. They can switch or add more later from the session bar.
  const { agents, onpick }: {
    agents: Agent[];
    onpick: (a: Agent) => void;
  } = $props();
</script>

<div class="onboarding">
  <div class="card">
    <span class="brand">◆ PADE</span>
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
    background: radial-gradient(120% 120% at 50% 0%, var(--surface-1), var(--surface));
  }

  .card {
    inline-size: min(560px, 100%);
    padding: 40px;
    border-radius: var(--r-xl);
    background: var(--surface-1);

    /* Floating hero surface — soft depth shadow (allowed on elevated cards),
       tinted from the darkest on-surface token so it tracks the theme. */
    box-shadow: 0 24px 60px color-mix(in sRGB, var(--on-surface) 35%, transparent);
    animation: rise 350ms var(--ease);

    .brand {
      color: var(--primary);
      font-weight: 700;
      font-size: 13px;
      letter-spacing: 0.02em;
    }

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
      color: var(--on-surface-var);
      font-size: 14px;
      line-height: 1.5;
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
    border-radius: var(--r-lg);
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
      color: var(--on-surface-var);
      font-family: var(--font-mono);
      font-size: 12px;
    }

    &:hover .cmd {
      color: var(--on-primary-container);
    }
  }
</style>
