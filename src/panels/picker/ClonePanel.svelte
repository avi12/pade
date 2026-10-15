<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import type { OpenTarget } from "@/lib/types";
  import CloneForm from "@/panels/picker/CloneForm.svelte";
  import { onMount } from "svelte";

  // Get started's Clone tab: git clone a repository into a root, gated on git
  // being installed - without it, a card offers the installer and a re-check.
  let { roots, onopen, onnewroot, createIn = $bindable("") }: {
    roots: string[];
    onopen: (target: OpenTarget) => void;
    onnewroot: () => void;
    createIn?: string;
  } = $props();

  const GIT_DOWNLOAD_URL = "https://git-scm.com/downloads";

  // `null` until the probe answers, so the panel is born knowing whether git
  // exists instead of flashing the wrong body.
  let gitInstalled = $state<boolean | null>(null);
  let hasSshKey = $state(false);

  // Shared by mount and the "Re-check" button after installing git.
  async function checkGit() {
    const [installed, keyOnDisk] = await Promise.all([
      sendMessage(Command.vcsGitInstalled),
      sendMessage(Command.vcsHasSshKey)
    ]);
    gitInstalled = installed;
    hasSshKey = keyOnDisk;
  }

  onMount(async () => await checkGit());
</script>

{#if gitInstalled !== null}
  {#if !gitInstalled}
    <div class="panel warning-card">
      <p class="warning-header">
        <span class="warning-icon" aria-hidden="true"><Icon name="alert" size={16} /></span>
        <strong>Git isn’t installed</strong>
      </p>
      <p class="hint">PADE needs Git to clone a repository. Install it, then press Re-check</p>
      <div class="actions">
        <button class="go" onclick={() => sendMessage(Command.openUrl, { url: GIT_DOWNLOAD_URL })} type="button">
          Install Git…
        </button>
        <button class="ghost" onclick={async () => await checkGit()} type="button">Re-check</button>
      </div>
    </div>
  {:else}
    <CloneForm {hasSshKey} {onnewroot} {onopen} {roots} bind:createIn />
  {/if}
{/if}

<style>
  /* Outlined secondary action (Re-check). */
  .ghost {
    padding: 9px 16px;
    border: 1px solid var(--outline);
    border-radius: var(--radius-medium);
    background: transparent;
    color: var(--on-surface);
    font: inherit;
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
    transition:
      color 150ms var(--ease),
      border-color 150ms var(--ease);

    &:hover {
      border-color: var(--primary);
      color: var(--primary);
      filter: none;
    }
  }
</style>
