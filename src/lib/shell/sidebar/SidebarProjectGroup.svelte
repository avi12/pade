<script lang="ts" module>
  /** The attribute naming a pinned project to drag-to-reorder, on the
   *  list's wrapper around each project. */
  export const PIN_ID_ATTRIBUTE = "data-pin-id";
</script>

<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import type { SidebarChat, SidebarProject } from "@/lib/session/chats/chat-registry";
  import InactiveChatsFold from "@/lib/shell/sidebar/InactiveChatsFold.svelte";
  import ProjectChatRow from "@/lib/shell/sidebar/ProjectChatRow.svelte";
  import { collapseRow, expandRow } from "@/lib/ui/motion";
  import { displayName } from "@/lib/util/paths";
  import ProjectRowMain from "@/lib/workspace/switcher/projects/ProjectRowMain.svelte";
  import ProjectRowMenu from "@/lib/workspace/switcher/projects/ProjectRowMenu.svelte";
  import ReorderGrip from "@/lib/workspace/switcher/ReorderGrip.svelte";
  import type { ComponentProps } from "svelte";

  type RowMenuActions = Pick<
    ComponentProps<typeof ProjectRowMenu>,
    "isRenamable" | "ondelete" | "onrelabel" | "onremove" | "onrenamefolder" | "ontogglepin"
  >;

  // One project in the left rail: its rich header row (the switcher's row, with
  // the chat count, a new-chat button and the options kebab riding it), then its
  // chats and recorded conversations indented under it. A pinned project leads
  // with a drag grip while the pins can be reordered.
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
  }: {
    group: SidebarProject;
    labels: Record<string, string>;
    isPinned: boolean;
    /** The pins can be dragged into a new order right now. */
    isPinReorderable: boolean;
    isOpenElsewhere: boolean;
    now: number;
    /** This window's focused session - its chat row reads as active. */
    activeId: string | null;
    /** What the row's options kebab offers. */
    menu: RowMenuActions;
    onpick: (e: MouseEvent) => void;
    onreorderpins: (paths: string[]) => void;
    onnewchat: () => void;
    onselectchat: (target: {
      chatId: string;
      opensNewWindow: boolean;
    }) => void;
    onresumeconversation: (target: {
      conversation: NonNullable<SidebarChat["resume"]>;
      opensNewWindow: boolean;
    }) => void;
    ondeletechat: (chatId: string) => void;
    onstopchat: (chatId: string) => void;
  } = $props();

  const NEW_CHAT = "New chat";

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
  // Live chats always show; the inactive ones (stopped chats and recorded
  // conversations) sit in a fold under them.
  const liveChats = $derived(group.chats.filter(chat => chat.isLive));
  const inactiveChats = $derived(group.chats.filter(chat => !chat.isLive));
  const hasNoChats = $derived(group.chats.length === 0);
</script>

{#snippet chatRow(chat: SidebarChat)}
  <!-- A conversation that turns up while the rail is open (a new chat, a
       handoff's successor, a transcript an agent just wrote) glides in, and
       one that goes glides out; the rows already there on first paint
       don't animate. -->
  <li in:expandRow out:collapseRow>
    <ProjectChatRow
      {chat}
      isActive={chat.id === activeChatId}
      ondelete={() => ondeletechat(chat.id)}
      onnew={onnewchat}
      onresume={onresumeconversation}
      onselect={opensNewWindow => onselectchat({
        chatId: chat.id,
        opensNewWindow
      })}
      onstop={() => onstopchat(chat.id)}
    />
  </li>
{/snippet}

<!-- Opening in and closing up, and the pin id drag-to-reorder moves by, sit
     on the list's wrapper around it (ProjectSection), the pins' siblings. -->
<section class="group">
  <!-- One keyboard line: the header is the project's only tab stop, and the
       arrows reach its buttons (sidebar-keyboard). -->
  <div class="project-row" data-sidebar-line>
    <ReorderGrip
      idAttribute={PIN_ID_ATTRIBUTE}
      isEnabled={isPinReorderable}
      itemSelector="[{PIN_ID_ATTRIBUTE}]"
      oncommit={onreorderpins}
    />
    <ProjectRowMain
      chatCount={group.chats.length}
      isCurrent={group.isCurrent}
      {isOpenElsewhere}
      {labels}
      {now}
      {onpick}
      project={group.project}
    />
    <button
      class="new-chat"
      aria-label="{NEW_CHAT} in {name}"
      data-sidebar-item
      data-tooltip={NEW_CHAT}
      onclick={onnewchat}
      tabindex="-1"
      type="button"
    ><Icon name="plus" size={14} /></button>
    <ProjectRowMenu {...menu} {isPinned} {labels} project={group.project} />
  </div>

  <ul class="chats">
    {#each liveChats as chat (chat.id)}
      {@render chatRow(chat)}
    {/each}
    <!-- Closes as the first chat opens, so the group grows by the difference
         instead of shrinking and then growing back. -->
    {#if hasNoChats}
      <li class="no-chats" in:expandRow out:collapseRow>No chats yet</li>
    {/if}
  </ul>

  <InactiveChatsFold count={inactiveChats.length} hasLiveChats={liveChats.length > 0} project={group.project}>
    <ul class="chats">
      {#each inactiveChats as chat (chat.id)}
        {@render chatRow(chat)}
      {/each}
    </ul>
  </InactiveChatsFold>
</section>

<style>
  .group {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-inline-size: 0;
  }

  /* Project header - the switch target, with the new-chat button and the
     options kebab riding it. Both reveal fully on hover or focus. It sticks
     under the section label while its own chats scroll beneath it, and lets
     go when the next project's header pushes it out. */
  .project-row {
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
    }
  }

  .new-chat {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 26px;
    inline-size: 24px;
    padding: 0;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface-variant);
    opacity: var(--row-kebab-opacity, 55%);
    cursor: pointer;
    transition: background 120ms var(--ease), color 120ms var(--ease), opacity 120ms var(--ease);

    &:hover {
      background: var(--surface-3);
      color: var(--on-surface);
    }
  }

  /* Chats indent under their project - the grouping reads from position
     alone, no extra chrome per row. */
  .chats {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-inline-size: 0;
    margin: 0;
    padding: 0;
    padding-inline-start: 18px;
    list-style: none;
  }

  .no-chats {
    margin: 0;
    padding-block: 4px;
    padding-inline: 8px;
    color: var(--on-surface-variant);
    font-size: 0.75rem;
  }
</style>
