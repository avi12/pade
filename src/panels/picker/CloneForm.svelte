<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { Command, sendMessage } from "@/lib/ipc/messaging";
  import type { OpenTarget } from "@/lib/types";
  import {
    CloneUrl,
    GitSecret,
    GitUsername,
    isSshCloneUrl,
    ProjectName,
    repositoryFolderName
  } from "@/lib/validate";
  import { cloneRepository } from "@/panels/picker/clone-repository";
  import CloneCredentials from "@/panels/picker/CloneCredentials.svelte";
  import CloneUrlField from "@/panels/picker/CloneUrlField.svelte";
  import RootSelect from "@/panels/picker/RootSelect.svelte";

  // The Clone tab's form, once git is known to be installed: repository URL
  // (probed live for reachability), target root + folder name (auto-filled from
  // the repository), and HTTPS credentials when SSH can't authenticate.
  let { roots, hasSshKey, onopen, onnewroot, createIn = $bindable("") }: {
    roots: string[];
    hasSshKey: boolean;
    onopen: (target: OpenTarget) => void;
    onnewroot: () => void;
    createIn?: string;
  } = $props();

  const REMOTE_PROBE_DEBOUNCE_MILLISECONDS = 500;

  let cloneUrl = $state("");
  let cloneName = $state("");
  // Auto-fill the folder name from the URL only until the user edits it.
  let isCloneNameEdited = $state(false);
  let cloneUsername = $state("");
  let clonePassword = $state("");
  let isCloning = $state(false);
  let cloneError = $state("");

  // The latest reachability answer, tagged with the URL it described - the
  // folder name auto-fills only once the repository itself has answered, not
  // from URL shape alone.
  let remoteProbe = $state<{
    url: string;
    reachable: boolean;
  }>({
    url: "",
    reachable: false
  });

  const cloneUrlValid = $derived(CloneUrl.safeParse(cloneUrl).success);
  const cloneNameValid = $derived(ProjectName.safeParse(cloneName).success);
  const remoteSettled = $derived(cloneUrlValid && remoteProbe.url === cloneUrl.trim());
  const remoteReachable = $derived(remoteSettled && remoteProbe.reachable);
  const remoteChecking = $derived(cloneUrlValid && !remoteSettled);
  const remoteUnreachable = $derived(remoteSettled && !remoteProbe.reachable);
  // An SSH-style URL with no key on disk can't authenticate - offer HTTPS
  // credentials instead (the backend rewrites the URL for the clone).
  const needsCredentials = $derived(cloneUrlValid && isSshCloneUrl(cloneUrl) && !hasSshKey);
  const credentialsMissing = $derived(
    needsCredentials
    && !(GitUsername.safeParse(cloneUsername).success && GitSecret.safeParse(clonePassword).success)
  );
  const cloneDisabled = $derived(
    !cloneUrlValid || !createIn || !cloneNameValid || credentialsMissing || isCloning
  );
  const submitLabel = $derived.by(() => {
    if (isCloning) {
      return "Cloning…";
    }

    return "Clone";
  });

  // Debounced live probe: does the typed repository exist, and can this
  // machine's auth see it? An answer, not a guess - the same philosophy as the
  // add-root path probe, pointed at the network.
  $effect(() => {
    const url = cloneUrl.trim();
    if (!CloneUrl.safeParse(url).success) {
      remoteProbe = {
        url: "",
        reachable: false
      };
      return;
    }

    const timer = setTimeout(async () => {
      const reachable = await sendMessage(Command.vcsProbeRemote, { url });
      remoteProbe = {
        url,
        reachable
      };
    }, REMOTE_PROBE_DEBOUNCE_MILLISECONDS);
    return () => clearTimeout(timer);
  });

  $effect(() => {
    if (isCloneNameEdited) {
      return;
    }

    cloneName = remoteReachable ? repositoryFolderName(cloneUrl) : "";
  });
</script>

<form
  class="panel"
  onsubmit={async e => {
    e.preventDefault();

    if (cloneDisabled) {
      return;
    }

    isCloning = true;
    cloneError = "";
    cloneError = await cloneRepository({
      rawUrl: cloneUrl,
      rawName: cloneName,
      root: createIn,
      needsCredentials,
      rawUsername: cloneUsername,
      rawPassword: clonePassword,
      onopen
    });
    isCloning = false;
  }}
>
  <CloneUrlField isChecking={remoteChecking} isUnreachable={remoteUnreachable} bind:url={cloneUrl} />

  <div class="new-project-field">
    <span id="clone-location-label" class="new-project-label">
      Clone into <span class="new-project-optional">- folder name auto-filled from the repo</span>
    </span>
    <div
      style:anchor-name="--new-project-root-menu-clone"
      class="new-project-location text-field-plate"
      aria-labelledby="clone-location-label"
      role="group"
    >
      <span class="new-project-location-icon" aria-hidden="true"><Icon name="folder" /></span>
      <RootSelect menuId="new-project-root-menu-clone" {onnewroot} {roots} bind:value={createIn} />
      <span class="new-project-separator" aria-hidden="true">\</span>
      <label class="visually-hidden" for="clone-name">Folder name</label>
      <input
        id="clone-name"
        class="new-project-name"
        autocomplete="off"
        oninput={() => (isCloneNameEdited = true)}
        placeholder="repo"
        spellcheck="false"
        bind:value={cloneName}
      />
    </div>
  </div>

  {#if needsCredentials}
    <CloneCredentials bind:username={cloneUsername} bind:password={clonePassword} />
  {/if}

  {#if cloneError}
    <output class="field-error">{cloneError}</output>
  {/if}

  <button class="go" disabled={cloneDisabled} type="submit">{submitLabel}</button>
</form>

<style>
  .new-project-optional {
    font-weight: 400;
    opacity: 75%;
  }

  .new-project-location .new-project-separator {
    flex: none;
    color: var(--on-surface-variant);
    font-family: var(--font-monospace);
    font-size: 0.8125rem;
  }

  .new-project-name {
    /* Wide enough for its own "project-name" placeholder so the hint never
       clips yet shares the root's line; typing scrolls, and only a genuinely
       long root wraps the name below. */
    --new-project-name-min-inline-size: 12ch;

    flex: 1 1 var(--new-project-name-min-inline-size);
    min-inline-size: var(--new-project-name-min-inline-size);
    font-family: var(--font-monospace);
  }
</style>
