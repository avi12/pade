<script lang="ts">
  import { parseAnsi } from "@/lib/ansi";
  import type { RunnerRow } from "@/lib/stores/runners.svelte";
  import { RunnerStream } from "@/lib/types";

  // A spawned runner's streamed output, its SGR styling kept and stderr tinted,
  // pinned to the newest line as it arrives.
  const { id, lines }: {
    id: RunnerRow["id"];
    lines: RunnerRow["lines"];
  } = $props();

  // Keep the pane pinned to its newest line as it streams.
  function autoscroll(elOutput: HTMLElement) {
    const observer = new MutationObserver(() => {
      elOutput.scrollTop = elOutput.scrollHeight;
    });
    observer.observe(elOutput, {
      childList: true,
      subtree: true
    });
    return () => observer.disconnect();
  }
</script>

<div
  style:view-transition-name={`runner-output-${id}`}
  class="output"
  {@attach autoscroll}
>
  {#each lines as line, i (i)}
    <div
      class="line"
      class:error={line.stream === RunnerStream.enum.stderr}
    >{#each parseAnsi(line.text) as segment, iSegment (iSegment)}<span
      style:color={segment.color}
      style:background={segment.background}
      class:bold={segment.bold}
      class:dim={segment.dim}
      class:italic={segment.italic}
      class:underline={segment.underline}
    >{segment.text || " "}</span>{/each}</div>
  {/each}
</div>

<style>
  .output {
    flex: 1;
    overflow: auto;
    min-block-size: 0;
    padding-block: 8px;
    padding-inline: 10px;
    background: var(--code-background);
    color: var(--code-foreground);
    font-family: var(--font-monospace);
    font-size: 12px;
    line-height: 1.5;
  }

  .line {
    min-block-size: 1.5em;
    white-space: pre-wrap;
    animation: line-in 180ms var(--ease);

    /* stderr in a crit tint so failures read at a glance. It's the base colour the
       spans inherit - an SGR foreground colour overrides it per segment. */
    &.error {
      color: color-mix(in sRGB, var(--critical) 82%, var(--code-foreground));
    }

    /* Each segment span carries its SGR styles; colour/background come from the
       shared terminal palette via the `style:` directive. */
    .bold {
      font-weight: 700;
    }

    .dim {
      opacity: 70%;
    }

    .italic {
      font-style: italic;
    }

    .underline {
      text-decoration: underline;
    }
  }
</style>
