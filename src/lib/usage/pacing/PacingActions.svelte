<script lang="ts">
  // The overrides on a paced session: undo pacing's latest step, leave the
  // session alone for the rest of its window, or - when it is stopped or
  // resting - continue it now.

  import { easePacing, resumePacing, skipPacing } from "@/lib/stores/usage/usagePacing.svelte";

  const { sessionId, isStopped }: {
    sessionId: string;
    isStopped: boolean;
  } = $props();

  const actions = $derived(
    [
      {
        label: "Ease one step",
        run: easePacing,
        isShown: true
      },
      {
        label: "Skip this window",
        run: skipPacing,
        isShown: true
      },
      {
        label: "Resume now",
        run: resumePacing,
        isShown: isStopped
      }
    ].filter(action => action.isShown)
  );
</script>

<div class="actions">
  {#each actions as action (action.label)}
    <button class="menu-item" onclick={async () => await action.run(sessionId)} type="button">{action.label}</button>
  {/each}
</div>

<style>
  .actions {
    display: flex;
    flex-direction: column;
  }
</style>
