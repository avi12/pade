import { NO_REPLY } from "@/lib/ipc/domains/no-reply";
import { UpdateStatus } from "@/lib/types/update";
import { z } from "zod";

export const SYSTEM_COMMANDS = {
  clipboardImageSave: "clipboard_image_save",
  discordSetActivity: "discord_set_activity",
  discordClearActivity: "discord_clear_activity",
  contextMenuStatus: "context_menu_status",
  contextMenuRegister: "context_menu_register",
  contextMenuUnregister: "context_menu_unregister",
  updateStatus: "update_status",
  updateCheck: "update_check",
  updateRestart: "update_restart"
} as const;

export const SYSTEM_EVENTS = {
  updateReady: "update://ready"
} as const;

export interface SystemCommandProtocol {
  /** When the clipboard holds a bitmap, save it as a PNG in a bounded temp
   *  folder and answer its absolute path; null when text-only. */
  [SYSTEM_COMMANDS.clipboardImageSave]: () => string | null;
  /** Report PADE on the user's Discord profile. `image` is the small overlay
   *  art-asset key, `caption` its hover text. */
  [SYSTEM_COMMANDS.discordSetActivity]: (data: {
    details?: string;
    state?: string;
    image?: string;
    caption?: string;
  }) => void;
  [SYSTEM_COMMANDS.discordClearActivity]: () => void;
  /** Whether the Explorer "Open in PADE" folder context-menu entry is registered. */
  [SYSTEM_COMMANDS.contextMenuStatus]: () => boolean;
  [SYSTEM_COMMANDS.contextMenuRegister]: () => void;
  [SYSTEM_COMMANDS.contextMenuUnregister]: () => void;
  /** PADE's version and whether a newer release is downloaded and ready. */
  [SYSTEM_COMMANDS.updateStatus]: () => UpdateStatus;
  /** Check GitHub for a newer release now, answering once any download ends. */
  [SYSTEM_COMMANDS.updateCheck]: () => UpdateStatus;
  /** Swap the ready release in and restart into it. */
  [SYSTEM_COMMANDS.updateRestart]: () => void;
}

export const SYSTEM_RESPONSE_SCHEMAS = {
  [SYSTEM_COMMANDS.clipboardImageSave]: z.string().nullable(),
  [SYSTEM_COMMANDS.discordSetActivity]: NO_REPLY,
  [SYSTEM_COMMANDS.discordClearActivity]: NO_REPLY,
  [SYSTEM_COMMANDS.contextMenuStatus]: z.boolean(),
  [SYSTEM_COMMANDS.contextMenuRegister]: NO_REPLY,
  [SYSTEM_COMMANDS.contextMenuUnregister]: NO_REPLY,
  [SYSTEM_COMMANDS.updateStatus]: UpdateStatus,
  [SYSTEM_COMMANDS.updateCheck]: UpdateStatus,
  [SYSTEM_COMMANDS.updateRestart]: NO_REPLY
} satisfies { [Key in keyof SystemCommandProtocol]: z.ZodType<ReturnType<SystemCommandProtocol[Key]>> };

export interface SystemEventProtocol {
  /** A newer PADE is downloaded, checked and ready to swap in. */
  [SYSTEM_EVENTS.updateReady]: (data: UpdateStatus) => void;
}

export const SYSTEM_PAYLOAD_SCHEMAS = {
  [SYSTEM_EVENTS.updateReady]: UpdateStatus
} satisfies { [Key in keyof SystemEventProtocol]: z.ZodType<Parameters<SystemEventProtocol[Key]>[0]> };
