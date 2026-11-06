<script lang="ts">
  // The subagents pacing's terminal stop couldn't end (pressed twice, still
  // writing): named here so the user stops them by hand, instead of PADE
  // pressing keys in a loop. Nothing shows while there are none.

  import { unstoppedSubagents } from "@/lib/stores/usage/pacingInterrupt.svelte";
  import { truncationTooltip } from "@/lib/ui/tooltip/truncation-tooltip";

  /** A subagent named by its id alone shows this much of it. */
  const SHORT_ID_LENGTH = 7;

  const rows = $derived(
    unstoppedSubagents().flatMap(session =>
      session.subagents.map(subagent => ({
        key: `${session.sessionId}:${subagent.id}`,
        text: `couldn't stop ${subagent.description ?? subagent.id.slice(0, SHORT_ID_LENGTH)}: stop it in its pane`,
        sessionName: session.sessionName
      })))
  );
</script>

{#if rows.length > 0}
  <div class="field">
    <span class="field-label">Still running</span>
    <ul>
      {#each rows as row (row.key)}
        <li class="field-row">
          <span class="name" {@attach truncationTooltip({ tooltip: row.sessionName })}>{row.sessionName}</span>
          <span class="problem" {@attach truncationTooltip({ tooltip: row.text })}>{row.text}</span>
        </li>
      {/each}
    </ul>
  </div>
{/if}

<style>
  ul {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .name {
    flex-shrink: 0;
    overflow: hidden;
    max-inline-size: 40%;
    color: var(--on-surface);
    font-size: 0.6875rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .problem {
    overflow: hidden;
    color: var(--critical);
    font-size: 0.6875rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
