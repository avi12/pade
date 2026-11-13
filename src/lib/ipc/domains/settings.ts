import { z } from "zod";

import { ConfigFile, Settings } from "@/lib/types/settings";
import type { Prefs } from "@/lib/types/settings";
import { Scheme, TerminalScheme } from "@/lib/types/theme";

export const SETTINGS_COMMANDS = {
  CONFIG_LIST: "config_list",
  CONFIG_READ: "config_read",
  TERMINAL_SCHEMES: "terminal_schemes",
  SETTINGS_GET: "settings_get",
  SET_DEFAULT_AGENT: "set_default_agent",
  SET_PROJECT_AGENT: "set_project_agent",
  SET_PREFS: "set_prefs"
} as const;

export const SETTINGS_EVENTS = {
  SETTINGS_CHANGED: "settings://changed",
  THEME_CHANGED: "theme://changed"
} as const;

export interface SettingsCommandProtocol {
  /** The agent CLI's own config files - read, never shadowed. */
  [SETTINGS_COMMANDS.CONFIG_LIST]: (data: { agent: string }) => ConfigFile[];
  /** `rel`: the config file's path relative to the project. */
  [SETTINGS_COMMANDS.CONFIG_READ]: (data: { rel: string }) => string;
  /** The Windows Terminal colour schemes PADE can paint its terminal with. */
  [SETTINGS_COMMANDS.TERMINAL_SCHEMES]: () => TerminalScheme[];
  [SETTINGS_COMMANDS.SETTINGS_GET]: () => Settings;
  [SETTINGS_COMMANDS.SET_DEFAULT_AGENT]: (data: { agent: string }) => Settings;
  [SETTINGS_COMMANDS.SET_PROJECT_AGENT]: (data: { path: string; agent: string }) => Settings;
  [SETTINGS_COMMANDS.SET_PREFS]: (data: { patch: Partial<Prefs> }) => Settings;
}

export const SETTINGS_RESPONSE_SCHEMAS = {
  [SETTINGS_COMMANDS.CONFIG_LIST]: z.array(ConfigFile),
  [SETTINGS_COMMANDS.CONFIG_READ]: z.string(),
  [SETTINGS_COMMANDS.TERMINAL_SCHEMES]: z.array(TerminalScheme),
  [SETTINGS_COMMANDS.SETTINGS_GET]: Settings,
  [SETTINGS_COMMANDS.SET_DEFAULT_AGENT]: Settings,
  [SETTINGS_COMMANDS.SET_PROJECT_AGENT]: Settings,
  [SETTINGS_COMMANDS.SET_PREFS]: Settings
} satisfies { [Key in keyof SettingsCommandProtocol]: z.ZodType<ReturnType<SettingsCommandProtocol[Key]>> };

export interface SettingsEventProtocol {
  /** The shared settings changed in SOME window (broadcast) - re-read them
   *  through the one adoption path. */
  [SETTINGS_EVENTS.SETTINGS_CHANGED]: (data: null) => void;
  /** The native OS theme flipped (WebView2's media query can miss it). */
  [SETTINGS_EVENTS.THEME_CHANGED]: (data: Scheme) => void;
}

export const SETTINGS_PAYLOAD_SCHEMAS = {
  [SETTINGS_EVENTS.SETTINGS_CHANGED]: z.null(),
  [SETTINGS_EVENTS.THEME_CHANGED]: Scheme
} satisfies { [Key in keyof SettingsEventProtocol]: z.ZodType<Parameters<SettingsEventProtocol[Key]>[0]> };
