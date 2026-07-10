// Single source of truth for every IPC payload shape. Schemas are zod so the
// data crossing the Rust→TS boundary is validated at runtime; the TS types are
// inferred from them (never hand-written alongside).

import { z } from "zod";

export const ChangeKind = z.enum(["created", "modified", "deleted"]);
export type ChangeKind = z.infer<typeof ChangeKind>;

export const ChangeEvent = z.object({
  id: z.string(),
  path: z.string(),
  kind: ChangeKind,
  added: z.number(),
  removed: z.number(),
  summary: z.string(),
  ts: z.number()
});
export type ChangeEvent = z.infer<typeof ChangeEvent>;

/** Lifecycle of an agent or sub-agent session, shared by the terminal and the
 *  agent tree. `ready` = idle at a prompt, done and waiting for you. */
export const SessionStatus = z.enum(["starting", "working", "ready", "exited"]);
export type SessionStatus = z.infer<typeof SessionStatus>;

export const VcsKind = z.enum(["created", "modified", "deleted", "renamed", "untracked"]);
export type VcsKind = z.infer<typeof VcsKind>;

export const StatusEntry = z.object({
  path: z.string(),
  kind: VcsKind,
  staged: z.boolean()
});
export type StatusEntry = z.infer<typeof StatusEntry>;

export const Commit = z.object({
  id: z.string(),
  short: z.string(),
  summary: z.string(),
  author: z.string(),
  when: z.string()
});
export type Commit = z.infer<typeof Commit>;

export const ConfigFile = z.object({
  name: z.string(),
  rel: z.string(),
  kind: z.enum(["instructions", "mcp", "settings"]),
  exists: z.boolean()
});
export type ConfigFile = z.infer<typeof ConfigFile>;

export const Agent = z.object({
  id: z.string(),
  label: z.string(),
  command: z.string()
});
export type Agent = z.infer<typeof Agent>;

export const Ide = z.object({
  id: z.string(),
  label: z.string(),
  command: z.string()
});
export type Ide = z.infer<typeof Ide>;

export const LaunchContext = z.object({
  hasProject: z.boolean(),
  cwd: z.string()
});
export type LaunchContext = z.infer<typeof LaunchContext>;

export const ProjectEntry = z.object({
  name: z.string(),
  path: z.string(),
  isGit: z.boolean()
});
export type ProjectEntry = z.infer<typeof ProjectEntry>;

export const ThemeMode = z.enum(["system", "light", "dark"]);
export type ThemeMode = z.infer<typeof ThemeMode>;

export const DiffStyle = z.enum(["unified", "split"]);
export type DiffStyle = z.infer<typeof DiffStyle>;

/** Appearance & editor preferences. All optional — `null`/absent = use default. */
export const StartMode = z.enum(["temp", "picker"]);
export type StartMode = z.infer<typeof StartMode>;

export const Prefs = z.object({
  uiFont: z.string().nullish(),
  monoFont: z.string().nullish(),
  themeMode: ThemeMode.nullish(),
  diffStyle: DiffStyle.nullish(),
  startMode: StartMode.nullish(),
  /** Auto-name temp workspaces once the agent has done real work (default on). */
  autoNameTemp: z.boolean().nullish()
});
export type Prefs = z.infer<typeof Prefs>;

export const Settings = z.object({
  roots: z.array(z.string()),
  defaultAgent: z.string().nullable(),
  projectAgents: z.record(z.string(), z.string()),
  recentProjects: z.array(z.string()).default([]),
  ownedWorkspaces: z.array(z.string()).default([]),
  /** Friendly display names for workspaces, keyed by absolute path. */
  labels: z.record(z.string(), z.string()).default({}),
  prefs: Prefs.default({})
});
export type Settings = z.infer<typeof Settings>;

/** PTY stream event payloads. */
export const PtyChunk = z.object({
  id: z.string(),
  data: z.string()
});
export type PtyChunk = z.infer<typeof PtyChunk>;

export const PtyExit = z.object({ id: z.string() });
export type PtyExit = z.infer<typeof PtyExit>;

/** A running terminal session bound to one agent (frontend-only, not IPC). */
export interface AgentSession {
  id: string;
  agent: Agent;
  initialPrompt?: string;
  /** Working dir override — a per-branch git worktree, when set. */
  cwd?: string;
  /** Branch this session works, when spawned on a worktree. */
  branch?: string;
}
