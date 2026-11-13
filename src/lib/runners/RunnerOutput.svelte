<script lang="ts">
  import { linkSegments } from "@/lib/runners/runner-links";
  import type { RunnerRow } from "@/lib/stores/tasks/runners.svelte";
  import { openTerminalLink } from "@/lib/terminal/links/open-terminal-link";
  import { parseAnsi } from "@/lib/terminal/output/ansi";
  import { RunnerStream } from "@/lib/types/terminal";

  // A spawned runner's streamed output, its SGR styling kept and stderr tinted,
  // pinned to the newest line as it arrives.
  const { id, lines }: {
    id: RunnerRow["id"];
    lines: RunnerRow["lines"];
  } = $props();
</script>

<div
  style:view-transition-name={`runner-output-${id}`}
  class="output"
>
  <div class="lines">
    {#each lines as line (line.id)}
      <div
        class="line"
        class:error={line.stream === RunnerStream.enum.stderr}
      >{#each linkSegments(parseAnsi(line.text)) as segment, iSegment (iSegment)}{#if segment.url}
        {@const url = segment.url}<a
          style:color={segment.color}
          style:background={segment.background}
          class:bold={segment.bold}
          class:dim={segment.dim}
          class:italic={segment.italic}
          href={url}
          onclick={async e => {
            e.preventDefault();
            await openTerminalLink(url);
          }}
        >{segment.text}</a>{:else}<span
        style:color={segment.color}
        style:background={segment.background}
        class:bold={segment.bold}
        class:dim={segment.dim}
        class:italic={segment.italic}
        class:underline={segment.underline}
      >{segment.text || " "}</span>{/if}{/each}</div>
    {/each}
  </div>
</div>

<style>
  /* Reversed, so the scroll origin is the newest line: the pane stays pinned to
     it as lines stream in, and keeps its place once scrolled up - no script
     reading the full height (which laid out every line) on each arrival. */
  .output {
    display: flex;
    flex: 1;
    flex-direction: column-reverse;
    overflow: auto;
    min-block-size: 0;
    padding-block: 8px;
    padding-inline: 10px;
    background: var(--code-background);
    color: var(--code-foreground);
    font-family: var(--font-monospace);
    font-size: 0.75rem;
    line-height: 1.5;
  }

  /* Off-screen lines skip layout and paint until scrolled to - a runner keeps
     thousands of them. */
  .line {
    contain-intrinsic-block-size: auto 1.5em;
    content-visibility: auto;
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
      opacity: 0.7;
    }

    .italic {
      font-style: italic;
    }

    .underline {
      text-decoration: underline;
    }

    /* A URL keeps its SGR colour and reads as a link only on hover, like a
       terminal pane's. */
    a {
      color: inherit;
      text-decoration: none;

      &:hover {
        text-decoration: underline;
      }
    }
  }
</style>
