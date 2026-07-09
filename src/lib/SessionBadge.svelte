<script lang="ts">
  import type { SessionStatus } from "./types";

  // Reusable across the terminal session and, later, every node in the agent
  // tree — one place defines how a session's state looks and reads.
  const { status, label }: {
    status: SessionStatus;
    label?: string;
  } = $props();

  const TEXT: Record<SessionStatus, string> = {
    starting: "Starting…",
    working: "Working",
    ready: "Ready — waiting for you",
    exited: "Done — session ended"
  };
</script>

<!-- <output> carries role="status" + aria-live="polite" natively (rule 7). -->
<output class="badge {status}">
  <span class="dot"></span>
  {#if label}
    <span class="label">{label}</span>
  {/if}
  <span class="state">{TEXT[status]}</span>
</output>

<style>
  /* Nested CSS (rule 8): a component's rules live together, mirroring markup. */
  .badge {
    display: inline-flex;
    gap: 8px;
    align-items: center;
    color: var(--on-surface-var);
    font-size: 12px;

    .label {
      color: var(--on-surface);
      font-family: var(--font-mono);
      font-weight: 600;
    }

    .dot {
      flex: none;
      block-size: 9px;
      inline-size: 9px;
      border-radius: 50%;
      background: var(--outline);
    }

    /* Working: primary, pulsing. Ready: tertiary (green), steady — the "done"
       signal you're looking for. Exited: neutral. */
    &.working .dot {
      background: var(--primary);
      animation: pulse 1100ms var(--ease) infinite;
    }

    &.ready .dot {
      background: var(--tertiary);
      box-shadow: 0 0 0 4px color-mix(in sRGB, var(--tertiary) 25%, transparent);
    }

    &.ready .state {
      color: var(--tertiary);
      font-weight: 600;
    }

    &.exited .dot {
      background: var(--on-surface-var);
    }
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 100%;
    }

    50% {
      opacity: 35%;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .badge.working .dot {
      animation: none;
    }
  }
</style>
