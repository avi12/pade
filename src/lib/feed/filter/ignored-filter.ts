import type { ChangeEvent } from "@/lib/types/feed";
import type { Prefs } from "@/lib/types/settings";

/** Whether the feed hides what the project's `.gitignore` ignores: only where a
 *  `.gitignore` exists, and then on unless the user turned it off. */
export function isHidingIgnored({
  prefs,
  hasGitignore
}: {
  prefs: Pick<Prefs, "hideIgnoredChanges">;
  hasGitignore: boolean;
}) {
  return hasGitignore && prefs.hideIgnoredChanges !== false;
}

/** The feed's events with the ignored ones left out while it hides them. */
export function filterIgnored({ events, isHiding }: { events: ChangeEvent[]; isHiding: boolean }) {
  return isHiding ? events.filter(event => !event.isIgnored) : events;
}
