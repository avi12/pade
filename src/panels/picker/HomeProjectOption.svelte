<script lang="ts">
  import { tooltip } from "@/lib/ui/tooltip/truncation-tooltip";
  import { isTemporaryWorkspace, projectRootPrefix } from "@/lib/util/paths";
  import RootedName from "@/lib/workspace/display/RootedName.svelte";
  import TempBadge from "@/lib/workspace/display/TempBadge.svelte";

  // One project in the composer's menu: its root and name ("avi\pade"), the
  // TEMP badge for a throwaway workspace, a tick when picked, and its path in
  // the shared top-layer bubble - never a
  // `data-tooltip` pseudo, whose box would widen the scrolling list into a
  // sideways scrollbar.
  const { menuId, path, name, isPicked, onpick }: {
    menuId: string;
    path: string;
    name: string;
    isPicked: boolean;
    onpick: () => void;
  } = $props();
</script>

<li>
  <button
    class="menu-item"
    class:picked={isPicked}
    {@attach tooltip(path)}
    aria-current={isPicked}
    onclick={onpick}
    popovertarget={menuId}
    popovertargetaction="hide"
    type="button"
  >
    <span class="menu-item-text"><RootedName {name} prefix={projectRootPrefix(path)} /></span>
    {#if isTemporaryWorkspace(path)}
      <TempBadge />
    {/if}
    {#if isPicked}
      <span class="tick" aria-hidden="true">✓</span>
    {/if}
  </button>
</li>
