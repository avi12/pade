import {
  SIDE_PANEL_MAXIMUM_WIDTH,
  SIDE_PANEL_MINIMUM_WIDTH,
  UI_SCALE_MAXIMUM,
  UI_SCALE_MINIMUM
} from "@/lib/settings/prefs-bounds";
import { ModelChoice } from "@/lib/types/agent";
import { AddedEditor } from "@/lib/types/editor";
import { DiffStyle, TerminalSchemeChoice, ThemeMode, ThemePalette } from "@/lib/types/theme";
import { MAXIMUM_HANDOFF_PERCENTAGE, MINIMUM_HANDOFF_PERCENTAGE } from "@/lib/usage/context-level";
import { z } from "zod";

export const ConfigFile = z.object({
  name: z.string(),
  rel: z.string(),
  kind: z.enum(["instructions", "mcp", "settings"]),
  exists: z.boolean()
});

export type ConfigFile = z.infer<typeof ConfigFile>;

export const StartMode = z.enum(["temp", "picker"]);

export type StartMode = z.infer<typeof StartMode>;

/** The idle windows offered for deleting unused temp workspaces, in days.
 *  `null` is the "Off" choice: temp workspaces stay until removed by hand. */
export const TEMP_CLEANUP_DAY_CHOICES = [null, 7, 30, 90] as const;

/** The size caps offered for one agent chat, in megabytes. `null` is "Off". */
export const CHAT_SIZE_LIMIT_CHOICES = [null, 100, 200, 500] as const;

/** The caps offered for all agent chats together, in megabytes (1, 2, 5 and
 *  10 GB). `null` is "Off". */
export const CHAT_STORAGE_LIMIT_CHOICES = [null, 1024, 2048, 5120, 10_240] as const;

/** Appearance & editor preferences. All optional - `null`/absent = use default. */
const Prefs = z.object({
  uiFont: z.string().nullish(),
  monoFont: z.string().nullish(),
  themeMode: ThemeMode.nullish(),
  /** The skin over the mode. Absent = the default palette. */
  themePalette: ThemePalette.nullish(),
  /** Windows Terminal colour schemes to paint the terminal with, one per app
   *  scheme. Absent on a side = that scheme uses the app theme's own palette. */
  terminalSchemes: TerminalSchemeChoice.nullish(),
  diffStyle: DiffStyle.nullish(),
  startMode: StartMode.nullish(),
  /** Auto-name temp workspaces once the agent has done real work (default on). */
  autoNameTemp: z.boolean().nullish(),
  /** Delete a temp workspace once it has gone unused this many days. Absent =
   *  off. Read by the backend's picker-refresh sweep (`workspace_prune`). */
  tempCleanupDays: z.number().int().positive().nullish(),
  /** Flag an agent chat once it grows past this many megabytes, for the user
   *  to tick for deletion. Absent = off. Read by `agents::storage`. */
  chatSizeLimitMegabytes: z.number().int().positive().nullish(),
  /** Flag each project's oldest chats in turn once all agent chats together
   *  pass this many megabytes. Absent = off. Read by `agents::storage` too. */
  chatStorageLimitMegabytes: z.number().int().positive().nullish(),
  /** Name session tabs with AI from what the agent is doing - the global default
   *  for every window. Opt-in: off unless explicitly true. */
  aiSessionNaming: z.boolean().nullish(),
  /** Per-project overrides of `aiSessionNaming` - project path → on/off. A
   *  project with no entry follows the global default. */
  aiSessionNamingProjects: z.record(z.string(), z.boolean()).nullish(),
  /** Per project-kind editor rules - kind (e.g. "web", "rust") → IDE id. */
  ideRules: z.record(z.string(), z.string()).nullish(),
  /** IDE id to open when no rule matches the project kind. */
  ideFallback: z.string().nullish(),
  /** Per-workspace starting model and effort for each agent - project path →
   *  agent id → choice. A new session of that agent in that project starts on
   *  it; an agent with no entry starts on its own default. */
  agentModelProjects: z.record(z.string(), z.record(z.string(), ModelChoice)).nullish(),
  /** Every workspace's starting model and effort for each agent - agent id →
   *  choice. A workspace's own pick (`agentModelProjects`) outranks it; with
   *  neither, the agent starts on its own default. */
  agentModelGlobal: z.record(z.string(), ModelChoice).nullish(),
  /** Explicit per-project editor picks - canonical project path → IDE id. A
   *  pick from the workspace's editor menu; outranks every rule for that
   *  project (`ide_suggest` puts it first). */
  ideProjectChoices: z.record(z.string(), z.string()).nullish(),
  /** Editors the user located by executable path (merged into detection). */
  addedEditors: z.array(AddedEditor).nullish(),
  /** Auto-hand-off to a fresh agent near the context limit. Opt-out: on unless
   *  explicitly set to false. */
  autoHandoff: z.boolean().nullish(),
  /** Render WebView2 on the CPU after the next restart, avoiding GPU contention. */
  softwareRender: z.boolean().nullish(),
  /** Percent of context at which the auto-handoff cycles the session. Absent =
   *  DEFAULT_CONTEXT_HANDOFF_PERCENTAGE (lib/usage/context-level). */
  handoffPct: z.number().min(MINIMUM_HANDOFF_PERCENTAGE).max(MAXIMUM_HANDOFF_PERCENTAGE).nullish(),
  /** Auto-resume a usage-limited session when its window resets - "continue"
   *  in place, or hand off when the context is nearly full. Opt-out: on unless
   *  explicitly set to false. */
  autoResume: z.boolean().nullish(),
  /** Slow agents down so neither usage limit runs out before it resets
   *  (lib/usage/pacing/pacing-policy). Opt-out: on unless explicitly false. */
  paceUsage: z.boolean().nullish(),
  /** UI + terminal zoom factor (0.85–1.30, step 0.05). Absent = default 1.0. */
  uiScale: z.number().min(UI_SCALE_MINIMUM).max(UI_SCALE_MAXIMUM).nullish(),
  /** Width, in pixels, of the resizable side panel (Feed/Git/Tasks/Config). The
   *  live layout additionally caps it at 60% of the window via a CSS clamp; this
   *  is the persisted drag target. Absent = default 380. */
  sidePanelWidth: z.number().min(SIDE_PANEL_MINIMUM_WIDTH).max(SIDE_PANEL_MAXIMUM_WIDTH).nullish(),
  /** Broadcast a Discord "Playing PADE" rich-presence status. Opt-in: off unless explicitly true. */
  discordPresence: z.boolean().nullish(),
  /** When presence is on, show the current project's name in the status. Default on. */
  discordShowProject: z.boolean().nullish(),
  /** Check GitHub for a newer PADE and download it in the background. Opt-out:
   *  on unless explicitly false (`update.rs` reads the same key). */
  autoUpdate: z.boolean().nullish()
});

export type Prefs = z.infer<typeof Prefs>;

/** Settings saved while Cyberpunk was a fourth theme mode carry it as
 *  `themeMode: "cyberpunk"`. It took its light/dark from the OS, so it reads as
 *  Auto wearing the Cyberpunk skin. */
const LegacyCyberpunkMode = z.looseObject({ themeMode: z.literal(ThemePalette.enum.cyberpunk) });

/** Prefs as stored, with a legacy Cyberpunk mode read in its two-axis shape. */
const StoredPrefs = z.preprocess(raw => {
  const parsed = LegacyCyberpunkMode.safeParse(raw);
  if (!parsed.success) {
    return raw;
  }

  return {
    ...parsed.data,
    themeMode: ThemeMode.enum.system,
    themePalette: ThemePalette.enum.cyberpunk
  };
}, Prefs);

export const Settings = z.object({
  roots: z.array(z.string()),
  defaultAgent: z.string().nullable(),
  projectAgents: z.record(z.string(), z.string()),
  recentProjects: z.array(z.string()).default([]),
  /** Projects pinned in the switcher - kept above the recents and independent of
   *  recent-history churn. */
  pinnedProjects: z.array(z.string()).default([]),
  ownedWorkspaces: z.array(z.string()).default([]),
  /** Friendly display names for workspaces, keyed by absolute path. */
  labels: z.record(z.string(), z.string()).default({}),
  /** When each project was last opened or left (Unix ms), keyed by canonical
   *  path - the switcher's "last accessed" time. */
  workspaceLastUsed: z.record(z.string(), z.number()).default({}),
  prefs: StoredPrefs.default({})
});

export type Settings = z.infer<typeof Settings>;
