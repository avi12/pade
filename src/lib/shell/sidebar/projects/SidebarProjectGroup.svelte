<script lang="ts" module>
  import { SvelteMap } from "svelte/reactivity";

  /** The attribute naming a pinned project to drag-to-reorder, on the
   *  list's wrapper around each project. */
  export const PIN_ID_ATTRIBUTE = "data-pin-id";

  // The projects the user opened or closed by their caret. Shared by every
  // group in the window, so a project moving between Pinned and Recent (which
  // remounts it) keeps it; it lasts until the window reloads.
  const expandedByUser = new SvelteMap<string, boolean>();
</script>

<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import type { SidebarProject } from "@/lib/session/chats/chat-registry";
  import { activityCounts, chatActivity, chatStatus } from "@/lib/shell/sidebar/chats/chat-activity";
  import ProjectChatList from "@/lib/shell/sidebar/chats/ProjectChatList.svelte";
  import { displayName } from "@/lib/util/paths";
  import { isNewWindowClick } from "@/lib/workspace/open-windows";
  import ProjectRowMain from "@/lib/workspace/switcher/projects/ProjectRowMain.svelte";
  import ProjectRowMenu from "@/lib/workspace/switcher/projects/ProjectRowMenu.svelte";
  import ReorderGrip from "@/lib/workspace/switcher/ReorderGrip.svelte";
  import type { ComponentProps } from "svelte";

  type RowMenuActions = Pick<
    ComponentProps<typeof ProjectRowMenu>,
    "isRenamable" | "ondelete" | "onrelabel" | "onremove" | "onrenamefolder" | "ontogglepin"
  >;
  type ChatListActions = Pick<
    ComponentProps<typeof ProjectChatList>,
    "onnewchat" | "onselectchat" | "onresumeconversation" | "ondeletechat" | "onstopchat"
  >;

  // One project in the left rail: its header row (the switcher's row with its
  // needs-you and working counts, which opens and closes the project like its
  // caret does - a new-window click still opens the project in a new window -
  // a new-chat button, and the options menu on hover or right-click), then,
  // while open, its chats indented under it. A pinned project leads with a
  // drag grip while the pins can be reordered. Closed, it keeps only its live
  // chats.
  const {
    group,
    labels,
    isPinned,
    isPinReorderable,
    isOpenElsewhere,
    now,
    activeId,
    menu,
    onpick,
    onreorderpins,
    onnewchat,
    onselectchat,
    onresumeconversation,
    ondeletechat,
    onstopchat
  }: ChatListActions & {
    group: SidebarProject;
    labels: Record<string, string>;
    isPinned: boolean;
    /** The pins can be dragged into a new order right now. */
    isPinReorderable: boolean;
    isOpenElsewhere: boolean;
    now: number;
    /** This window's focused session - its chat row reads as active. */
    activeId: string | null;
    /** What the row's options menu offers. */
    menu: RowMenuActions;
    onpick: (e: MouseEvent) => void;
    onreorderpins: (paths: string[]) => void;
  } = $props();

  const NEW_CHAT = "New chat";
  const EXPAND = "Show chats";
  const COLLAPSE = "Hide inactive chats";

  let rowMenu = $state<ProjectRowMenu | null>(null);

  const name = $derived(
    displayName({
      path: group.project,
      labels
    })
  );
  // The focused session belongs to the project in front, so only its chat can
  // be the focused one.
  const activeChatId = $derived.by(() => {
    if (group.isCurrent) {
      return activeId;
    }

    return null;
  });
  const hasLiveChats = $derived(group.chats.some(chat => chat.isLive));
  // Open until the user says otherwise when it is in front or runs anything;
  // a project with nothing running starts closed.
  const isExpanded = $derived(expandedByUser.get(group.project) ?? (group.isCurrent || hasLiveChats));
  // Closed, a project still lists its live chats - only the rest fold away.
  const listedChats = $derived.by(() => {
    if (isExpanded) {
      return group.chats;
    }

    return group.chats.filter(chat => chat.isLive);
  });
  const counts = $derived(
    activityCounts(
      group.chats.map(chat => chatActivity({
        isLive: chat.isLive,
        status: chatStatus(chat)
      }))
    )
  );
  const caretLabel = $derived.by(() => {
    if (isExpanded) {
      return COLLAPSE;
    }

    return EXPAND;
  });
</script>

<!-- Opening in and closing up, and the pin id drag-to-reorder moves by, sit
     on the list's wrapper around it (ProjectSection), the pins' siblings. -->
<section class="group">
  <!-- One keyboard line: the header is the project's only tab stop, and the
       arrows reach its buttons (sidebar-keyboard) - the caret after the
       project's own button, though it is drawn before it. -->
  <div class="project-row" class:current={group.isCurrent} data-sidebar-line>
    <ReorderGrip
      idAttribute={PIN_ID_ATTRIBUTE}
      isEnabled={isPinReorderable}
      itemSelector="[{PIN_ID_ATTRIBUTE}]"
      oncommit={onreorderpins}
    />
    <ProjectRowMain
      isCurrent={group.isCurrent}
      {isExpanded}
      {isOpenElsewhere}
      {labels}
      needsYouCount={counts.needsYou}
      {now}
      oncontextmenu={e => {
        e.preventDefault();
        rowMenu?.open();
      }}
      onpick={e => {
        if (isNewWindowClick(e)) {
          onpick(e);
          return;
        }

        expandedByUser.set(group.project, !isExpanded);
      }}
      project={group.project}
      workingCount={counts.working}
    />
    <button
      class="caret"
      class:expanded={isExpanded}
      aria-expanded={isExpanded}
      aria-label="{caretLabel}: {name}"
      data-sidebar-item
      onclick={() => expandedByUser.set(group.project, !isExpanded)}
      tabindex="-1"
      type="button"
    ><Icon name="chevronDown" size={12} /></button>
    <button
      class="new-chat"
      aria-label="{NEW_CHAT} in {name}"
      data-sidebar-item
      data-tooltip={NEW_CHAT}
      onclick={onnewchat}
      tabindex="-1"
      type="button"
    ><Icon name="plus" size={14} /></button>
    <ProjectRowMenu bind:this={rowMenu} {...menu} {isPinned} {labels} project={group.project} />
  </div>

  {#if isExpanded || hasLiveChats}
    <ProjectChatList
      {activeChatId}
      chats={listedChats}
      {now}
      {ondeletechat}
      {onnewchat}
      {onresumeconversation}
      {onselectchat}
      {onstopchat}
      project={group.project}
    />
  {/if}
</section>

<style>
  .group {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-inline-size: 0;
  }

  /* Project header - the switch target, with the caret before it and the
     new-chat button and options menu riding it. The menu opens out on hover
     or focus. It sticks under the section label while its own chats scroll
     beneath it, and lets go when the next project's header pushes it out. */
  .project-row {
    --row-kebab-inline-size: 0px;

    position: sticky;
    inset-block-start: var(--section-head-block-size, 0);
    z-index: 1;
    display: flex;
    gap: 1px;
    align-items: center;
    min-inline-size: 0;
    background: var(--surface-1);

    &:hover,
    &:focus-within {
      --row-kebab-opacity: 100%;
      --row-kebab-inline-size: 1.5rem;
    }

    /* Cyberpunk: the project in front is one flat lit strip - a raised fill
       under the whole header, buttons and all, led by the red accent rail. */
    :global(:root[data-palette="cyberpunk"]) &.current {
      background: var(--surface-2);
      box-shadow: inset 3px 0 0 0 var(--cyber-accent);
    }
  }

  .caret,
  .new-chat {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    padding: 0;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface-variant);
    cursor: pointer;
    transition: background 120ms var(--ease), color 120ms var(--ease), opacity 120ms var(--ease);

    &:hover {
      background: var(--surface-3);
      color: var(--on-surface);
    }
  }

  /* Drawn first, though it follows the project's button for the arrows. It
     points right while the project is closed and down while open. A bare
     glyph: pointing at it only brightens it, no box of its own, and it says
     what it does to a screen reader rather than in a bubble. */
  .caret {
    order: -1;
    block-size: 1.5rem;
    inline-size: 1.125rem;
    margin-inline-start: 0.25rem;
    font-size: 0.625rem;

    &:hover {
      background: transparent;
    }

    > :global(svg) {
      transition: rotate 200ms var(--ease);
      rotate: -90deg;

      @media (prefers-reduced-motion: reduce) {
        transition: none;
      }
    }

    &.expanded > :global(svg) {
      rotate: 0deg;
    }
  }

  .new-chat {
    block-size: 26px;
    inline-size: 24px;
    opacity: var(--row-kebab-opacity, 55%);

    /* Cyberpunk: a square hit target, lit cyan on hover like the design. The
       icon jitters, not the button: a translated button would hold its own
       tooltip's fixed bubble and draw it over the button. */
    :global(:root[data-palette="cyberpunk"]) & {
      block-size: 1.5rem;
      inline-size: 1.5rem;
      font-size: 0.75rem;

      @media (prefers-reduced-motion: no-preference) {
        &:hover > :global(*) {
          animation: var(--cyber-jitter-animation);
        }
      }

      &:hover {
        background: var(--primary-container);
        color: var(--on-primary-container);
      }
    }
  }
</style>
