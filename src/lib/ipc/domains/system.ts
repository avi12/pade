import { z } from "zod";

import { NO_REPLY } from "@/lib/ipc/domains/no-reply";
import { UpdateStatus } from "@/lib/types/update";

export const SYSTEM_COMMANDS = {
  CLIPBOARD_IMAGE_SAVE: "clipboard_image_save",
  DISCORD_SET_ACTIVITY: "discord_set_activity",
  DISCORD_CLEAR_ACTIVITY: "discord_clear_activity",
  CONTEXT_MENU_STATUS: "context_menu_status",
  CONTEXT_MENU_REGISTER: "context_menu_register",
  CONTEXT_MENU_UNREGISTER: "context_menu_unregister",
  UPDATE_STATUS: "update_status",
  UPDATE_CHECK: "update_check"
} as const;

export const SYSTEM_EVENTS = {
  UPDATE_AVAILABLE: "update://available"
} as const;

export interface SystemCommandProtocol {
  /** When the clipboard holds a bitmap, save it as a PNG in a bounded temp
   *  folder and answer its absolute path; null when text-only. */
  [SYSTEM_COMMANDS.CLIPBOARD_IMAGE_SAVE]: () => string | null;
  /** Report PADE on the user's Discord profile. `image` is the small overlay
   *  art-asset key, `caption` its hover text. */
  [SYSTEM_COMMANDS.DISCORD_SET_ACTIVITY]: (data: {
    details?: string;
    state?: string;
    image?: string;
    caption?: string;
  }) => void;
  [SYSTEM_COMMANDS.DISCORD_CLEAR_ACTIVITY]: () => void;
  /** Whether the Explorer "Open in PADE" folder context-menu entry is registered. */
  [SYSTEM_COMMANDS.CONTEXT_MENU_STATUS]: () => boolean;
  [SYSTEM_COMMANDS.CONTEXT_MENU_REGISTER]: () => void;
  [SYSTEM_COMMANDS.CONTEXT_MENU_UNREGISTER]: () => void;
  /** PADE's version and any newer release found. */
  [SYSTEM_COMMANDS.UPDATE_STATUS]: () => UpdateStatus;
  /** Check GitHub for a newer release now. */
  [SYSTEM_COMMANDS.UPDATE_CHECK]: () => UpdateStatus;
}

export const SYSTEM_RESPONSE_SCHEMAS = {
  [SYSTEM_COMMANDS.CLIPBOARD_IMAGE_SAVE]: z.string().nullable(),
  [SYSTEM_COMMANDS.DISCORD_SET_ACTIVITY]: NO_REPLY,
  [SYSTEM_COMMANDS.DISCORD_CLEAR_ACTIVITY]: NO_REPLY,
  [SYSTEM_COMMANDS.CONTEXT_MENU_STATUS]: z.boolean(),
  [SYSTEM_COMMANDS.CONTEXT_MENU_REGISTER]: NO_REPLY,
  [SYSTEM_COMMANDS.CONTEXT_MENU_UNREGISTER]: NO_REPLY,
  [SYSTEM_COMMANDS.UPDATE_STATUS]: UpdateStatus,
  [SYSTEM_COMMANDS.UPDATE_CHECK]: UpdateStatus
} satisfies { [Key in keyof SystemCommandProtocol]: z.ZodType<ReturnType<SystemCommandProtocol[Key]>> };

export interface SystemEventProtocol {
  /** A newer PADE release was found. */
  [SYSTEM_EVENTS.UPDATE_AVAILABLE]: (data: UpdateStatus) => void;
}

export const SYSTEM_PAYLOAD_SCHEMAS = {
  [SYSTEM_EVENTS.UPDATE_AVAILABLE]: UpdateStatus
} satisfies { [Key in keyof SystemEventProtocol]: z.ZodType<Parameters<SystemEventProtocol[Key]>[0]> };
