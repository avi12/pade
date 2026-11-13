<script lang="ts">
  // A pane's running model and effort, always in view at the end of its header:
  // the agent's mark, the model's name, its effort as signal bars on the
  // model's own ladder, the effort's word and a caret. A button: it opens the
  // window's model menu, the one a tab's model badge opens. Nothing renders
  // until the agent has reported.

  import { agentIconName } from "@/lib/agent/agent-icon";
  import Icon from "@/lib/Icon.svelte";
  import EffortBars from "@/lib/model/readout/EffortBars.svelte";
  import { sessionModelLook } from "@/lib/model/readout/session-model-look";
  import { MODEL_MENU_ID, modelMenuAnchor, ModelMenuOpener, openModelMenu } from "@/lib/stores/model/modelMenu.svelte";
  import type { AgentSession } from "@/lib/types/agent";

  const { session }: { session: AgentSession } = $props();

  const look = $derived(
    sessionModelLook({
      sessionId: session.id,
      command: session.agent.command
    })
  );
</script>

{#if look !== null}
  <button
    style:anchor-name={modelMenuAnchor({
      sessionId: session.id,
      opener: ModelMenuOpener.PANE
    })}
    class="model-chip"
    aria-label={`${look.label} - change model or effort`}
    data-noreorder
    data-tooltip={`${look.label} · click to change`}
    onclick={async () => await openModelMenu({
      session,
      opener: ModelMenuOpener.PANE
    })}
    popovertarget={MODEL_MENU_ID}
  >
    <span class="agent" aria-hidden="true"><Icon name={agentIconName(session.agent.id)} size={13} /></span>
    <span class="name">{look.running.name}</span>
    {#if look.running.isUltracode}
      <Icon name="bolt" size={12} />
    {/if}
    <EffortBars --effort-bar-width="3px" --effort-bars-height="12px" bars={look.bars} />
    {#if look.running.effort !== null}
      <span class="effort">{look.running.effort}</span>
    {/if}
    <span class="caret" aria-hidden="true">▾</span>
  </button>
{/if}

<style>
  /* The design's pane model chip: a pill on the next surface up with a
     hairline frame that turns primary on hover. It gives up width before the
     header's label does, by eliding the model's name. */
  .model-chip {
    display: inline-flex;
    flex-shrink: 1;
    gap: 7px;
    align-items: center;
    block-size: 26px;
    min-inline-size: 0;
    padding-block: 0;
    padding-inline: 9px 8px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-full);
    background: var(--surface-2);
    color: var(--on-surface);
    font-family: var(--font-ui);
    font-weight: 600;
    font-size: 0.71875rem;
    white-space: nowrap;
    cursor: pointer;
    transition: background-color 120ms var(--ease), border-color 120ms var(--ease);

    &:hover,
    &:focus-visible {
      border-color: var(--primary);
      background: var(--surface-3);
    }

    /* Cyberpunk cuts the chip's top-left and bottom-right corners, and on
       hover its parts jitter - the parts, as on the tab's badge, so the chip
       never carries its own tooltip bubble along. */
    :global(:root[data-palette="cyberpunk"]) & {
      clip-path: polygon(6px 0, 100% 0, 100% calc(100% - 6px), calc(100% - 6px) 100%, 0 100%, 0 6px);

      @media (prefers-reduced-motion: no-preference) {
        &:hover > :global(*) {
          animation: var(--cyber-jitter-animation);
        }
      }
    }
  }

  .agent {
    display: inline-flex;
    flex-shrink: 0;
    color: var(--primary);

    /* Cyberpunk lights the agent's mark: it glows in the dark theme (the
       glow token is none in light). */
    :global(:root[data-palette="cyberpunk"]) & {
      filter: drop-shadow(var(--cyber-glow));
    }
  }

  .name {
    overflow: hidden;
    min-inline-size: 0;
    text-overflow: ellipsis;
  }

  .effort {
    flex-shrink: 0;
    color: var(--on-surface-variant);
    font-family: var(--font-monospace);
    font-weight: 700;
    font-size: 0.65625rem;

    /* Cyberpunk spaces its mono readouts. */
    :global(:root[data-palette="cyberpunk"]) & {
      letter-spacing: 0.02em;
    }
  }

  .caret {
    flex-shrink: 0;
    font-size: 0.5625rem;
    opacity: 0.6;
  }
</style>
