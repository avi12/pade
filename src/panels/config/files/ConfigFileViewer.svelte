<script lang="ts">
  import { collectVars } from "@/lib/code/colors";
  import ColorText from "@/lib/code/ColorText.svelte";
  import { markdownDocument } from "@/lib/code/markdown";
  import SandboxedPreview from "@/lib/code/SandboxedPreview.svelte";
  import { isMarkdownPath } from "@/lib/feed/preview/preview";
  import type { ConfigFile } from "@/lib/types";
  import Segmented from "@/lib/ui/controls/Segmented.svelte";

  // A read-only view of the selected config file: highlighted source, or - for
  // markdown - a rendered preview on request.
  const { file, content }: {
    file: ConfigFile | null;
    content: string;
  } = $props();

  // Trace var(--x) swatches against the file's own token definitions first.
  const fileVars = $derived(collectVars(content));

  // A markdown file reads as its source or rendered; the pick holds as the user
  // moves between files.
  const ViewerPane = {
    Source: "source",
    Preview: "preview"
  } as const;
  type ViewerPane = (typeof ViewerPane)[keyof typeof ViewerPane];
  const viewerPanes = [
    {
      value: ViewerPane.Source,
      label: "Source"
    },
    {
      value: ViewerPane.Preview,
      label: "Preview"
    }
  ] as const;
  let viewerPane = $state<ViewerPane>(ViewerPane.Source);
  const heading = $derived(file?.rel ?? "Select a file to view");
  const isMarkdownFile = $derived(isMarkdownPath(file?.rel ?? ""));
  const isShowingPreview = $derived(isMarkdownFile && viewerPane === ViewerPane.Preview);
</script>

<section class="viewer">
  <div class="frame">
    <div class="frame-header">
      <h3 class:placeholder={!file}>{heading}</h3>
      {#if isMarkdownFile}
        <Segmented
          chosen={viewerPane}
          label="Show the file as"
          onchoose={pane => {
            viewerPane = pane;
          }}
          options={viewerPanes}
        />
      {/if}
    </div>
    {#if isShowingPreview}
      <SandboxedPreview document={markdownDocument(content)} label="Preview of {heading}" />
    {:else}
      <pre class="body"><ColorText
          markdown={isMarkdownFile}
          text={content}
          vars={fileVars}
        /></pre>
    {/if}
  </div>
  <p class="note">Read-only in the MVP - edits will write back to this same file</p>
</section>

<style>
  .viewer {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-block-start: 6px;

    .frame {
      --preview-height: 280px;

      overflow: hidden;
      border: 1px solid var(--outline);
      border-radius: var(--radius-medium);
    }

    .frame-header {
      display: flex;
      gap: 8px;
      justify-content: space-between;
      align-items: center;
      padding-block: 4px;
      padding-inline: 12px 4px;
      background: var(--surface-2);
    }

    h3 {
      overflow: hidden;
      min-inline-size: 0;
      margin: 0;
      padding-block: 4px;
      color: var(--on-surface);
      font-family: var(--font-monospace);
      font-weight: 600;
      font-size: 0.75rem;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .placeholder {
      color: var(--on-surface-variant);
      font-style: italic;
    }

    .body {
      overflow: auto;
      max-block-size: 280px;
      margin: 0;
      padding: 12px;
      background: var(--code-background);
      color: var(--code-foreground);
      font-family: var(--font-monospace);
      font-size: 0.75rem;
      line-height: 1.55;
      white-space: pre-wrap;
    }

    .note {
      margin-block: 2px 0;
      margin-inline: 0;
      color: var(--on-surface-variant);
      font-style: italic;
      font-size: 0.6875rem;
    }
  }
</style>
