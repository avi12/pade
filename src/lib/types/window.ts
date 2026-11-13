import { z } from "zod";

import type { RecordedConversation } from "@/lib/types/chat-storage";

/** How a spawned window routes off its `?w=` query string - `window_create`
 *  encodes the target here and the Rust `window` module emits the same tokens.
 *  The one home for these literals so boot routing, the IPC protocol, and the
 *  New-window menu can't drift. */
export const WindowMode = z.enum(["empty", "temp", "open", "dashboard", "handoff"]);

export type WindowMode = z.infer<typeof WindowMode>;

/** Which way `window_focus_relative` steps through the open windows. */
const WindowDirection = z.enum(["previous", "next"]);

export type WindowDirection = z.infer<typeof WindowDirection>;

/** Where the picker hands a chosen or freshly-created project back to the app:
 *  the project path, plus - for a create - an optional first prompt to seed and
 *  the agent id to launch it with (both absent when reopening an existing one). */
export interface OpenTarget {
  path: string;
  initialPrompt?: string;
  agent?: string;
  /** The directory was just created empty, so editor detection should wait for
   *  the agent's first project marker instead of keeping the generic fallback. */
  created?: true;
  /** A persisted chat to bring to the front when the project opens (the
   *  sidebar's chat pick). Its project's other chats come back with it. */
  chatId?: string;
  /** Open with a fresh chat added to the ones the project already has. */
  newChat?: boolean;
  /** A recorded conversation to resume when the project opens (the
   *  sidebar's `/resume` list). */
  resume?: RecordedConversation;
}

export const LaunchContext = z.object({
  hasProject: z.boolean(),
  cwd: z.string()
});

export type LaunchContext = z.infer<typeof LaunchContext>;

/** One open PADE window and the project it has, for the switcher's Open-windows
 *  list. `label` is the stable window id used to focus it. */
export const WindowInfo = z.object({
  label: z.string(),
  path: z.string(),
  isCurrent: z.boolean()
});

export type WindowInfo = z.infer<typeof WindowInfo>;

/** Physical (device-pixel) cursor position carried by the window's native
 *  drag-and-drop events - divide by devicePixelRatio before comparing with
 *  CSS-pixel rects. */
const DragPosition = z.object({
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
