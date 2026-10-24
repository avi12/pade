<script lang="ts">
  // A tab's inline manual rename: the status glyph and a text field seeded with
  // the session's current label. Enter or blur commits the draft, Escape drops it.

  import TabStatusGlyph from "@/lib/session/tabs/TabStatusGlyph.svelte";
  import { sessionLabel, setSessionLabel } from "@/lib/stores/session/sessionLabels.svelte";
  import type { AgentSession } from "@/lib/types";
  import { Key } from "@/lib/ui/keyboard/keys";
  import { parseInput, SessionName } from "@/lib/validate";
  import { untrack } from "svelte";

  const { session, isAwaitingChoice, ondone }: {
    session: AgentSession;
    isAwaitingChoice: boolean;
    /** The rename was committed or dropped - leave rename mode. */
    ondone: () => void;
  } = $props();

  // Seeded once as the field mounts; the user's typing owns it from then on.
  let draft = $state(untrack(() => sessionLabel(session.id) ?? session.agent.label));
  // Set once the rename is committed or dropped, so the blur that follows the
  // field leaving the page can't commit it a second time.
  let isSettled = false;

  // Enter and blur both land here: a valid name becomes the session's label.
  function commit() {
    if (isSettled) {
      return;
    }

    isSettled = true;
    const label = parseInput({
      schema: SessionName,
      raw: draft
    });
    if (label !== null) {
      setSessionLabel({
        id: session.id,
        label
      });
    }

    ondone();
  }

  // Focus + select the rename field the moment it mounts.
  function focusOnMount(elInput: HTMLInputElement) {
    elInput.focus();
    elInput.select();
  }
</script>

<span class="rename">
  <TabStatusGlyph {isAwaitingChoice} {session} />
  <input
    class="rename-input"
    {@attach focusOnMount}
    aria-label="Rename session"
    data-noreorder
    onblur={commit}
    oninput={e => (draft = e.currentTarget.value)}
    onkeydown={e => {
      if (e.key === Key.Enter) {
        e.preventDefault();
        commit();
      } else if (e.key === Key.Escape) {
        e.preventDefault();
        isSettled = true;
        ondone();
      }
    }}
    value={draft}
  />
</span>

<style>
  /* Under `.tabs` so the rule keeps the specificity it had in the strip. */
  :global(.tabs) {
    /* Inline rename field, sized like the label it replaces. */
    .rename {
      display: inline-flex;
      gap: 7px;
      align-items: center;
      padding-block: 6px;
      padding-inline: 12px 4px;

      .rename-input {
        inline-size: 7.5rem;
        min-inline-size: 0;
        border: none;
        background: transparent;
        color: var(--on-surface);
        outline: none;
        font-family: var(--font-monospace);
        font-weight: 700;
        font-size: 0.75rem;
      }
    }
  }
</style>
