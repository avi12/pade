<script lang="ts">
  import type { watchSummary } from "@/lib/feed/feed-status";
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import { showToast } from "@/lib/stores/ui/toast.svelte";

  // The empty panel's self-description: an empty list is ambiguous (nothing
  // changed, the watch is on another project, or there is no watch), so it
  // states which - and, when nothing watches this project, offers to arm it.
  const { summary, project, onArmed }: {
    summary: ReturnType<typeof watchSummary>;
    project: string;
    /** Re-read what the backend reports once an arm attempt has settled. */
    onArmed: () => Promise<void>;
  } = $props();

  let isArming = $state(false);
</script>

<div class="empty">
  <p class="empty-headline">{summary.headline}</p>
  <p class="empty-detail">{summary.detail}</p>
  {#if summary.isStalled}
    <button
      class="arm"
      disabled={isArming}
      onclick={async () => {
        isArming = true;
        try {
          await sendMessage(Command.watchStart, { root: project });
        } catch (error) {
          // The backend's refusal names the path and the reason - the one thing
          // worth reading when a project can't be watched.
          showToast(String(error));
        }

        await onArmed();
        isArming = false;
      }}
    >
      {#if isArming}
        Arming…{:else}Watch this project{/if}
    </button>
  {/if}
</div>

<style>
  .empty {
    display: flex;
    flex-direction: column;
    gap: 6px;
    align-items: flex-start;
    margin: 16px;
    color: var(--on-surface-variant);
    font-size: 13px;
    line-height: 1.5;

    .empty-headline {
      margin: 0;
      color: var(--on-surface);
      font-weight: 600;
    }

    .empty-detail {
      margin: 0;
    }

    /* Only shown when there is nothing watching this project - the one state
       the reader can act on. */
    .arm {
      margin-block-start: 4px;
      padding: 5px 14px;
      border: none;
      border-radius: var(--radius-full);
      background: var(--primary-container);
      color: var(--on-primary-container);
      font-family: inherit;
      font-weight: 600;
      font-size: 12px;
      cursor: pointer;

      &:disabled {
        opacity: 60%;
        cursor: default;
      }
    }
  }
</style>
