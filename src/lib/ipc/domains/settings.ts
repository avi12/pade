import { z } from "zod";

import { ConfigFile, Settings } from "@/lib/types/settings";
import type { Prefs } from "@/lib/types/settings";
import { Scheme, TerminalScheme } from "@/lib/types/theme";

export const SETTINGS_COMMANDS = {
  configList: "config_list",
  configRead: "config_read",
  terminalSchemes: "terminal_schemes",
  settingsGet: "settings_get",
  setDefaultAgent: "set_default_agent",
  setProjectAgent: "set_project_agent",
  setPrefs: "set_prefs"
} as const;

export const SETTINGS_EVENTS = {
  settingsChanged: "settings://changed",
  themeChanged: "theme://changed"
} as const;

export interface SettingsCommandProtocol {
  /** The agent CLI's own config files - read, never shadowed. */
  [SETTINGS_COMMANDS.configList]: (data: { agent: string }) => ConfigFile[];
  /** `rel`: the config file's path relative to the project. */
  [SETTINGS_COMMANDS.configRead]: (data: { rel: string }) => string;
  /** The Windows Terminal colour schemes PADE can paint its terminal with. */
  [SETTINGS_COMMANDS.terminalSchemes]: () => TerminalScheme[];
  [SETTINGS_COMMANDS.settingsGet]: () => Settings;
  [SETTINGS_COMMANDS.setDefaultAgent]: (data: { agent: string }) => Settings;
  [SETTINGS_COMMANDS.setProjectAgent]: (data: { path: string; agent: string }) => Settings;
  [SETTINGS_COMMANDS.setPrefs]: (data: { patch: Partial<Prefs> }) => Settings;
}

export const SETTINGS_RESPONSE_SCHEMAS = {
  [SETTINGS_COMMANDS.configList]: z.array(ConfigFile),
  [SETTINGS_COMMANDS.configRead]: z.string(),
  [SETTINGS_COMMANDS.terminalSchemes]: z.array(TerminalScheme),
  [SETTINGS_COMMANDS.settingsGet]: Settings,
  [SETTINGS_COMMANDS.setDefaultAgent]: Settings,
  [SETTINGS_COMMANDS.setProjectAgent]: Settings,
  [SETTINGS_COMMANDS.setPrefs]: Settings
} satisfies { [Key in keyof SettingsCommandProtocol]: z.ZodType<ReturnType<SettingsCommandProtocol[Key]>> };

export interface SettingsEventProtocol {
  /** The shared settings changed in SOME window (broadcast) - re-read them
   *  through the one adoption path. */
  [SETTINGS_EVENTS.settingsChanged]: (data: null) => void;
  /** The native OS theme flipped (WebView2's media query can miss it). */
  [SETTINGS_EVENTS.themeChanged]: (data: Scheme) => void;
}

export const SETTINGS_PAYLOAD_SCHEMAS = {
  [SETTINGS_EVENTS.settingsChanged]: z.null(),
  [SETTINGS_EVENTS.themeChanged]: Scheme
} satisfies { [Key in keyof SettingsEventProtocol]: z.ZodType<Parameters<SettingsEventProtocol[Key]>[0]> };
