<script lang="ts">
  import type { SidebarProject } from "@/lib/session/chat-registry";
  import type { sidebarSections } from "@/lib/shell/sidebar/sidebar-sections";
  import SidebarProjectGroup from "@/lib/shell/sidebar/SidebarProjectGroup.svelte";
  import type { RecordedConversation, WindowInfo } from "@/lib/types";
  import { isOpenInAnotherWindow } from "@/lib/workspace/open-windows";
  import ProjectSection from "@/lib/workspace/switcher/ProjectSection.svelte";

  // The rail's scrolling project list: the Pinned and Recent sections (already
  // narrowed by the search), one group per project. Every action only reports
  // back - the rail owns the prompts and the settings requests.
  const {
    sections,
    isFiltered,
    labels,
    now,
    activeId,
    windowRows,
    onpick,
    onnewchat,
    onselectchat,
    onresumeconversation,
    ondeletechat,
    ontogglepin,
    onreorderpins,
    onrelabel,
    onrenamefolder,
    onremove,
    ondeleteproject,
    onclearrecent
  }: {
    sections: ReturnType<typeof sidebarSections>;
    /** A search narrows the rows. */
    isFiltered: boolean;
    labels: Record<string, string>;
    now: number;
    activeId: string | null;
    /** The open windows - a project open in another one is marked, focused
     *  there when picked, and can't be renamed from here. */
    windowRows: WindowInfo[];
    onpick: (target: {
      project: string;
      event: MouseEvent;
    }) => void;
    onnewchat: (project: string) => void;
    onselectchat: (target: {
      project: string;
      chatId: string;
    }) => void;
    onresumeconversation: (target: {
      project: string;
      conversation: RecordedConversation;
    }) => void;
    ondeletechat: (target: {
      project: string;
      chatId: string;
    }) => void;
    ontogglepin: (target: {
      path: string;
      pinned: boolean;
    }) => void;
    onreorderpins: (paths: string[]) => void;
    onrelabel: (project: string) => void;
    onrenamefolder: (project: string) => void;
    onremove: (project: string) => void;
    ondeleteproject: (project: string) => void;
    onclearrecent: () => void;
  } = $props();

  const NO_MATCHES = "No projects match";
  const NO_PROJECTS = "No projects yet - open one to start a chat";

  // Pins reorder only when there's more than one and no search narrows the set -
  // the drag engine commits the visible rows, so a filtered subset would drop
  // the hidden pins from the saved order.
  const isPinListReorderable = $derived(!isFiltered && sections.pinned.length > 1);
  const isEmpty = $derived(sections.pinned.length === 0 && sections.recent.length === 0);
  const emptyMessage = $derived.by(() => {
    if (!isEmpty) {
      return null;
    }

    if (isFiltered) {
      return NO_MATCHES;
    }

    return NO_PROJECTS;
  });
</script>

{#snippet group(entry: SidebarProject, isPinned: boolean, isPinReorderable: boolean)}
  {@const isOpenElsewhere = isOpenInAnotherWindow({
    windowRows,
    path: entry.project
  })}
  <SidebarProjectGroup
    {activeId}
    group={entry}
    {isOpenElsewhere}
    {isPinReorderable}
    {isPinned}
    {labels}
    menu={{
      isRenamable: !isOpenElsewhere,
      ondelete: () => ondeleteproject(entry.project),
      onrelabel: () => onrelabel(entry.project),
      onremove: () => onremove(entry.project),
      onrenamefolder: () => onrenamefolder(entry.project),
      ontogglepin: () => ontogglepin({
        path: entry.project,
        pinned: !isPinned
      })
    }}
    {now}
    ondeletechat={chatId => ondeletechat({
      project: entry.project,
      chatId
    })}
    onnewchat={() => onnewchat(entry.project)}
    onpick={e => onpick({
      project: entry.project,
      event: e
    })}
    {onreorderpins}
    onresumeconversation={conversation => onresumeconversation({
      project: entry.project,
      conversation
    })}
    onselectchat={chatId => onselectchat({
      project: entry.project,
      chatId
    })}
  />
{/snippet}

{#snippet pinnedGroup(entry: SidebarProject)}
  {@render group(entry, true, isPinListReorderable)}
{/snippet}

{#snippet recentGroup(entry: SidebarProject)}
  {@render group(entry, false, false)}
{/snippet}

<div class="groups">
  <ProjectSection heading="Pinned" items={sections.pinned} key={entry => entry.project} row={pinnedGroup} />
  <ProjectSection
    heading="Recent"
    items={sections.recent}
    key={entry => entry.project}
    onclear={onclearrecent}
    row={recentGroup}
  />

  {#if emptyMessage}
    <p class="empty">{emptyMessage}</p>
  {/if}
</div>

<style>
  /* The list scrolls on its own under the fixed head and search. Its rows
     shrink to the rail's width rather than overflow it, and the scrollbar is
     the skin's: thin, a primary-tinted thumb over the rail's own surface. */
  .groups {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 8px;
    overflow: hidden auto;
    min-block-size: 0;
    min-inline-size: 0;
    padding-block: 0 14px;
    padding-inline: 6px 4px;
    scrollbar-color: color-mix(in sRGB, var(--primary) 45%, transparent) var(--surface-1);
    scrollbar-width: thin;
    scrollbar-gutter: stable;
  }

  .empty {
    margin: 0;
    padding-block: 4px;
    padding-inline: 8px;
    color: var(--on-surface-variant);
    font-size: 12px;
  }
</style>
