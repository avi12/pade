<script lang="ts">
  // One session pill's contents: the status glyph and label (or the inline
  // rename field), the model tag, the pacing badge, the AI-name toggle and the
  // close button. The strip owns the pill element itself, its drag and collapse.

  import Icon from "@/lib/Icon.svelte";
  import ModelTag from "@/lib/model/ModelTag.svelte";
  import { MODEL_MENU_ID } from "@/lib/session/tabs/TabModelMenu.svelte";
  import TabRenameField from "@/lib/session/tabs/TabRenameField.svelte";
  import TabStatusGlyph from "@/lib/session/tabs/TabStatusGlyph.svelte";
  import { refreshModelOptions } from "@/lib/stores/model/modelOptions.svelte";
  import { sessionLabel } from "@/lib/stores/session/sessionLabels.svelte";
  import { isNaming, toggleNaming } from "@/lib/stores/session/sessionNaming.svelte";
  import type { AgentSession } from "@/lib/types";
  import PacingBadge from "@/lib/usage/PacingBadge.svelte";

  const {
    session,
    isMirror,
    isActive,
    isEditing,
    isAwaitingChoice,
    isModelMenuTarget,
    onselect,
    onrename,
    onrenamedone,
    onclose,
    onmodelopen
  }: {
    session: AgentSession;
    /** The off-layout measuring copy, which must never claim the model menu's
     *  anchor (two anchors of one name pin the menu to the wrong pill). */
    isMirror: boolean;
    isActive: boolean;
    isEditing: boolean;
    isAwaitingChoice: boolean;
    /** The strip's one model menu was last opened from this session's tag. */
    isModelMenuTarget: boolean;
    onselect: () => void;
    /** Click on the active pill - show the inline rename field. */
    onrename: () => void;
    /** The rename was committed or dropped - hide the field again. */
    onrenamedone: () => void;
    onclose: () => void;
    /** The model tag was clicked - hang the strip's model menu off it. */
    onmodelopen: () => void;
  } = $props();

  // The model menu hangs off this pill's tag - never the mirror copy's.
  const isModelMenuAnchored = $derived(!isMirror && isModelMenuTarget);

  // `MouseEvent.button` for the wheel button, which closes a pill.
  const MIDDLE_BUTTON = 1;
  const AUXCLICK_EVENT = "auxclick";

  // Middle-click anywhere on a pill closes it (preventDefault stops the browser's
  // middle-click autoscroll). onmousedown suppresses the same on press.
  function onTabPointer(e: MouseEvent) {
    if (e.button !== MIDDLE_BUTTON) {
      return;
    }

    e.preventDefault();

    if (e.type === AUXCLICK_EVENT) {
      onclose();
    }
  }
</script>

{#if isEditing}
  <TabRenameField {isAwaitingChoice} ondone={onrenamedone} {session} />
{:else}
  <button
    class="pick"
    onauxclick={onTabPointer}
    onclick={() => {
      // Finder-style: a click selects an inactive tab; clicking the already-active
      // tab renames it (its label reads with a text caret). The reorder engine
      // swallows the post-drag click, so dragging the active pill never renames.
      if (isActive) {
        onrename();
      } else {
        onselect();
      }
    }}
    onmousedown={onTabPointer}
  >
    <TabStatusGlyph {isAwaitingChoice} {session} />
    <span class="label">{sessionLabel(session.id) ?? session.agent.label}</span>
  </button>
  <ModelTag
    anchored={isModelMenuAnchored}
    menuId={MODEL_MENU_ID}
    onopen={async () => {
      onmodelopen();
      await refreshModelOptions(session.agent.command);
    }}
    {session}
  />
  <PacingBadge isMeasuring={isMirror} sessionId={session.id} />
  <span
    class="ai-wrap"
    class:on={isNaming(session.id)}
    data-tooltip={isNaming(session.id) ? "Auto-naming on - click to turn off" : "Auto-name this session with AI"}
  >
    <button
      class="ai"
      aria-label="Auto-name this session with AI"
      data-noreorder
      onclick={() => toggleNaming({
        id: session.id,
        agent: session.agent.command
      })}
    ><Icon name="sparkles" size={13} /></button>
  </span>
{/if}
<button
  class="close-button"
  aria-label="Close session"
  data-noreorder
  data-tooltip="Close session"
  onclick={onclose}
><Icon name="close" size={13} /></button>

<style>
  /* Under `.tabs` so these rules keep the specificity they had in the strip,
     and keyed off the strip's `.tab` pill for its hover / active states. */
  :global(.tabs) {
    .pick {
      display: inline-flex;
      gap: 7px;
      align-items: center;
      padding-block: 6px;
      padding-inline: 12px 4px;
      border: none;

      /* The pill's leading cap, mirroring the trailing one on .close-button:
         this button is the pill's first child and would otherwise square off
         the corner the pill rounds (or, under a palette that bevels it, cuts)
         away - and a corner is hit-tested against the shape it is GIVEN, so a
         square child inside a shaped parent takes clicks from air. */
      border-end-start-radius: 999px;
      border-start-start-radius: 999px;
      background: transparent;
      color: var(--on-surface-variant);
      font-family: var(--font-monospace);
      font-size: 12px;
      white-space: nowrap;
      cursor: pointer;
    }

    /* The ✦ AI-name toggle - subdued on every pill, brighter on the one being
       used, and primary while auto-naming is on for the session. The slot is
       reserved on every pill, hovered or not. A slot that
       opened on hover grew its pill by 24px and pushed every pill to its right -
       so crossing a neighbour on the way to this one slid the target out from
       under the pointer as that neighbour collapsed again, and the click landed
       on the × beside it. Fixed geometry also makes the off-layout mirror measure
       the width the pill really renders at. */
    .ai-wrap {
      display: inline-flex;
      flex: none;
      align-items: center;
      block-size: 26px;
      inline-size: 24px;
    }

    .ai {
      display: inline-flex;
      flex: none;
      justify-content: center;
      align-items: center;
      overflow: hidden;
      block-size: 26px;
      inline-size: 24px;
      border: none;
      background: transparent;
      color: var(--on-surface-variant);

      /* Subdued rather than absent, matching the × beside it: renaming a session
         you are NOT in is exactly when this is wanted, and an icon that only
         exists once you hover cannot be found without hunting for it - or, on a
         touch screen, at all. */
      opacity: 60%;
      cursor: pointer;
      transition:
        opacity 140ms var(--ease),
        color 140ms var(--ease);

      /* Brighter on the pill being used. */
      :global(.tab:hover) .ai-wrap &,
      :global(.tab.active) .ai-wrap & {
        opacity: 85%;
      }

      /* The tooltip belongs to `.ai-wrap`, outside this faded icon button, so
         the indicator can stay subdued without dimming or clipping its bubble. */
      :global(.tab:hover) .ai-wrap &:hover,
      :global(.tab.active) .ai-wrap &:hover,
      &:hover {
        color: var(--primary);
        opacity: 100%;
      }

      /* Pinned bright while auto-naming is on for this session, and backed by a
         tinted plate. The plate is what actually carries the on/off state: the
         brightening alone leans on the icon's COLOUR differing between the two,
         and on the active pill it cannot - the accent this icon reaches for is
         the pill's own fill, so both states resolve to the container's
         on-colour and only a 15-point opacity step separates them. Mixed from
         `currentcolor`, so it reads on the pale row and the filled pill alike
         without either needing to know which one it is sitting on. */
      .ai-wrap.on & {
        border-radius: var(--radius-small);
        background: color-mix(in sRGB, currentColor 22%, transparent);
        color: var(--primary);
        opacity: 100%;
      }

      /* On the active pill the fill IS the primary container, so the accent
         every state above reaches for is the same colour as the ground beneath
         it - on a palette whose container is dark that measured 1.01:1, which
         is invisible rather than merely dim. Each of those states takes the
         container's on-colour here instead, the way the × beside it already
         does. */
      :global(.tab.active) .ai-wrap &,
      :global(.tab.active) .ai-wrap &:hover,
      :global(.tab.active) .ai-wrap.on & {
        color: var(--on-primary-container);
      }
    }

    .close-button {
      display: inline-flex;
      justify-content: center;
      align-items: center;
      block-size: 26px;
      inline-size: 24px;
      border: none;
      border-end-start-radius: 0;
      border-end-end-radius: 999px;
      border-start-end-radius: 999px;
      border-start-start-radius: 0;
      background: transparent;
      color: var(--on-surface-variant);
      font-size: 15px;
      line-height: 1;
      opacity: 60%;
      cursor: pointer;
      transition: color 150ms var(--ease), background 150ms var(--ease), opacity 150ms var(--ease);

      &:hover {
        background: var(--critical-wash);
        color: var(--critical);
        opacity: 100%;
      }
    }

    :global(.tab.active) {
      .pick {
        color: var(--on-primary-container);
        font-weight: 600;
      }

      /* The active pill's label reads with a text caret - a single click there
         renames it (Finder-style); inactive labels inherit the pill's pointer. */
      .label {
        cursor: text;
      }

      /* On the active pill the close × rides the container's on-color too. */
      .close-button {
        color: var(--on-primary-container);
      }
    }
  }
</style>
