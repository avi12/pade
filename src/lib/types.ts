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

/** The Change Feed's git-free preview inputs for one path: the session's
 *  first-touch baseline snapshot (`before`) and the file's current on-disk
 *  content (`after`, empty when it is now deleted). The whole payload is nullable
 *  at the boundary — `null` = nothing to preview (binary, over the size cap, or a
 *  path with no captured baseline) — so the card shows "No preview available". */
export const FeedDiff = z.object({
  before: z.string(),
  after: z.string()
});
export type FeedDiff = z.infer<typeof FeedDiff>;

/** The manifest family that confirms a directory as a package (workspace
 *  member discovery — a folder is a package IFF it holds its own manifest). */
export const Ecosystem = z.enum(["javascript", "rust", "go", "python"]);
export type Ecosystem = z.infer<typeof Ecosystem>;

/** One manifest-confirmed workspace member: its repo-relative `/`-joined
 *  directory (`""` = the root), the name its manifest declares (`null` when it
 *  declares none), and the manifest family that confirmed it (`null` only for
 *  a root without any manifest — kept so every file still has a bucket). */
export const WorkspaceMember = z.object({
  path: z.string(),
  name: z.string().nullable(),
  ecosystem: Ecosystem.nullable()
});
export type WorkspaceMember = z.infer<typeof WorkspaceMember>;

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
  when: z.string(),
  /** Lines added across the commit (defaulted for back-compat with old callers). */
  additions: z.number().default(0),
  /** Lines deleted across the commit. */
  deletions: z.number().default(0),
  /** Files the commit touched. */
  files: z.number().default(0)
});
export type Commit = z.infer<typeof Commit>;

/** One file changed by a commit, with its per-file line counts. */
export const CommitFileEntry = z.object({
  path: z.string(),
  kind: VcsKind,
  additions: z.number(),
  deletions: z.number(),
  /** True when git reports the file as binary (line counts are meaningless). */
  binary: z.boolean().default(false)
});
export type CommitFileEntry = z.infer<typeof CommitFileEntry>;

/** A single commit's full detail: message body, branch, and per-file stats.
 *  Reuses `Commit`'s field names for the shared header fields. */
export const CommitDetail = z.object({
  id: z.string(),
  short: z.string(),
  summary: z.string(),
  /** The commit message body (everything after the subject); empty if none. */
  body: z.string(),
  author: z.string(),
  when: z.string(),
  /** Current HEAD branch name (empty on a detached HEAD). */
  branch: z.string(),
  files: z.array(CommitFileEntry),
  additions: z.number(),
  deletions: z.number()
});
export type CommitDetail = z.infer<typeof CommitDetail>;

/** A commit ranked as a candidate for "restore a version" — a Commit plus a
 *  0..≈1.5 fuzzy `score` (token overlap with the query, boosted by time hints). */
export const RestoreCandidate = Commit.extend({ score: z.number() });
export type RestoreCandidate = z.infer<typeof RestoreCandidate>;

/** How a "Sync all" fast-forward pull resolved. `updated` fast-forwarded to new
 *  upstream commits, `alreadyUpToDate` had nothing to fetch, and `refusedDirty`
 *  bailed because the working tree has uncommitted changes to tracked files. A
 *  branch that diverged from upstream (no fast-forward) throws instead. */
export const PullStatus = z.enum(["updated", "alreadyUpToDate", "refusedDirty"]);
export type PullStatus = z.infer<typeof PullStatus>;

/** The result of `vcs.pull`: the outcome plus a short human line for the toast. */
export const PullOutcome = z.object({
  status: PullStatus,
  message: z.string()
});
export type PullOutcome = z.infer<typeof PullOutcome>;

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

/** The always-present shell fallback's agent id — the one backend-provided agent
 *  that isn't a real coding agent, so it's excluded from auto-launch/onboarding. */
export const SHELL_AGENT_ID = "shell";

/** Where the picker hands a chosen or freshly-created project back to the app:
 *  the project path, plus — for a create — an optional first prompt to seed and
 *  the agent id to launch it with (both absent when reopening an existing one). */
export type OpenTarget = {
  path: string;
  initialPrompt?: string;
  agent?: string;
};

/** One runnable task parsed from a project manifest. */
export const Task = z.object({
  name: z.string(),
  command: z.string()
});
export type Task = z.infer<typeof Task>;

/** The tasks extracted from one manifest (package.json, Cargo.toml, …). */
export const TaskGroup = z.object({
  manifest: z.string(),
  dir: z.string(),
  kind: z.enum(["npm", "cargo", "make", "python"]),
  tasks: z.array(Task)
});
export type TaskGroup = z.infer<typeof TaskGroup>;

export const Ide = z.object({
  id: z.string(),
  label: z.string(),
  command: z.string(),
  /** A console editor (Neovim, Vim, Helix) PADE opens in a terminal tab rather
   *  than launching as a detached window. */
  terminal: z.boolean().default(false),
  /** Leads `ide_suggest`'s ranking because of an explicit per-project pick —
   *  badged as the user's choice, not the auto-detected best fit. */
  chosen: z.boolean().default(false)
});
export type Ide = z.infer<typeof Ide>;

/** One project kind the editor-rules engine renders a row for. The kind
 *  registry lives in Rust (`ide_kinds`, render/priority order); the frontend
 *  derives its rows from it rather than hardcoding the list. */
export const EditorKind = z.object({
  kind: z.string(),
  label: z.string(),
  signals: z.array(z.string())
});
export type EditorKind = z.infer<typeof EditorKind>;

/** An editor the user located by executable path (merged into detection). */
export const AddedEditor = z.object({
  id: z.string(),
  label: z.string(),
  path: z.string()
});
export type AddedEditor = z.infer<typeof AddedEditor>;

/** An AI design/UI-generation tool, ranked for the active agent. */
export const DesignTool = z.object({
  id: z.string(),
  label: z.string(),
  vendor: z.string(),
  url: z.string(),
  recommended: z.boolean()
});
export type DesignTool = z.infer<typeof DesignTool>;

/** Remaining usage / quota for the active agent. Every field is optional so an
 *  adapter can surface only what it has locally; the whole payload is nullable
 *  (`null` = no reliable local signal, UI shows "usage —"). */
export const Usage = z.object({
  /** Percent of quota consumed, 0..100, when a precise number is known. */
  usedPct: z.number().nullish(),
  /** Short meter label (e.g. a plan/tier name). */
  label: z.string(),
  /** ISO-8601 reset time for the quota window, when known. */
  resetsAt: z.string().nullish(),
  /** Where the figure came from — so the UI stays honest about it. */
  source: z.string()
});
export type Usage = z.infer<typeof Usage>;

/** The semantic kind of a rate-limit window. The account endpoint returns a
 *  handful of named windows (the 5-hour session, the 7-day weekly all-models cap)
 *  plus any per-model caps; the backend classifies each. A named window it doesn't
 *  recognize arrives as `opaque` — surfaced honestly, never dropped. */
export const UsageWindowKind = z.enum(["session", "weekly", "model", "opaque"]);
export type UsageWindowKind = z.infer<typeof UsageWindowKind>;

/** One live rate-limit window mirrored from the same claude.ai OAuth usage
 *  endpoint Claude Code's `/usage` reads: a stable `key`, its `kind`, a human
 *  `label`, the 0..100 `utilization`, and an ISO-8601 reset time when known. */
export const UsageWindow = z.object({
  /** Stable identity from the API (`five_hour`, `seven_day`, or a model name). */
  key: z.string(),
  kind: UsageWindowKind,
  /** Human name — a model's display name, or a humanized form of the key. */
  label: z.string(),
  /** Percent of the window consumed (0..100). */
  utilization: z.number(),
  /** ISO-8601 reset time for the window, when known. */
  resetsAt: z.string().nullish()
});
export type UsageWindow = z.infer<typeof UsageWindow>;

/** Live account usage — every rate-limit window the endpoint returns (session,
 *  weekly, and any per-model or other windows), plus the plan label. `null` when
 *  offline / the local token is missing or expired. */
export const AccountUsage = z.object({
  windows: z.array(UsageWindow).default([]),
  plan: z.string(),
  source: z.string()
});
export type AccountUsage = z.infer<typeof AccountUsage>;

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

/** One open PADE window and the project it has, for the switcher's Open-windows
 *  list. `label` is the stable window id used to focus it. */
export const WindowInfo = z.object({
  label: z.string(),
  path: z.string(),
  isCurrent: z.boolean()
});
export type WindowInfo = z.infer<typeof WindowInfo>;

export const ThemeMode = z.enum(["system", "light", "dark"]);
export type ThemeMode = z.infer<typeof ThemeMode>;

/** The concrete appearance `ThemeMode` resolves to ("system" → the OS answer).
 *  Applied to the document root, the terminal palette, and — over the wire —
 *  each agent's own theme config (`theme_sync`) and spawn env. */
export const Scheme = z.enum(["light", "dark"]);
export type Scheme = z.infer<typeof Scheme>;

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
  autoNameTemp: z.boolean().nullish(),
  /** Per project-kind editor rules — kind (e.g. "web", "rust") → IDE id. */
  ideRules: z.record(z.string(), z.string()).nullish(),
  /** IDE id to open when no rule matches the project kind. */
  ideFallback: z.string().nullish(),
  /** Explicit per-project editor picks — canonical project path → IDE id. A
   *  pick from the workspace's editor menu; outranks every rule for that
   *  project (`ide_suggest` puts it first). */
  ideProjectChoices: z.record(z.string(), z.string()).nullish(),
  /** Editors the user located by executable path (merged into detection). */
  addedEditors: z.array(AddedEditor).nullish(),
  /** Auto-hand-off to a fresh agent near the context limit. Opt-out: on unless
   *  explicitly set to false. */
  autoHandoff: z.boolean().nullish(),
  /** Auto-resume a usage-limited session when its window resets — "continue"
   *  in place, or hand off when the context is nearly full. Opt-out: on unless
   *  explicitly set to false. */
  autoResume: z.boolean().nullish(),
  /** UI + terminal zoom factor (0.85–1.30, step 0.05). Absent = default 1.0. */
  uiScale: z.number().min(0.85).max(1.3).nullish(),
  /** Broadcast a Discord "Playing PADE" rich-presence status. Opt-in: off unless explicitly true. */
  discordPresence: z.boolean().nullish(),
  /** When presence is on, show the current project's name in the status. Default on. */
  discordShowProject: z.boolean().nullish()
});
export type Prefs = z.infer<typeof Prefs>;

export const Settings = z.object({
  roots: z.array(z.string()),
  defaultAgent: z.string().nullable(),
  projectAgents: z.record(z.string(), z.string()),
  recentProjects: z.array(z.string()).default([]),
  /** Projects pinned in the switcher — kept above the recents and independent of
   *  recent-history churn. */
  pinnedProjects: z.array(z.string()).default([]),
  ownedWorkspaces: z.array(z.string()).default([]),
  /** Friendly display names for workspaces, keyed by absolute path. */
  labels: z.record(z.string(), z.string()).default({}),
  prefs: Prefs.default({})
});
export type Settings = z.infer<typeof Settings>;

/** How `workspace_add_root` resolved. `added` carries the refreshed settings;
 *  `missing` (the path doesn't exist and creation wasn't requested) and
 *  `notADirectory` (the path names a file) are the two "didn't add" outcomes the
 *  picker acts on — prompting to create, or showing an inline error. */
export const AddRootStatus = z.enum(["added", "missing", "notADirectory"]);
export type AddRootStatus = z.infer<typeof AddRootStatus>;

export const AddRootOutcome = z.discriminatedUnion("status", [
  z.object({
    status: z.literal(AddRootStatus.enum.added),
    settings: Settings
  }),
  z.object({ status: z.literal(AddRootStatus.enum.missing) }),
  z.object({ status: z.literal(AddRootStatus.enum.notADirectory) })
]);
export type AddRootOutcome = z.infer<typeof AddRootOutcome>;

/** A live probe of the path being typed into the add-root field. Instead of
 *  regex-guessing whether the text is a valid path, the backend just checks the
 *  filesystem: whether the path itself is a directory (`isDir`) or a file
 *  (`isFile`), and whether its parent exists (`parentExists`) — so a not-yet-made
 *  folder in a real place reads as "will create" and a stray string reads as
 *  invalid. `suggestions` are the child directories that complete what was typed,
 *  for the autocomplete. */
export const PathProbe = z.object({
  isDir: z.boolean(),
  isFile: z.boolean(),
  parentExists: z.boolean(),
  suggestions: z.array(z.string())
});
export type PathProbe = z.infer<typeof PathProbe>;

/** The nothing-typed-yet probe: no disk knowledge, no completions. The shared
 *  reset every path field starts from, so an empty box never reads as a folder. */
export const emptyPathProbe: PathProbe = {
  isDir: false,
  isFile: false,
  parentExists: false,
  suggestions: []
};

/** A probe tagged with the exact text it describes — the field only trusts the
 *  disk flags once `path` matches the current (trimmed) input, since the probe is
 *  async + debounced. The shape `PathCombobox` binds back to its host. */
export type TaggedPathProbe = {
  path: string;
  result: PathProbe;
};

/** PTY stream event payloads. `seq` is the chunk's position in the session's
 *  stream, which is what lets a terminal attaching mid-flight tell the chunks it
 *  caught live apart from the ones already inside the history it replayed. */
export const PtyChunk = z.object({
  id: z.string(),
  data: z.string(),
  seq: z.number()
});
export type PtyChunk = z.infer<typeof PtyChunk>;

/** Physical (device-pixel) cursor position carried by the window's native
 *  drag-and-drop events — divide by devicePixelRatio before comparing with
 *  CSS-pixel rects. */
export const DragPosition = z.object({
  x: z.number(),
  y: z.number()
});
export type DragPosition = z.infer<typeof DragPosition>;

/** An OS file/folder drop onto the window: absolute paths + drop position. */
export const DragDropPayload = z.object({
  paths: z.array(z.string()),
  position: DragPosition
});
export type DragDropPayload = z.infer<typeof DragDropPayload>;

/** A drag travelling over the window (fires continuously with the position). */
export const DragOverPayload = z.object({
  position: DragPosition
});
export type DragOverPayload = z.infer<typeof DragOverPayload>;

/** A session's replayable output, the sequence number of the last chunk in it, and
 *  whether the program is currently painting the alternate screen — in which case
 *  the history is a stream of framebuffer edits rather than a document, and a
 *  trimmed one cannot be replayed faithfully. */
export const PtyHistory = z.object({
  data: z.string(),
  seq: z.number(),
  alternate: z.boolean()
});
export type PtyHistory = z.infer<typeof PtyHistory>;

export const PtyExit = z.object({ id: z.string() });
export type PtyExit = z.infer<typeof PtyExit>;

/** One live PTY session the backend still hosts (`pty_list`) — the re-attach
 *  roster after a WebView reload. A listed session is alive by construction.
 *  Deliberately no idleness field: the sessions store's status signal is the
 *  one authority on idle (SSOT). */
export const PtySession = z.object({
  id: z.string(),
  /** The agent command it was spawned with (`null` = the default-shell fallback). */
  command: z.string().nullable(),
  /** The directory it was spawned into — its workspace mapping. */
  cwd: z.string().nullable()
});
export type PtySession = z.infer<typeof PtySession>;

/** A tracked task-runner (dock) and its stream event payloads. */
export const RunnerInfo = z.object({
  id: z.string(),
  command: z.string(),
  cwd: z.string().nullable(),
  startedAt: z.number()
});
export type RunnerInfo = z.infer<typeof RunnerInfo>;

export const RunnerStream = z.enum(["stdout", "stderr"]);
export type RunnerStream = z.infer<typeof RunnerStream>;

export const RunnerData = z.object({
  id: z.string(),
  data: z.string(),
  stream: RunnerStream
});
export type RunnerData = z.infer<typeof RunnerData>;

export const RunnerExit = z.object({
  id: z.string(),
  code: z.number().nullable()
});
export type RunnerExit = z.infer<typeof RunnerExit>;

/** A running terminal session bound to one agent (frontend-only, not IPC). */
export interface AgentSession {
  id: string;
  agent: Agent;
  initialPrompt?: string;
  /** Working dir override — a per-branch git worktree, when set. */
  cwd?: string;
  /** Branch this session works, when spawned on a worktree. */
  branch?: string;
  /** Extra args for the command — the project path when this session runs a
   *  terminal editor (Neovim/Vim/Helix) instead of an agent. */
  args?: string[];
  /** A stable conversation id ADE pins the session to (`claude --session-id`),
   *  so a restart can resume THIS conversation, not merely the most recent. It
   *  outlives the session `id`, which is re-keyed to remount the terminal on a
   *  restart. Only meaningful for an agent whose CLI supports it. */
  conversationId?: string;
}

/** A project's declared MCP servers changed (a server name added or removed —
 *  not a value-only edit). The affected agents' sessions restart to pick it up. */
export const McpChange = z.object({
  path: z.string(),
  agents: z.array(z.string()),
  added: z.array(z.string()),
  removed: z.array(z.string())
});
export type McpChange = z.infer<typeof McpChange>;
