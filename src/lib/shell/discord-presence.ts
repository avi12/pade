// Maps app state (the open project + its language + prefs) to Discord rich-presence
// commands - the one seam `windowServices` drives, keeping the presence concern out
// of the shell (SoC). Presence is best-effort: Discord may be closed, so IPC
// failures are swallowed here rather than surfaced to the UI.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import { displayName } from "@/lib/util/paths";

// Per project-kind Discord overlay: the small art-asset key uploaded to the public
// PADE app (mirrors the language logos in src/lib/icons) and the human label shown
// as the status line + icon hover. Keyed by the kind ids from language-icon.ts /
// the Rust kind registry, plus the `typescript` narrowing a web project can report.
// A kind absent here shows no language - just "Playing PADE / Working on <project>".
const LANGUAGES: Record<string, {
  image: string;
  label: string;
}> = {
  web: {
    image: "javascript",
    label: "JavaScript"
  },
  typescript: {
    image: "typescript",
    label: "TypeScript"
  },
  python: {
    image: "python",
    label: "Python"
  },
  java: {
    image: "java",
    label: "Java"
  },
  go: {
    image: "go",
    label: "Go"
  },
  rust: {
    image: "rust",
    label: "Rust"
  },
  android: {
    image: "android",
    label: "Android"
  },
  cpp: {
    image: "cplusplus",
    label: "C++"
  },
  dotnet: {
    image: "csharp",
    label: "C#"
  },
  php: {
    image: "php",
    label: "PHP"
  },
  ruby: {
    image: "ruby",
    label: "Ruby"
  }
};

type PresenceState = {
  isEnabled: boolean;
  isProjectShown: boolean;
  project: string;
  /** The workspace-label map, so a renamed (auto-named) temp workspace reports
   *  its friendly name to Discord rather than its `temp-<stamp>` folder. */
  labels: Record<string, string>;
  /** The open project's detected kind (from `ide_project_kinds`), if any. */
  kind?: string;
};

/** A closed Discord (or any IPC hiccup) must never reach the UI, so a failed
 *  presence call is only noted for whoever is debugging. */
async function bestEffort(call: () => Promise<void>) {
  try {
    await call();
  } catch (error) {
    console.debug("pade: Discord presence unavailable", error);
  }
}

/** The activity to report, or `null` to report none at all. */
function presenceActivity({ isEnabled, isProjectShown, project, labels, kind }: PresenceState) {
  if (!isEnabled) {
    return null;
  }

  // Presence on but hiding what I'm working on → just "Playing PADE".
  if (!isProjectShown || !project) {
    return {};
  }

  return {
    details: `Working on ${displayName({
      path: project,
      labels
    })}`,
    ...languageFields(kind)
  };
}

/** The language mark and its name for a project kind, when PADE knows one. */
function languageFields(kind?: string) {
  const language = kind ? LANGUAGES[kind] : undefined;
  if (!language) {
    return {};
  }

  return {
    state: language.label,
    image: language.image,
    caption: language.label
  };
}

// Stop reporting entirely - used when the (focused, publishing) window closes so
// PADE's process-wide presence doesn't linger on the last project.
export async function clearDiscordPresence() {
  await bestEffort(() => sendMessage(Command.discordClearActivity));
}

export async function updateDiscordPresence(presence: PresenceState) {
  const activity = presenceActivity(presence);
  if (activity === null) {
    await bestEffort(() => sendMessage(Command.discordClearActivity));
    return;
  }

  await bestEffort(() => sendMessage(Command.discordSetActivity, activity));
}
