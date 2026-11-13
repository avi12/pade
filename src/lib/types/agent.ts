import { z } from "zod";

/** Lifecycle of an agent or sub-agent session, shared by the terminal and the
 *  agent tree. `ready` = idle at a prompt, done and waiting for you. */
export const SessionStatus = z.enum(["starting", "working", "ready", "exited"]);

export type SessionStatus = z.infer<typeof SessionStatus>;

export const Agent = z.object({
  id: z.string(),
  label: z.string(),
  command: z.string(),
  /** This CLI applies the bidi algorithm to its own output, so what reaches the
   *  terminal is already in visual order and must be painted, not reordered again
   *  (agents.rs owns which ones - the terminal's right-to-left overlay reads it).
   *  Defaulted rather than required: a session persisted before the field existed
   *  still parses, and "no" is the safe answer for an unknown agent. */
  reordersBidi: z.boolean().default(false)
});

export type Agent = z.infer<typeof Agent>;

/** The always-present shell fallback's agent id - the one backend-provided agent
 *  that isn't a real coding agent, so it's excluded from auto-launch/onboarding. */
export const SHELL_AGENT_ID = "shell";

/** The launchable coding agents - every detected agent except the shell fallback.
 *  The one home for "which agents are real", so the app top bar, the picker's
 *  agents list, and quick-start can't drift on what counts as launchable. */
export function realAgents(agents: readonly Agent[]) {
  return agents.filter(agent => agent.id !== SHELL_AGENT_ID);
}

/** A model and/or reasoning effort picked for one session, or as a workspace's
 *  default for one agent. */
export const ModelChoice = z.object({
  model: z.string().optional(),
  effort: z.string().optional()
});

export type ModelChoice = z.infer<typeof ModelChoice>;

/** A running terminal session bound to one agent (frontend-only, not IPC). */
export interface AgentSession {
  id: string;
  agent: Agent;
  /** The project root this session belongs to. A window holds sessions from
   *  several projects at once; the window's project context (top bar, Change
   *  Feed, repository, tasks) follows the active session's project. */
  project: string;
  /** Working dir override - a per-branch git worktree, when set. */
  cwd?: string;
  /** Branch this session works, when spawned on a worktree. */
  branch?: string;
  /** Extra args for the command - the project path when this session runs a
   *  terminal editor (Neovim/Vim/Helix) instead of an agent. */
  args?: string[];
  /** A stable conversation id ADE pins the session to (`claude --session-id`),
   *  so a restart can resume THIS conversation, not merely the most recent. It
   *  outlives the session `id`, which is re-keyed to remount the terminal on a
   *  restart. Only meaningful for an agent whose CLI supports it. */
  conversationId?: string;
  /** A model and/or effort picked from the tab for THIS session alone. Every
   *  launch of the session carries it as launch flags, so a restart or a
   *  successor keeps running it; the agent's saved default is never touched. */
  modelChoice?: ModelChoice;
}

/** A registered project MCP config changed. `membershipChanged` distinguishes
 *  automatic handoff triggers from same-name edits that first reload normally. */
export const McpChange = z.object({
  path: z.string(),
  root: z.string(),
  agents: z.array(z.string()),
  added: z.array(z.string()),
  removed: z.array(z.string()),
  membershipChanged: z.boolean()
});

export type McpChange = z.infer<typeof McpChange>;

/** How full a session's context window is, as the agent's own session log
 *  accounts for it. */
export const SessionContext = z.object({
  usedTokens: z.number().nullable(),
  windowTokens: z.number().nullable()
});

export type SessionContext = z.infer<typeof SessionContext>;

/** One model's share of what a session and its subagents spent over a recent
 *  window, by token type, as the agent's own transcripts account for it. */
const ModelTokenUsage = z.object({
  model: z.string(),
  inputTokens: z.number(),
  outputTokens: z.number(),
  cacheReadTokens: z.number(),
  cacheWriteFiveMinuteTokens: z.number(),
  cacheWriteOneHourTokens: z.number()
});

export type ModelTokenUsage = z.infer<typeof ModelTokenUsage>;

/** What a session spent lately, its own replies apart from each subagent's -
 *  the subagent `id` is the one the agent's hooks name its tool calls by. */
export const SessionTokenUsage = z.object({
  own: z.array(ModelTokenUsage),
  subagents: z.array(
    z.object({
      id: z.string(),
      description: z.string().nullable(),
      /** When it last wrote to its transcript, epoch ms. */
      writtenAt: z.number(),
      usage: z.array(ModelTokenUsage)
    })
  )
});

export type SessionTokenUsage = z.infer<typeof SessionTokenUsage>;

/** The user's own Claude Code shortcuts (`~/.claude/keybindings.json`): blocks
 *  of chord → action per context, a null action unbinding the chord. */
export const ClaudeKeybindings = z.object({
  bindings: z.array(
    z.object({
      context: z.string(),
      bindings: z.record(z.string(), z.string().nullable())
    })
  )
});

export type ClaudeKeybindings = z.infer<typeof ClaudeKeybindings>;

/** One model an agent offers to switch to, as its own local catalog describes
 *  it - effort levels lowest first (with the agent's labels), empty for a
 *  model without one. */
export const ModelOption = z.object({
  id: z.string(),
  name: z.string(),
  efforts: z.array(
    z.object({
      id: z.string(),
      name: z.string()
    })
  ),
  recommendedEffort: z.string().nullable()
});

export type ModelOption = z.infer<typeof ModelOption>;

/** How a picked model reaches a running session (see `agent_apply_model`). */
export const SwitchOutcome = z.enum(["applied", "needsRestart"]);

export type SwitchOutcome = z.infer<typeof SwitchOutcome>;

/** The model and reasoning effort a session runs, as the agent reported it.
 *  The wire's `ultracode` (Claude's multi-agent orchestration is on for the
 *  session) arrives as the boolean-named `isUltracode`. */
export const RunningModel = z
  .object({
    model: z.string(),
    effort: z.string().nullable(),
    ultracode: z.boolean()
  })
  .transform(({ ultracode, ...reported }) => ({
    ...reported,
    isUltracode: ultracode
  }));

export type RunningModel = z.infer<typeof RunningModel>;

export const RunningModelChanged = z.object({
  sessionId: z.string().nullable()
});

export type RunningModelChanged = z.infer<typeof RunningModelChanged>;
