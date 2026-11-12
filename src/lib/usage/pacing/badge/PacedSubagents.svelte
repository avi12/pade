<script lang="ts">
  // What the latest account-wide pacing round asks of each live session's
  // subagents, across every window: which subagent is held before each tool
  // call and for how long, which is wrapping up, and which sessions may start
  // no new subagent. Nothing shows while the round holds nothing back.

  import { throttledSessions } from "@/lib/stores/usage/pacingControl.svelte";
  import PacedSession from "@/lib/usage/pacing/badge/PacedSession.svelte";
  import { isThrottling } from "@/lib/usage/pacing/engine/account-round";

  const sessions = $derived(throttledSessions().filter(session => isThrottling(session)));
</script>

{#if sessions.length > 0}
  <div class="field">
    <span class="field-label">Paced subagents</span>
    <ul>
      {#each sessions as session (session.sessionId)}
        <PacedSession {session} />
      {/each}
    </ul>
  </div>
{/if}

<style>
  ul {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
</style>
