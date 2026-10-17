<script lang="ts">
  import { openLink } from "@/lib/dashboard/open-link";
  import Icon from "@/lib/Icon.svelte";
  import type { PullRequest } from "@/lib/types";

  // A project's open pull request, one click from its dashboard card - opened
  // in the browser, not by focusing the project window.
  const { pullRequest }: {
    pullRequest: PullRequest;
  } = $props();
</script>

<a
  class="pull-request"
  href={pullRequest.url}
  onclick={async e => {
    e.preventDefault();
    await openLink(e.currentTarget.href);
  }}
>
  <Icon name="github" size={13} />
  <span class="number">PR #{pullRequest.number}</span>
  <span class="title">{pullRequest.title}</span>
</a>

<style>
  .pull-request {
    display: flex;
    gap: 6px;
    align-items: center;
    min-inline-size: 0;
    color: var(--primary);
    font-size: 12px;
    text-decoration: none;

    .number {
      flex: none;
      font-family: var(--font-monospace);
      font-weight: 700;
    }

    .title {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    &:hover .title {
      text-decoration: underline;
    }
  }
</style>
