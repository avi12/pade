<script lang="ts">
  import Icon from "@/lib/Icon.svelte";

  // The SSH→HTTPS fallback: an SSH-style URL with no key on disk can't
  // authenticate, so the Clone tab asks for HTTPS credentials instead (the
  // backend rewrites the URL for the clone).
  let { username = $bindable(""), password = $bindable("") }: {
    username?: string;
    password?: string;
  } = $props();
</script>

<div class="warning-card credentials">
  <p class="warning-header">
    <span class="warning-icon" aria-hidden="true"><Icon name="alert" size={16} /></span>
    <span>No SSH key set up for this host - sign in to clone over HTTPS instead</span>
  </p>
  <div class="new-project-field">
    <label class="new-project-label" for="clone-user">Email or username</label>
    <input
      id="clone-user"
      class="text-field"
      autocomplete="username"
      placeholder="you@example.com"
      spellcheck="false"
      bind:value={username}
    />
  </div>
  <div class="new-project-field">
    <label class="new-project-label" for="clone-password">Password or access token</label>
    <input
      id="clone-password"
      class="text-field"
      autocomplete="current-password"
      placeholder="••••••••••••"
      type="password"
      bind:value={password}
    />
  </div>
</div>
