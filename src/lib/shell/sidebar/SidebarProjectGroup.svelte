<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import type { SidebarChat, SidebarProject } from "@/lib/session/chat-registry";
  import ProjectChatRow from "@/lib/shell/sidebar/ProjectChatRow.svelte";
  import { displayName } from "@/lib/util/paths";
  import ProjectRowMain from "@/lib/workspace/switcher/ProjectRowMain.svelte";
  import ProjectRowMenu from "@/lib/workspace/switcher/ProjectRowMenu.svelte";
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
    onselectchat: (chatId: string) => void;
    onresumeconversation: (conversation: NonNullable<SidebarChat["resume"]>) => void;
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
  // Only this window's project has live sessions, so only its chat can be the
  // focused one.
  const activeChatId = $derived.by(() => {
    if (group.isCurrent) {
      return activeId;
    }

    return null;
  });
</script>

<section class="group" data-pin-id={isPinReorderable ? group.project : undefined}>
  <div class="project-row">
    <ReorderGrip
      idAttribute="data-pin-id"
      isEnabled={isPinReorderable}
      itemSelector="[data-pin-id]"
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
      data-tooltip={NEW_CHAT}
      onclick={onnewchat}
      type="button"
    ><Icon name="plus" size={14} /></button>
    <ProjectRowMenu {...menu} {isPinned} {labels} project={group.project} />
  </div>

  <ul class="chats">
    {#each group.chats as chat (chat.id)}
      <li>
        <ProjectChatRow
          {chat}
          isActive={chat.id === activeChatId}
          ondelete={() => ondeletechat(chat.id)}
          onnew={onnewchat}
          onresume={onresumeconversation}
          onselect={() => onselectchat(chat.id)}
          onstop={() => onstopchat(chat.id)}
        />
      </li>
    {:else}
      <li class="no-chats">No chats yet</li>
    {/each}
  </ul>
</section>

<style>
  .group {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-inline-size: 0;
  }

  /* Project header - the switch target, with the new-chat button and the
     options kebab riding it. Both reveal fully on hover or focus. */
  .project-row {
    position: relative;
    display: flex;
    gap: 1px;
    align-items: center;
    min-inline-size: 0;

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
    font-size: 12px;
  }
</style>
