<script lang="ts">
  // The Clone tab's repository URL field, with the live reachability verdict
  // under it: a quiet note while the repository is probed, an error once it
  // answered that it can't be reached. The probe itself lives in CloneForm.
  let { isChecking, isUnreachable, url = $bindable("") }: {
    isChecking: boolean;
    isUnreachable: boolean;
    url?: string;
  } = $props();
</script>

<div class="new-project-field">
  <label class="new-project-label" for="clone-url">Repository URL</label>
  <input
    id="clone-url"
    class="path-input framed"
    autocomplete="off"
    placeholder="https://github.com/org/repo.git  ·  git@github.com:org/repo.git"
    spellcheck="false"
    bind:value={url}
  />
  {#if isChecking}
    <output class="probe-note">Checking the repository…</output>
  {:else if isUnreachable}
    <output class="field-error">Can’t reach that repository - check the URL or your access</output>
  {/if}
</div>

<style>
  /* Quiet in-flight note under the URL field while the repository is probed. */
  .probe-note {
    display: block;
    color: var(--on-surface-variant);
    font-size: 0.75rem;
    line-height: 1.4;
  }
</style>
