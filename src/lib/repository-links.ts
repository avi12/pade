// One frontend path for opening repository and commit URLs. The backend owns
// remote normalization; this module owns browser launch and visible failures.

import { os, vcs } from "@/lib/bridge";
import { showToast } from "@/lib/stores/toast.svelte";

type RemoteProvider = {
  hosts: readonly string[];
  branchUrl: (remoteUrl: URL, branch: string) => string;
};

function pathBranchUrl({ remoteUrl, segment, branch }: {
  remoteUrl: URL;
  segment: string;
  branch: string;
}): string {
  return `${remoteUrl.toString().replace(/\/$/, "")}${segment}${encodeURIComponent(branch)}`;
}

const REMOTE_PROVIDERS: readonly RemoteProvider[] = [
  {
    hosts: ["github.com"],
    branchUrl: (remoteUrl, branch) => pathBranchUrl({
      remoteUrl,
      segment: "/tree/",
      branch
    })
  },
  {
    hosts: ["gitlab.com"],
    branchUrl: (remoteUrl, branch) => pathBranchUrl({
      remoteUrl,
      segment: "/-/tree/",
      branch
    })
  },
  {
    hosts: ["bitbucket.org"],
    branchUrl: (remoteUrl, branch) => pathBranchUrl({
      remoteUrl,
      segment: "/src/",
      branch
    })
  },
  {
    hosts: ["codeberg.org", "gitea.com"],
    branchUrl: (remoteUrl, branch) => pathBranchUrl({
      remoteUrl,
      segment: "/src/branch/",
      branch
    })
  },
  {
    hosts: ["dev.azure.com", "visualstudio.com"],
    branchUrl(remoteUrl, branch) {
      remoteUrl.searchParams.set("version", `GB${branch}`);
      return remoteUrl.toString();
    }
  }
];

/** Resolve a browsable branch URL for a known remote provider. Unknown hosts
 * safely retain the repository root until a provider entry is added. */
export function repositoryTargetUrl({ remoteUrl, branch }: {
  remoteUrl: string;
  branch?: string;
}): string {
  if (!branch) {
    return remoteUrl;
  }

  let parsedRemoteUrl: URL;
  try {
    parsedRemoteUrl = new URL(remoteUrl);
  } catch {
    return remoteUrl;
  }

  const provider = REMOTE_PROVIDERS.find(candidate => candidate.hosts.includes(parsedRemoteUrl.hostname));
  return provider?.branchUrl(parsedRemoteUrl, branch) ?? remoteUrl;
}

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

export async function openRepositoryTarget({ project, knownRemoteUrl, branch }: {
  project: string;
  knownRemoteUrl?: string | null;
  branch?: string;
}): Promise<string | null> {
  const remoteUrl = knownRemoteUrl || await readRemoteUrl(project);
  if (!remoteUrl) {
    showToast("No Git remote configured");
    return null;
  }

  await launchRepositoryUrl(
    repositoryTargetUrl({
      remoteUrl,
      branch
    })
  );
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
