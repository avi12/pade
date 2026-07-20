// Maps app state (the open project + its language + prefs) to Discord rich-presence
// bridge calls — the one seam App.svelte drives, keeping the presence concern out
// of the shell (SoC). Presence is best-effort: Discord may be closed, so IPC
// failures are swallowed here rather than surfaced to the UI.

import { discord } from "@/lib/bridge";
import { displayName } from "@/lib/paths";

// Per project-kind Discord overlay: the small art-asset key uploaded to the public
// PADE app (mirrors the language logos in src/lib/icons) and the human label shown
// as the status line + icon hover. Keyed by the kind ids from language-icon.ts /
// the Rust kind registry, plus the `typescript` narrowing a web project can report.
// A kind absent here shows no language — just "Playing PADE / Working on <project>".
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

export async function updateDiscordPresence({ enabled, showProject, project, kind }: {
  enabled: boolean;
  showProject: boolean;
  project: string;
  /** The open project's detected kind (from `ide.projectKinds`), if any. */
  kind?: string;
}): Promise<void> {
  try {
    if (!enabled) {
      await discord.clearActivity();
      return;
    }

    // Presence on but hiding what I'm working on → just "Playing PADE".
    if (!(showProject && project)) {
      await discord.setActivity({});
      return;
    }

    // No labels map here — displayName falls back to the folder name, so the
    // status shows a clean project name rather than the full absolute path.
    const details = `Working on ${displayName({ path: project, labels: {} })}`;
    const language = kind ? LANGUAGES[kind] : undefined;
    await discord.setActivity({
      details,
      state: language?.label,
      image: language?.image,
      caption: language?.label
    });
  } catch {
    // Best-effort: a closed Discord (or any IPC hiccup) must never reach the UI.
  }
}
