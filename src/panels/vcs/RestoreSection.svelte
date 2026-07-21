<script lang="ts">
  import { vcs } from "@/lib/bridge";
  import { formatPercent } from "@/lib/format";
  import Icon from "@/lib/Icon.svelte";
  import type { RestoreCandidate } from "@/lib/types";
  import { parseInput, RestoreQuery } from "@/lib/validate";

  const { project }: { project: string } = $props();

  // Restore a version — natural-language query → ranked prior commits →
  // checkout. Self-contained: the checkout touches the working tree, so the
  // panel's watcher-driven refresh picks up the result on its own.
  let restoreQuery = $state("");
  let candidates = $state<RestoreCandidate[]>([]);
  let restoreError = $state<string | null>(null);
  let restoreDone = $state<string | null>(null);
  let searching = $state(false);

  // Reset a prior repository's search/result when this lazy component receives
  // a new project from VcsPanel.
  $effect(() => {
    if (project) {
      restoreQuery = "";
      candidates = [];
      restoreError = null;
      restoreDone = null;
      searching = false;
    }
  });

  async function runRestore() {
    const query = parseInput({
      schema: RestoreQuery,
      raw: restoreQuery
    });
    if (!query) {
      return;
    }

    searching = true;
    restoreError = null;
    restoreDone = null;
    try {
      candidates = await vcs.restoreCandidates({
        cwd: project,
        query
      });
    } catch (e) {
      restoreError = String(e);
      candidates = [];
    } finally {
      searching = false;
    }
  }

  // Confidence as a 0..100 percentage — scores run 0..≈1.5, clamped for display.
  function confidencePct(score: number): number {
    return Math.round(Math.min(score / 1.5, 1) * 100);
  }
</script>

<section class="restore">
  <h3 class="eyebrow"><span class="lead"><Icon name="history" /></span> Restore a version</h3>
  <div class="restore-input">
    <input
      aria-label="Restore to a previous version"
      onkeydown={e => {
        const isSubmit = e.key === "Enter";
        if (isSubmit) {
          runRestore();
        }
      }}
      placeholder="e.g. last working version, before the meter change"
      type="text"
      bind:value={restoreQuery}
    />
    <button class="go" disabled={searching} onclick={runRestore}>Restore</button>
  </div>
  <p class="hint">
    PADE reads your local edit history and runs <code>git bisect</code> to trace back to the matching version.
  </p>

  {#if restoreError}
    <p class="restore-msg crit">{restoreError}</p>
  {/if}
  {#if restoreDone}
    <p class="restore-msg ok">Checked out on <code>{restoreDone}</code></p>
  {/if}

  {#if candidates.length}
    <ul class="candidates">
      {#each candidates as c (c.id)}
        <li>
          <button
            class="candidate"
            onclick={async () => {
              restoreError = null;
              try {
                const branch = await vcs.restoreCheckout({
                  cwd: project,
                  sha: c.id
                });
                restoreDone = branch;
                candidates = [];
              } catch (e) {
                restoreError = String(e);
              }
            }}
          >
            <div class="cand-top">
              <code class="sha">{c.short}</code>
              <span class="summary">{c.summary}</span>
            </div>
            <div class="cand-bot">
              <span class="by">{c.author} · {c.when}</span>
              <span class="conf" aria-label="Match confidence">
                <span class="bar"><span style:inline-size="{confidencePct(c.score)}%" class="fill"></span></span>
                <span class="pct">{formatPercent(confidencePct(c.score))}</span>
              </span>
            </div>
          </button>
        </li>
      {/each}
    </ul>
  {:else if searching}
    <p class="hint">Searching your history…</p>
  {:else if restoreQuery.trim() && !restoreDone && !restoreError}
    <p class="hint">No matching version found — try describing it differently.</p>
  {/if}
</section>

<style>
  .restore {
    display: flex;
    flex-direction: column;
    gap: 9px;
    padding: 12px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: var(--surface-1);

    .eyebrow {
      display: flex;
      gap: 7px;
      align-items: center;
      margin: 0;
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 11px;
      letter-spacing: 0.05em;
      text-transform: uppercase;
    }

    .lead {
      display: inline-flex;
      color: var(--primary);
    }

    .restore-input {
      display: flex;
      gap: 8px;
    }

    input {
      flex: 1;
      min-inline-size: 0;
      padding-block: 8px;
      padding-inline: 14px;
      border: 1px solid var(--outline);
      border-radius: 999px;
      background: var(--surface-2);
      color: var(--on-surface);
      font: inherit;
      font-size: 13px;

      &:focus-visible {
        border-color: var(--primary);
        outline: none;
      }
    }

    .go {
      flex: none;
      padding-block: 8px;
      padding-inline: 16px;
      border: none;
      border-radius: 999px;
      background: var(--primary);
      color: var(--on-primary);
      font-weight: 700;
      font-size: 13px;
      cursor: pointer;
      transition: filter 120ms var(--ease);

      &:hover:not(:disabled) {
        filter: brightness(1.06);
      }

      &:disabled {
        opacity: 60%;
        cursor: default;
      }
    }

    .hint {
      margin: 0;
      color: var(--on-surface-variant);
      font-size: 11px;
    }

    .hint code {
      font-family: var(--font-monospace);
    }

    .restore-msg {
      margin: 0;
      font-size: 12px;
    }

    .restore-msg code {
      font-family: var(--font-monospace);
      font-weight: 600;
    }

    .restore-msg.ok {
      color: var(--tertiary);
    }

    .restore-msg.crit {
      color: var(--critical);
      white-space: pre-wrap;
    }
  }

  .candidates {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .candidate {
    display: flex;
    flex-direction: column;
    gap: 4px;
    inline-size: 100%;
    padding-block: 8px;
    padding-inline: 10px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-small);
    background: var(--surface-2);
    text-align: start;
    cursor: pointer;
    transition: border-color 120ms var(--ease);
    animation: line-in 240ms var(--ease) both;

    &:hover {
      border-color: var(--primary);
    }

    .cand-top {
      display: flex;
      gap: 10px;
      align-items: baseline;
    }

    .summary {
      overflow: hidden;
      font-size: 13px;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .cand-bot {
      display: flex;
      gap: 10px;
      justify-content: space-between;
      align-items: center;
    }

    .conf {
      display: flex;
      flex: none;
      gap: 6px;
      align-items: center;
    }

    .bar {
      display: block;
      overflow: hidden;
      block-size: 4px;
      inline-size: 48px;
      border-radius: 999px;
      background: var(--surface-3);
    }

    .fill {
      display: block;
      block-size: 100%;
      border-radius: 999px;
      background: var(--tertiary);
    }

    .pct {
      color: var(--on-surface-variant);
      font-size: 11px;
      font-variant-numeric: tabular-nums;
    }
  }
</style>
