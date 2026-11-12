import { NO_REPLY } from "@/lib/ipc/domains/no-reply";
import {
  Agent,
  ClaudeKeybindings,
  McpChange,
  ModelOption,
  RunningModel,
  RunningModelChanged,
  SessionContext,
  SessionTokenUsage,
  SwitchOutcome
} from "@/lib/types/agent";
import type { ModelChoice } from "@/lib/types/agent";
import { ChatStorage, RecordedConversation, RecordedConversationsChanged } from "@/lib/types/chat-storage";
import type { PacingControlPartKind, PacingRestUntil, PacingThrottle } from "@/lib/types/pacing";
import type { Scheme } from "@/lib/types/theme";
import { ShadowSummary } from "@/lib/usage/pacing/pacing-report";
import type { PacingReport } from "@/lib/usage/pacing/pacing-report";
import { z } from "zod";

export const AGENT_COMMANDS = {
  agentsDetect: "agents_detect",
  agentThemePublish: "agent_theme_publish",
  agentModelOptions: "agent_model_options",
  agentApplyModel: "agent_apply_model",
  agentSetPacingControl: "agent_set_pacing_control",
  agentPacingStopSeen: "agent_pacing_stop_seen",
  pacingReport: "pacing_report",
  pacingShadowSummary: "pacing_shadow_summary",
  claudeKeybindings: "claude_keybindings",
  agentResumeArgs: "agent_resume_args",
  agentConversations: "agent_conversations",
  chatStorage: "chat_storage",
  chatStorageDelete: "chat_storage_delete",
  agentSessionContext: "agent_session_context",
  sessionTokenUsage: "session_token_usage",
  agentRunningModel: "agent_running_model",
  projectAutoname: "project_autoname",
  sessionGenerateName: "session_generate_name",
  sessionSummarize: "session_summarize"
} as const;

export const AGENT_EVENTS = {
  agentModelChanged: "agent-model://changed",
  agentConversationsChanged: "agent-conversations://changed",
  mcpChanged: "mcp://changed"
} as const;

export interface AgentCommandProtocol {
  [AGENT_COMMANDS.agentsDetect]: () => Agent[];
  /** Re-theme every RUNNING session of a live-themed agent (Claude Code) by
   *  rewriting the theme definition it watches. Sent on every scheme flip: the
   *  agent repaints itself, so a conversation in flight never restarts to
   *  follow the app's appearance (see theming.rs). */
  [AGENT_COMMANDS.agentThemePublish]: (data: { scheme: Scheme }) => void;
  /** The models `command` offers to switch to, from the agent's own local
   *  catalog. Empty for an agent PADE can't read a catalog for. */
  [AGENT_COMMANDS.agentModelOptions]: (data: { command: string }) => ModelOption[];
  /** Apply a model / effort picked from the tab to the running session.
   *  `applied`: switched in place (Claude). `needsRestart`: the agent only
   *  takes a model at launch - relaunch it on its conversation. */
  [AGENT_COMMANDS.agentApplyModel]: (data: {
    sessionId: string;
    command: string;
    conversationId: string;
    choice: ModelChoice;
  }) => SwitchOutcome;
  /** Replace one part of a paced session's control, read by the agent's hook
   *  on every tool call: the model its subagents may spawn on (`cap`, null
   *  lifts it), whether it is being stopped (`stop`), when its rest ends
   *  (`rest`, epoch ms - its tool calls are held until then, null lifts it), or the account-wide
   *  round's per-subagent steps and spawn block (`throttle`). */
  [AGENT_COMMANDS.agentSetPacingControl]: (data: {
    command: string;
    conversationId: string;
    part: {
      part: typeof PacingControlPartKind.enum.cap;
      value: {
        model: string;
        allowedModels: string[];
      } | null;
    } | {
      part: typeof PacingControlPartKind.enum.stop;
      value: boolean;
    } | {
      part: typeof PacingControlPartKind.enum.rest;
      value: PacingRestUntil;
    } | {
      part: typeof PacingControlPartKind.enum.throttle;
      value: PacingThrottle;
    };
  }) => void;
  /** When the session's hook last answered a call while pacing was stopping
   *  it (epoch ms), null when it hasn't since the stop was written. */
  [AGENT_COMMANDS.agentPacingStopSeen]: (data: { conversationId: string }) => number | null;
  /** This window's pacing scan, for the backend engine to compare its own
   *  decisions with (shadow phase: it acts on nothing). */
  [AGENT_COMMANDS.pacingReport]: (data: PacingReport) => void;
  /** How often the engine's decisions agreed with this app's, per kind, and the
   *  latest disagreements. */
  [AGENT_COMMANDS.pacingShadowSummary]: () => ShadowSummary;
  /** The user's Claude Code keybindings as written, null when none. */
  [AGENT_COMMANDS.claudeKeybindings]: () => ClaudeKeybindings | null;
  /** Launch args that reopen the agent's recorded conversation for `cwd`
   *  (`["resume", "<uuid>"]` for Codex), empty when the agent has no such
   *  mechanism or no recording. `conversationId`: when the agent reported its
   *  own session id for it, that exact session is resumed. */
  [AGENT_COMMANDS.agentResumeArgs]: (data: {
    command: string;
    cwd: string;
    conversationId?: string;
  }) => string[];
  /** Every conversation the agents recorded for `project`, newest first -
   *  the sidebar's take on each CLI's `/resume` list. */
  [AGENT_COMMANDS.agentConversations]: (data: { project: string }) => RecordedConversation[];
  /** The disk every agent's chats take, and which chats these limits flag
   *  (a project's newest chat and last week's chats never are). */
  [AGENT_COMMANDS.chatStorage]: (data: {
    sizeLimitMegabytes: number | null;
    storageLimitMegabytes: number | null;
  }) => ChatStorage;
  /** Delete the ticked chats (`ids`, only ones the saved limits flag), then
   *  measure again. */
  [AGENT_COMMANDS.chatStorageDelete]: (data: { ids: string[] }) => ChatStorage;
  /** How full the session's context window is, read from the agent's own
   *  session log; `null` for an agent that keeps no readable log. */
  [AGENT_COMMANDS.agentSessionContext]: (data: {
    command: string;
    conversationId?: string;
  }) => SessionContext | null;
  /** What the session and each of its subagents spent since `since` (epoch
   *  ms), one entry per model, read from the agent's own transcripts; empty
   *  for an agent that keeps none. */
  [AGENT_COMMANDS.sessionTokenUsage]: (data: {
    command: string;
    conversationId?: string;
    since: number;
  }) => SessionTokenUsage;
  /** The model and effort the session runs, as the agent itself reported it
   *  (hooks, its session log, a plugin - never its terminal). */
  [AGENT_COMMANDS.agentRunningModel]: (data: {
    command: string;
    conversationId: string;
  }) => RunningModel | null;
  /** Suggest a name for a temp workspace via the agent CLI, else a heuristic. */
  [AGENT_COMMANDS.projectAutoname]: (data: {
    path: string;
    agent: string;
  }) => string | null;
  /** A concise session name from its transcript (null until there's enough
   *  conversation, or if nothing usable is produced). */
  [AGENT_COMMANDS.sessionGenerateName]: (data: {
    id: string;
    agent: string;
  }) => string | null;
  /** A sentence or two on what the session has done and is doing now, from its
   *  transcript, for the agents dashboard (null until there's enough). */
  [AGENT_COMMANDS.sessionSummarize]: (data: {
    id: string;
    agent: string;
    /** The conversation the session is pinned to, whose own log may tie it
     *  to a pull request the summary then links. */
    conversationId: string | null;
  }) => string | null;
}

export const AGENT_RESPONSE_SCHEMAS = {
  [AGENT_COMMANDS.agentsDetect]: z.array(Agent),
  [AGENT_COMMANDS.agentThemePublish]: NO_REPLY,
  [AGENT_COMMANDS.agentModelOptions]: z.array(ModelOption),
  [AGENT_COMMANDS.agentApplyModel]: SwitchOutcome,
  [AGENT_COMMANDS.agentSetPacingControl]: NO_REPLY,
  [AGENT_COMMANDS.agentPacingStopSeen]: z.number().nullable(),
  [AGENT_COMMANDS.pacingReport]: NO_REPLY,
  [AGENT_COMMANDS.pacingShadowSummary]: ShadowSummary,
  [AGENT_COMMANDS.claudeKeybindings]: ClaudeKeybindings.nullable(),
  [AGENT_COMMANDS.agentResumeArgs]: z.array(z.string()),
  [AGENT_COMMANDS.agentConversations]: z.array(RecordedConversation),
  [AGENT_COMMANDS.chatStorage]: ChatStorage,
  [AGENT_COMMANDS.chatStorageDelete]: ChatStorage,
  [AGENT_COMMANDS.agentSessionContext]: SessionContext.nullable(),
  [AGENT_COMMANDS.sessionTokenUsage]: SessionTokenUsage,
  [AGENT_COMMANDS.agentRunningModel]: RunningModel.nullable(),
  [AGENT_COMMANDS.projectAutoname]: z.string().nullable(),
  [AGENT_COMMANDS.sessionGenerateName]: z.string().nullable(),
  [AGENT_COMMANDS.sessionSummarize]: z.string().nullable()
} satisfies { [Key in keyof AgentCommandProtocol]: z.ZodType<ReturnType<AgentCommandProtocol[Key]>> };

export interface AgentEventProtocol {
  /** A session's model or effort may have changed: the agent reported
   *  (`sessionId` is its conversation id), or a settings file every session of
   *  an agent reads changed (`sessionId` null). */
  [AGENT_EVENTS.agentModelChanged]: (data: RunningModelChanged) => void;
  /** An agent's recorded conversations may have changed (started, renamed,
   *  grown, replaced) - throttled per project in the backend. */
  [AGENT_EVENTS.agentConversationsChanged]: (data: RecordedConversationsChanged) => void;
  /** Project MCP config changed in an agent's registered file. */
  [AGENT_EVENTS.mcpChanged]: (data: McpChange) => void;
}

export const AGENT_PAYLOAD_SCHEMAS = {
  [AGENT_EVENTS.agentModelChanged]: RunningModelChanged,
  [AGENT_EVENTS.agentConversationsChanged]: RecordedConversationsChanged,
  [AGENT_EVENTS.mcpChanged]: McpChange
} satisfies { [Key in keyof AgentEventProtocol]: z.ZodType<Parameters<AgentEventProtocol[Key]>[0]> };
