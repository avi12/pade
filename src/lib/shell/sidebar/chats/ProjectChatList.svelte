<script lang="ts" module>
  import { SvelteSet } from "svelte/reactivity";

  // The projects whose whole chat list the user asked to see. Shared by every
  // list in the window, so a project moving between Pinned and Recent (which
  // remounts it) keeps it; it lasts until the window reloads.
  const showingAllChats = new SvelteSet<string>();
</script>

<script lang="ts">
  import type { SidebarChat } from "@/lib/session/chats/chat-registry";
  import { shownChats } from "@/lib/shell/sidebar/chats/chat-activity";
  import ProjectChatRow from "@/lib/shell/sidebar/chats/ProjectChatRow.svelte";
  import { collapseRow, expandRow } from "@/lib/ui/motion";
  import { formatCount } from "@/lib/util/format";

  // An open project's chats, indented under its header: the first few, the
  // rest behind "Show N more" (and "Show less" to fold them back), or a quiet
  // note while it has none (its whole header line is the project's own).
  const {
    project,
    chats,
    activeChatId,
    now,
    onnewchat,
    onselectchat,
    onresumeconversation,
    ondeletechat,
    onstopchat
  }: {
    project: string;
    chats: readonly SidebarChat[];
    /** The focused chat, when it is one of these - it reads as active and
     *  never hides behind "Show N more". */
    activeChatId: string | null;
    now: number;
    onnewchat: () => void;
    onselectchat: (target: {
      chatId: string;
      shouldOpenNewWindow: boolean;
    }) => void;
    onresumeconversation: (target: {
      conversation: NonNullable<SidebarChat["resume"]>;
      shouldOpenNewWindow: boolean;
    }) => void;
    ondeletechat: (chatId: string) => void;
    onstopchat: (chatId: string) => void;
  } = $props();

  const SHOW_LESS = "Show less";

  const isShowingAll = $derived(showingAllChats.has(project));
  const listed = $derived(
    shownChats({
      chats,
      activeChatId,
      isShowingAll
    })
  );
  // Showing everything, and folding back would hide some.
  const isTrimmable = $derived(
    isShowingAll && shownChats({
      chats,
      activeChatId,
      isShowingAll: false
    }).hiddenCount > 0
  );
  // The link under a long list: how many more it opens, or folding it back.
  const moreLabel = $derived.by(() => {
    if (listed.hiddenCount > 0) {
      return `Show ${formatCount(listed.hiddenCount)} more`;
    }

    if (isTrimmable) {
      return SHOW_LESS;
    }

    return null;
  });
</script>

<ul class="chats" in:expandRow out:collapseRow>
  {#each listed.shown as chat (chat.id)}
    <!-- A conversation that turns up while the rail is open (a new chat, a
         handoff's successor, a transcript an agent just wrote) glides in, and
         one that goes glides out; the rows already there on first paint
         don't animate. -->
    <li in:expandRow out:collapseRow>
      <ProjectChatRow
        {chat}
        isActive={chat.id === activeChatId}
        {now}
        ondelete={() => ondeletechat(chat.id)}
        onnew={onnewchat}
        onresume={onresumeconversation}
        onselect={shouldOpenNewWindow => onselectchat({
          chatId: chat.id,
          shouldOpenNewWindow
        })}
        onstop={() => onstopchat(chat.id)}
      />
    </li>
  {:else}
    <li class="no-chats" in:expandRow out:collapseRow>No chats yet</li>
  {/each}
  {#if moreLabel !== null}
    <li data-sidebar-line>
      <button
        class="show-more"
        data-sidebar-item
        onclick={() => {
          if (isShowingAll) {
            showingAllChats.delete(project);
            return;
          }

          showingAllChats.add(project);
        }}
        tabindex="-1"
        type="button"
      >{moreLabel}</button>
    </li>
  {/if}
</ul>

<style>
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

    :global(:root[data-palette="cyberpunk"]) & {
      padding-block: 0.125rem 0.375rem;
    }
  }

  /* The quiet link that opens a long chat list the rest of the way. */
  .show-more {
    padding-block: 0.25rem;
    padding-inline: 0.4375rem;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--primary);
    font: inherit;
    font-weight: 600;
    font-size: 0.6875rem;
    cursor: pointer;

    &:hover,
    &:focus-visible {
      outline: none;
      text-decoration: underline;
    }

    :global(:root[data-palette="cyberpunk"]) & {
      text-shadow: var(--cyber-glow);
    }
  }

  .no-chats {
    margin: 0;
    padding-block: 4px;
    padding-inline: 8px;
    color: var(--on-surface-variant);
    font-size: 0.75rem;
  }
</style>
