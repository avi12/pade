export type ChangeKind = "created" | "modified" | "deleted";

/** Lifecycle of an agent or sub-agent session, shared by the terminal and
 *  (later) the agent tree. `ready` = idle at a prompt, done with its task and
 *  waiting for you; `exited` = the process ended. */
export type SessionStatus = "starting" | "working" | "ready" | "exited";

export type VcsKind = "created" | "modified" | "deleted" | "renamed" | "untracked";

/** One changed path in the working tree. */
export interface StatusEntry {
  path: string;
  kind: VcsKind;
  staged: boolean;
}

/** A recent commit in the Log view. */
export interface Commit {
  id: string;
  short: string;
  summary: string;
  author: string;
  when: string;
}

/** An agent config file the ADE surfaces (read-only for MVP). */
export interface ConfigFile {
  name: string;
  rel: string;
  kind: "instructions" | "mcp" | "settings";
  exists: boolean;
}

/** One entry in the Change Feed — a file the agent (or you) touched. */
export interface ChangeEvent {
  id: string;
  path: string;
  kind: ChangeKind;
  added: number;
  removed: number;
  /** Plain-language, one-line intent. MVP: heuristic; later: agent-authored. */
  summary: string;
  ts: number;
}
