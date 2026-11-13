// The one owner of a window's session state: which sessions exist, which are
// shown side by side, which is active, and the bookkeeping kept per session
// (launch time, hand-close claims, collapsing split panes). The lifecycle steps
// in `window-session-lifecycle` and the `windowSessions` facade only read and
// write it through here.

import { SvelteMap, SvelteSet } from "svelte/reactivity";

import { frontProject, sessionsOfProject } from "@/lib/session/chats/session-projects";
import { forgetSession } from "@/lib/session/lifecycle/session-scope";
import { layoutWithoutSession, layoutWithoutSessions, wrappedIndex } from "@/lib/session/panes/pane-layout";
import type { PaneLayout } from "@/lib/session/panes/pane-layout";
import type { AgentSession } from "@/lib/types/agent";

export class WindowLayout {
  sessions = $state<AgentSession[]>([]);
  // Which sessions are shown side by side in the terminal area. One id = the
  // classic single pane; more = a split view. Always a subset of `sessions`.
  paneIds = $state<string[]>([]);
  activeId = $state<string | null>(null);
  activeSession = $derived(this.sessions.find(session => session.id === this.activeId));
  // The project the window points at while no session is in front: the one
  // it entered, or the one its last session left.
  fallbackProject = $state("");
  // The window's one project context: the active session's project. Every
  // project-scoped surface (top bar, Change Feed, repository, tasks, address)
  // reads this, so switching sessions across projects switches them all.
  activeProject = $derived(
    frontProject({
      activeSession: this.activeSession,
      fallbackProject: this.fallbackProject
    })
  );
  // The front project's sessions - what the tab strip shows. Every session
  // stays mounted (its terminal live) whichever project is in front.
  projectSessions = $derived(
    sessionsOfProject({
      sessions: this.sessions,
      project: this.activeProject
    })
  );

  // When each session launched, so a self-exit right after reads as a failed
  // start (lib/session/lifecycle/session-exit) and is not respawned into a loop.
  sessionLaunchedAt = new SvelteMap<string, number>();
  // A hand-close also kills the PTY, firing the exit event; the exit handler skips
  // these so the two teardown paths don't race.
  closingByHand = new SvelteSet<string>();

  // A pane that was one of several in a live split when its session detached
  // collapses its width so the survivors glide across to fill (out:collapsePane).
  // Every other removal - a sole-pane tab close above all - swaps instantly: with
  // nothing beside it, a collapsing pane would only crush the surviving terminal
  // to near-zero width and let it reflow back out, a needless squeeze. So just
  // genuine split members are recorded here (and dropped again on `outroend`);
  // the rest get a zero-duration outro.
  collapsingSplitPanes = new SvelteSet<string>();

  get layout() {
    return {
      sessions: this.sessions,
      paneIds: this.paneIds,
      activeId: this.activeId
    };
  }

  assign({ sessions, paneIds, activeId }: PaneLayout) {
    this.sessions = sessions;
    this.paneIds = paneIds;
    this.activeId = activeId;
  }

  isOpen(id: string) {
    return this.sessions.some(session => session.id === id);
  }

  // A tab click shows that session as the sole pane (classic single view).
  // Collapsing a split is deliberately instant (as the design is): a View
  // Transition here would scale the surviving pane's snapshot as it grows to
  // fill, ghosting the old half-width terminal text over the reflowed full-width
  // grid - the platform morph can't track a terminal that repaints on resize.
  select(id: string) {
    this.activeId = id;
    this.paneIds = [id];
  }

  // Ctrl+Tab / Alt+Arrow - cycle the active tab among the front project's
  // tabs (the ones the strip shows), wrapping at the ends.
  step(delta: number) {
    if (this.projectSessions.length === 0) {
      return;
    }

    const iNext = wrappedIndex({
      index: this.projectSessions.findIndex(session => session.id === this.activeId),
      delta,
      count: this.projectSessions.length
    });
    this.select(this.projectSessions[iNext].id);
  }

  // The window keeps pointing at the project its last session leaves.
  private keepProjectOfLastSession(id: string) {
    const isLastSession = this.sessions.length === 1 && this.sessions[0].id === id;
    if (isLastSession) {
      this.fallbackProject = this.sessions[0].project;
    }
  }

  // The caller kills the PTY and decides the empty-workspace policy.
  detach(id: string) {
    const isPaneIdsLengthAbove1And = this.paneIds.length > 1 && this.paneIds.includes(id);
    if (isPaneIdsLengthAbove1And) {
      this.collapsingSplitPanes.add(id);
    }

    this.keepProjectOfLastSession(id);
    this.assign(
      layoutWithoutSession({
        layout: this.layout,
        id
      })
    );
    this.forget(id);
  }

  // A session left this window - ended, re-keyed or moved away: everything
  // kept for it goes, here and in every per-session store (session-scope).
  forget(id: string) {
    this.sessionLaunchedAt.delete(id);
    this.closingByHand.delete(id);
    forgetSession(id);
  }

  removeClosed(ids: ReadonlySet<string>) {
    this.assign(
      layoutWithoutSessions({
        layout: this.layout,
        ids
      })
    );

    for (const id of ids) {
      this.forget(id);
    }
  }

  // Hand a session the args that resume its own conversation, so the restart
  // that follows reopens it rather than starting a blank one.
  applyResumeArgs({ id, args }: { id: string; args: string[] }) {
    if (args.length === 0) {
      return;
    }

    this.sessions = this.sessions.map(session =>
      session.id === id
        ? {
            ...session,
            args
          }
        : session
    );
  }

  /** Re-attach the sessions a reload orphaned, stamped as launched-now so one
   *  that dies right after reads as a failed start (no respawn loop). */
  restore(layout: PaneLayout) {
    this.assign(layout);
    for (const restored of layout.sessions) {
      this.sessionLaunchedAt.set(restored.id, Date.now());
    }
  }

  markClosing(ids: Iterable<string>) {
    for (const id of ids) {
      this.closingByHand.add(id);
    }
  }

  release(ids: readonly string[]) {
    for (const id of ids) {
      this.detach(id);
    }
  }
}

/** The state half of the public `windowSessions` object, read and written
 *  straight through to the layout. */
export function layoutAccessors(layout: WindowLayout) {
  return {
    get sessions() {
      return layout.sessions;
    },
    set sessions(next) {
      layout.sessions = next;
    },
    get paneIds() {
      return layout.paneIds;
    },
    set paneIds(next) {
      layout.paneIds = next;
    },
    get activeId() {
      return layout.activeId;
    },
    set activeId(next) {
      layout.activeId = next;
    },
    get activeSession() {
      return layout.activeSession;
    },
    /** The window's project context - the active session's project. */
    get activeProject() {
      return layout.activeProject;
    },
    /** The front project's sessions, as the tab strip shows them. */
    get projectSessions() {
      return layout.projectSessions;
    },
    collapsingSplitPanes: layout.collapsingSplitPanes
  };
}
