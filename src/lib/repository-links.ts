// One frontend path for opening repository and commit URLs. The backend owns
// remote normalization; this module owns browser launch and visible failures.

import { os, vcs } from "@/lib/bridge";
import { showToast } from "@/lib/stores/toast.svelte";

async function readRemoteUrl(project: string): Promise<string | null> {
  try {
    return await vcs.remoteUrl(project);
  } catch {
    showToast("Could not read Git remote");
    return null;
  }
}

async function launchRepositoryUrl(url: string): Promise<void> {
  try {
    await os.openUrl(url);
  } catch {
    showToast("Could not open repository in browser");
  }
}

async function openRepositoryTarget({ project, knownRemoteUrl, suffix = "" }: {
  project: string;
  knownRemoteUrl?: string | null;
  suffix?: string;
}): Promise<string | null> {
  const remoteUrl = knownRemoteUrl || await readRemoteUrl(project);
  if (!remoteUrl) {
    showToast("No Git remote configured");
    return null;
  }

  await launchRepositoryUrl(`${remoteUrl}${suffix}`);
  return remoteUrl;
}

/** Let a popover trigger keep its normal click while Ctrl/Cmd-click opens the
 * project's remote repository instead. */
export function openRepositoryOnModifiedClick({ project }: { project: string }) {
  return (element: HTMLElement) => {
    async function openWhenModified(e: MouseEvent): Promise<void> {
      if (!e.ctrlKey && !e.metaKey) {
        return;
      }

      e.preventDefault();
      await openRepositoryTarget({ project });
    }

    element.addEventListener("click", openWhenModified);
    return () => element.removeEventListener("click", openWhenModified);
  };
}
