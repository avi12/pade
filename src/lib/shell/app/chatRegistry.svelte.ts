import {
  loadChatRegistry,
  persistedChatsFromSessions,
  registryWithLiveChats,
  registryWithoutChat,
  saveChatRegistry
} from "@/lib/session/chats/chat-registry";
import type { StoredChatRegistry } from "@/lib/session/chats/chat-registry";
import type { WindowSessions } from "@/lib/shell/windowSessions.svelte";
import { sessionLabel } from "@/lib/stores/session/sessionLabels.svelte";
import { untrack } from "svelte";

/** Every project's persisted chats (lib/session/chats/chat-registry) - the left
 *  rail's memory, loaded once and kept in sync with the live sessions. */
export function createChatRegistry() {
  let registry = $state<StoredChatRegistry>(loadChatRegistry());

  function forgetChat({ project, chatId }: {
    project: string;
    chatId: string;
  }) {
    const next = registryWithoutChat({
      registry,
      project,
      chatId
    });
    registry = next;
    saveChatRegistry(next);
  }

  // Keep the persisted chat registry in step with this window's live
  // sessions, each under its own project, labels included, so a relaunch can
  // list and resume them. A chat closed for good leaves through `forgetChat`,
  // never by being absent here.
  function followSessions({ windowSessions, activeProject }: {
    windowSessions: WindowSessions;
    activeProject: () => string;
  }) {
    $effect(() => {
      if (windowSessions.sessions.length === 0) {
        return;
      }

      const chats = persistedChatsFromSessions({
        sessions: windowSessions.sessions,
        labels: Object.fromEntries(
          windowSessions.sessions.map(session => [session.id, sessionLabel(session.id)] as const)
        ),
        now: Date.now()
      });
      const current = untrack(() => registry);
      const next = registryWithLiveChats({
        registry: current,
        chats,
        activeChatId: windowSessions.activeId,
        activeProject: activeProject()
      });
      if (next === current) {
        return;
      }

      registry = next;
      saveChatRegistry(next);
    });
  }

  return {
    get registry() {
      return registry;
    },
    forgetChat,
    followSessions,
    adoptStoredCopy() {
      registry = loadChatRegistry();
    }
  };
}

export type ChatRegistry = ReturnType<typeof createChatRegistry>;
