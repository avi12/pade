export type ChangeKind = "created" | "modified" | "deleted";

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
