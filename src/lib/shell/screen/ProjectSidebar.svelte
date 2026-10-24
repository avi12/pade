<script lang="ts">
  import Icon from "@/lib/Icon.svelte";
  import { sidebarProjects } from "@/lib/session/chat-registry";
  import type { StoredChatRegistry } from "@/lib/session/chat-registry";
  import { settings } from "@/lib/settings/settings.svelte";
  import ProjectChatRow from "@/lib/shell/screen/ProjectChatRow.svelte";
  import { recordedConversations, refreshRecordedConversations } from "@/lib/stores/session/recordedConversations.svelte";
  import { sessionLabel } from "@/lib/stores/session/sessionLabels.svelte";
  import type { AgentSession, RecordedConversation } from "@/lib/types";
  import { displayName } from "@/lib/util/paths";

  // The left rail: every project, with its chats grouped under it. The
  // current project's chats are its live sessions; the others are the
  // persisted ones, reopened on their own conversations when picked.
  const { registry, project, sessions, activeId, onopenproject, onselectchat, onresumeconversation, ondeletechat, onnewchat }: {
    registry: StoredChatRegistry;
    /** The project this window has open - its chats are the live sessions. */
    project: string;
    sessions: AgentSession[];
    activeId: string | null;
    onopenproject: (project: string) => void;
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
    onnewchat: (project: string) => void;
  } = $props();

  // Every project the user has touched, current first, then pinned, then
  // recent, then any other project that still holds chats - deduplicated by
  // the grouping itself.
  const projectOrder = $derived.by(() => {
    const projectsWithChats = [...new Set(registry.chats.map(chat => chat.project))];
    return [project, ...settings.pinnedProjects, ...settings.recentProjects, ...projectsWithChats];
  });

  const liveLabels = $derived(
    Object.fromEntries(sessions.map(session => [session.id, sessionLabel(session.id)] as const))
  );

  const groups = $derived(
    sidebarProjects({
      registry,
      projectOrder,
      currentProject: project,
      liveSessions: sessions,
      liveLabels,
      recordedConversations
    })
  );

  // Re-read the agents' recorded conversations when the listed projects
  // change, or a chat opens or closes (a closed chat lives on as a recorded
  // conversation). One joined key of both, so a registry save that leaves
  // them as they were doesn't re-read anything.
  const refreshKey = $derived([sessions.length, ...projectOrder].join("\n"));
  $effect(() => {
    const [, ...projects] = refreshKey.split("\n");
    refreshRecordedConversations(projects);
  });
</script>

<nav class="sidebar" aria-label="Projects and chats">
  <div class="sidebar-head">
    <h2>Projects</h2>
  </div>

  <div class="groups">
    {#each groups as group (group.project)}
      <section class="group" class:current={group.isCurrent}>
        <div class="project-row">
          <button
            class="project-main"
            aria-current={group.isCurrent}
            onclick={() => {
              if (!group.isCurrent) {
                onopenproject(group.project);
              }
            }}
            title={group.project}
            type="button"
          >
            <span class="project-icon"><Icon name="folder" size={15} /></span>
            <span class="project-name">{displayName({
              path: group.project,
              labels: settings.labels
            })}</span>
            <span class="chat-count">{group.chats.length}</span>
          </button>
          <button
            class="new-chat"
            aria-label="New chat in {displayName({
              path: group.project,
              labels: settings.labels
            })}"
            data-tooltip="New chat"
            onclick={() => onnewchat(group.project)}
            type="button"
          ><Icon name="plus" size={14} /></button>
        </div>

        <ul class="chats">
          {#each group.chats as chat (chat.id)}
            <li>
              <ProjectChatRow
                {chat}
                isActive={group.isCurrent && chat.id === activeId}
                ondelete={chat.resume === undefined
                  ? () => ondeletechat({
                    project: group.project,
                    chatId: chat.id
                  })
                  : undefined}
                onnew={() => onnewchat(group.project)}
                onselect={() => {
                  if (chat.resume !== undefined) {
                    onresumeconversation({
                      project: group.project,
                      conversation: chat.resume
                    });
                    return;
                  }

                  onselectchat({
                    project: group.project,
                    chatId: chat.id
                  });
                }}
              />
            </li>
          {:else}
            <li class="no-chats">No chats yet</li>
          {/each}
        </ul>
      </section>
    {:else}
      <p class="no-projects">No projects yet - open one to start a chat</p>
    {/each}
  </div>
</nav>

<style>
  /* The left rail - a fixed-width column that never squeezes the terminal:
     projects stack, their chats indent under them, and the whole list
     scrolls on its own. */
  .sidebar {
    display: flex;
    flex: none;
    flex-direction: column;
    inline-size: 248px;
    min-block-size: 0;
    border-inline-end: 1px solid var(--outline);
    background: var(--surface-1);
  }

  .sidebar-head {
    flex: none;
    padding-block: 12px 6px;
    padding-inline: 14px;

    h2 {
      margin: 0;
      color: var(--on-surface-variant);
      font-weight: 700;
      font-size: 11px;
      letter-spacing: 0.07em;
      text-transform: uppercase;
    }
  }

  .groups {
    display: flex;
    flex-direction: column;
    gap: 14px;
    overflow-y: auto;
    min-block-size: 0;
    padding-block: 4px 14px;
    padding-inline: 8px;
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  /* Project header - the switch target, with the new-chat button riding it. */
  .project-row {
    display: flex;
    gap: 2px;
    align-items: center;
  }

  .project-main {
    display: flex;
    flex: 1;
    gap: 8px;
    align-items: center;
    min-inline-size: 0;
    padding-block: 7px;
    padding-inline: 8px;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface);
    font: inherit;
    font-weight: 600;
    font-size: 13px;
    text-align: start;
    cursor: pointer;
    transition: background 120ms var(--ease);

    &:hover {
      background: var(--surface-2);
    }

    .project-icon {
      display: inline-flex;
      flex: none;
      color: var(--on-surface-variant);
    }

    .project-name {
      flex: 1;
      overflow: hidden;
      min-inline-size: 0;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .chat-count {
      flex: none;
      color: var(--on-surface-variant);
      font-size: 11px;
    }
  }

  .group.current .project-main {
    color: var(--primary);
  }

  .new-chat {
    display: inline-flex;
    flex: none;
    justify-content: center;
    align-items: center;
    block-size: 26px;
    inline-size: 26px;
    padding: 0;
    border: none;
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--on-surface-variant);
    cursor: pointer;
    transition: background 120ms var(--ease), color 120ms var(--ease);

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
    margin: 0;
    padding: 0;
    padding-inline-start: 18px;
    list-style: none;
  }

  .no-chats,
  .no-projects {
    margin: 0;
    padding-block: 4px;
    padding-inline: 8px;
    color: var(--on-surface-variant);
    font-size: 12px;
  }

  .no-chats {
    padding-inline-start: 26px;
  }

  @media (width <= 720px) {
    .sidebar {
      inline-size: 184px;
    }
  }
</style>
