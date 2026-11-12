// The single boundary between the UI and the Rust core (DRY + SoC), shaped like
// `@webext-core/messaging`: every `#[tauri::command]` is a `Command` member and
// every event channel an `EventType` member. Each domain module under
// `ipc/domains` owns its own members, protocol entries and response schemas
// (the zod schemas are the single source of truth for what comes back); this
// file only composes them into the one typed layer. Callers use
// `sendMessage(Command.x, data)` and `onMessage(EventType.x, ({ data }) => ...)`.

import { defineTauriMessaging } from "@/lib/ipc/define-tauri-messaging";
import { AGENT_COMMANDS, AGENT_EVENTS, AGENT_PAYLOAD_SCHEMAS, AGENT_RESPONSE_SCHEMAS } from "@/lib/ipc/domains/agent";
import type { AgentCommandProtocol, AgentEventProtocol } from "@/lib/ipc/domains/agent";
import { EDITOR_COMMANDS, EDITOR_RESPONSE_SCHEMAS } from "@/lib/ipc/domains/editor";
import type { EditorCommandProtocol } from "@/lib/ipc/domains/editor";
import { FEED_COMMANDS, FEED_EVENTS, FEED_PAYLOAD_SCHEMAS, FEED_RESPONSE_SCHEMAS } from "@/lib/ipc/domains/feed";
import type { FeedCommandProtocol, FeedEventProtocol } from "@/lib/ipc/domains/feed";
import {
  SETTINGS_COMMANDS,
  SETTINGS_EVENTS,
  SETTINGS_PAYLOAD_SCHEMAS,
  SETTINGS_RESPONSE_SCHEMAS
} from "@/lib/ipc/domains/settings";
import type { SettingsCommandProtocol, SettingsEventProtocol } from "@/lib/ipc/domains/settings";
import {
  SYSTEM_COMMANDS,
  SYSTEM_EVENTS,
  SYSTEM_PAYLOAD_SCHEMAS,
  SYSTEM_RESPONSE_SCHEMAS
} from "@/lib/ipc/domains/system";
import type { SystemCommandProtocol, SystemEventProtocol } from "@/lib/ipc/domains/system";
import {
  TERMINAL_COMMANDS,
  TERMINAL_EVENTS,
  TERMINAL_PAYLOAD_SCHEMAS,
  TERMINAL_RESPONSE_SCHEMAS
} from "@/lib/ipc/domains/terminal";
import type { TerminalCommandProtocol, TerminalEventProtocol } from "@/lib/ipc/domains/terminal";
import { USAGE_COMMANDS, USAGE_RESPONSE_SCHEMAS } from "@/lib/ipc/domains/usage";
import type { UsageCommandProtocol } from "@/lib/ipc/domains/usage";
import { VCS_COMMANDS, VCS_EVENTS, VCS_PAYLOAD_SCHEMAS, VCS_RESPONSE_SCHEMAS } from "@/lib/ipc/domains/vcs";
import type { VcsCommandProtocol, VcsEventProtocol } from "@/lib/ipc/domains/vcs";
import {
  WINDOW_COMMANDS,
  WINDOW_EVENTS,
  WINDOW_PAYLOAD_SCHEMAS,
  WINDOW_RESPONSE_SCHEMAS
} from "@/lib/ipc/domains/window";
import type { WindowCommandProtocol, WindowEventProtocol } from "@/lib/ipc/domains/window";
import {
  WORKSPACE_COMMANDS,
  WORKSPACE_EVENTS,
  WORKSPACE_PAYLOAD_SCHEMAS,
  WORKSPACE_RESPONSE_SCHEMAS
} from "@/lib/ipc/domains/workspace";
import type { WorkspaceCommandProtocol, WorkspaceEventProtocol } from "@/lib/ipc/domains/workspace";
import type { Settings } from "@/lib/types/settings";
import type { z } from "zod";

/** Every Rust `#[tauri::command]`, valued by its exact registered name. The
 *  drift guard (`messaging.test.ts`) holds this set equal to `lib.rs`'s
 *  `generate_handler!` list. */
export const Command = {
  ...AGENT_COMMANDS,
  ...EDITOR_COMMANDS,
  ...WINDOW_COMMANDS,
  ...SYSTEM_COMMANDS,
  ...TERMINAL_COMMANDS,
  ...FEED_COMMANDS,
  ...VCS_COMMANDS,
  ...USAGE_COMMANDS,
  ...WORKSPACE_COMMANDS,
  ...SETTINGS_COMMANDS
} as const;
export type Command = (typeof Command)[keyof typeof Command];

/** Every event channel the frontend listens to, valued by its wire name. The
 *  drift guard holds each one to an emit in `src-tauri/src` (the `tauri://`
 *  ones are Tauri's own). */
export const EventType = {
  ...AGENT_EVENTS,
  ...WINDOW_EVENTS,
  ...SYSTEM_EVENTS,
  ...TERMINAL_EVENTS,
  ...FEED_EVENTS,
  ...VCS_EVENTS,
  ...WORKSPACE_EVENTS,
  ...SETTINGS_EVENTS
} as const;
export type EventType = (typeof EventType)[keyof typeof EventType];

export interface CommandProtocolMap
  extends AgentCommandProtocol,
  EditorCommandProtocol,
  WindowCommandProtocol,
  SystemCommandProtocol,
  TerminalCommandProtocol,
  FeedCommandProtocol,
  VcsCommandProtocol,
  UsageCommandProtocol,
  WorkspaceCommandProtocol,
  SettingsCommandProtocol {}

interface EventProtocolMap
  extends AgentEventProtocol,
  WindowEventProtocol,
  SystemEventProtocol,
  TerminalEventProtocol,
  FeedEventProtocol,
  VcsEventProtocol,
  WorkspaceEventProtocol,
  SettingsEventProtocol {}

/** The commands that answer the whole settings - sent through
 *  `sendSettingsMessage` so every reply is adopted into the shared store. */
export type SettingsCommand = {
  [Key in Command]: ReturnType<CommandProtocolMap[Key]> extends Settings ? Key : never
}[Command];

const COMMAND_RESPONSE_SCHEMAS = {
  ...AGENT_RESPONSE_SCHEMAS,
  ...EDITOR_RESPONSE_SCHEMAS,
  ...WINDOW_RESPONSE_SCHEMAS,
  ...SYSTEM_RESPONSE_SCHEMAS,
  ...TERMINAL_RESPONSE_SCHEMAS,
  ...FEED_RESPONSE_SCHEMAS,
  ...VCS_RESPONSE_SCHEMAS,
  ...USAGE_RESPONSE_SCHEMAS,
  ...WORKSPACE_RESPONSE_SCHEMAS,
  ...SETTINGS_RESPONSE_SCHEMAS
} satisfies { [Key in Command]: z.ZodType<ReturnType<CommandProtocolMap[Key]>> };

const EVENT_PAYLOAD_SCHEMAS = {
  ...AGENT_PAYLOAD_SCHEMAS,
  ...WINDOW_PAYLOAD_SCHEMAS,
  ...SYSTEM_PAYLOAD_SCHEMAS,
  ...TERMINAL_PAYLOAD_SCHEMAS,
  ...FEED_PAYLOAD_SCHEMAS,
  ...VCS_PAYLOAD_SCHEMAS,
  ...WORKSPACE_PAYLOAD_SCHEMAS,
  ...SETTINGS_PAYLOAD_SCHEMAS
} satisfies { [Key in EventType]: z.ZodType<Parameters<EventProtocolMap[Key]>[0]> };

export const { sendMessage, onMessage } = defineTauriMessaging<CommandProtocolMap, EventProtocolMap>({
  responseSchemas: COMMAND_RESPONSE_SCHEMAS,
  payloadSchemas: EVENT_PAYLOAD_SCHEMAS
});
