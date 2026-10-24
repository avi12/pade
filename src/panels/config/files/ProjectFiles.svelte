<script lang="ts">
  import { Command, EventType, onMessage, sendMessage } from "@/lib/ipc/messaging";
  import type { ConfigFile } from "@/lib/types";
  import { settledValue } from "@/lib/util/settled";
  import { isListedConfigFile } from "@/panels/config/files/config-files";
  import ConfigFileViewer from "@/panels/config/files/ConfigFileViewer.svelte";
  import SectionLabel from "@/panels/config/SectionLabel.svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount } from "svelte";

  // The config files relevant to the active agent, and the viewer for the one
  // picked. Both stay live as agents (or anyone) edit the files.
  const { agent }: { agent: string } = $props();

  let files = $state<ConfigFile[]>([]);
  let selected = $state<ConfigFile | null>(null);
  let content = $state("");
  const selectedRel = $derived(selected?.rel);

  /** Lists the agent's files, or null when the agent changed mid-request - its
   *  answer is superseded and must not land. */
  async function listFiles(requestedAgent: string) {
    const listed = await sendMessage(Command.configList, { agent: requestedAgent });
    if (requestedAgent !== agent) {
      return null;
    }

    files = listed;
    return listed;
  }

  async function readContent(file: ConfigFile) {
    if (!file.exists) {
      return "";
    }

    return await settledValue({
      promise: sendMessage(Command.configRead, { rel: file.rel }),
      fallback: ""
    });
  }

  async function open(file: ConfigFile) {
    selected = file;
    const text = await readContent(file);
    // The read can resolve out of order relative to a later selection; only
    // apply it while this file is still the selected one.
    if (selected?.rel === file.rel) {
      content = text;
    }
  }

  async function openFirstFile(requestedAgent: string) {
    const listed = await listFiles(requestedAgent);
    const first = listed?.find(file => file.exists);
    if (first) {
      await open(first);
    }
  }

  // `agent` is a reactive prop and the panel is not remounted when the active
  // agent changes, so reload the file list whenever it does.
  $effect(() => {
    const requestedAgent = agent;
    selected = null;
    content = "";
    files = [];
    openFirstFile(requestedAgent);
  });

  // Keep the listed files live: when one of them is written, re-list them - a
  // file can appear or vanish - and re-read the one on show.
  let unlistenFileChanges: UnlistenFn | undefined;
  onMount(async () => {
    unlistenFileChanges = await onMessage(EventType.feedChange, async ({ data }) => {
      if (!isListedConfigFile({
        changedPath: data.path,
        files
      })) {
        return;
      }

      const listed = await listFiles(agent);
      const shown = listed?.find(file => file.rel === selected?.rel);
      if (shown) {
        await open(shown);
      }
    });
  });
  onDestroy(() => unlistenFileChanges?.());
</script>

{#if files.length}
  <SectionLabel text="Project files" />
{/if}

{#each files as file (file.rel)}
  <button
    class="row"
    class:selected={selectedRel === file.rel}
    disabled={!file.exists}
    onclick={() => open(file)}
  >
    <span class="kind {file.kind}">{file.kind}</span>
    <span class="relative-path">{file.rel}</span>
    {#if !file.exists}
      <span class="missing">absent</span>
    {/if}
  </button>
{/each}

<ConfigFileViewer {content} file={selected} />

<style>
  .row {
    display: flex;
    gap: 10px;
    align-items: center;
    inline-size: 100%;
    padding-block: 8px;
    padding-inline: 10px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface);
    text-align: start;
    cursor: pointer;
    transition: background 140ms var(--ease);

    &:disabled {
      opacity: 45%;
      cursor: default;
    }

    &:hover:not(:disabled) {
      background: var(--surface-2);
    }

    &.selected {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }

  .kind {
    flex: none;
    padding-block: 2px;
    padding-inline: 9px;
    border-radius: var(--radius-full);
    background: var(--surface-3);
    color: var(--on-surface-variant);
    font-weight: 700;
    font-size: 0.625rem;
    letter-spacing: 0.05em;
    text-transform: uppercase;

    &.instructions {
      background: var(--tertiary-wash);
      color: var(--tertiary);
    }

    &.mcp {
      background: var(--primary-container);
      color: var(--on-primary-container);
    }
  }

  .relative-path {
    overflow: hidden;
    font-family: var(--font-monospace);
    font-size: 0.75rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .missing {
    margin-inline-start: auto;
    color: var(--on-surface-variant);
    font-style: italic;
    font-size: 0.625rem;
  }
</style>
