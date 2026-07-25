// One frontend path for opening repository and commit URLs. The backend owns
// remote normalization; this module owns browser launch and visible failures.

import { os, vcs } from "@/lib/bridge";
import { showToast } from "@/lib/stores/toast.svelte";

type RemoteProvider = {
  matches: (remoteUrl: URL) => boolean;
  branchUrl: (remoteUrl: URL, branch: string) => string;
  commitUrl: (remoteUrl: URL, commit: string) => string;
};

function pathTargetUrl({ remoteUrl, segment, target }: {
  remoteUrl: URL;
  segment: string;
  target: string;
}): string {
  return `${remoteUrl.toString().replace(/\/$/, "")}${segment}${encodeURIComponent(target)}`;
}

const REMOTE_PROVIDERS: readonly RemoteProvider[] = [
  {
    matches: remoteUrl => remoteUrl.hostname === "github.com",
    branchUrl: (remoteUrl, branch) => pathTargetUrl({
      remoteUrl,
      segment: "/tree/",
      target: branch
    }),
    commitUrl: (remoteUrl, commit) => pathTargetUrl({
      remoteUrl,
      segment: "/commit/",
      target: commit
    })
  },
  {
    matches: remoteUrl => remoteUrl.hostname === "gitlab.com",
    branchUrl: (remoteUrl, branch) => pathTargetUrl({
      remoteUrl,
      segment: "/-/tree/",
      target: branch
    }),
    commitUrl: (remoteUrl, commit) => pathTargetUrl({
      remoteUrl,
      segment: "/-/commit/",
      target: commit
    })
  },
  {
    matches: remoteUrl => remoteUrl.hostname === "bitbucket.org",
    branchUrl: (remoteUrl, branch) => pathTargetUrl({
      remoteUrl,
      segment: "/src/",
      target: branch
    }),
    commitUrl: (remoteUrl, commit) => pathTargetUrl({
      remoteUrl,
      segment: "/commits/",
      target: commit
    })
  },
  {
    matches: remoteUrl => ["codeberg.org", "gitea.com"].includes(remoteUrl.hostname),
    branchUrl: (remoteUrl, branch) => pathTargetUrl({
      remoteUrl,
      segment: "/src/branch/",
      target: branch
    }),
    commitUrl: (remoteUrl, commit) => pathTargetUrl({
      remoteUrl,
      segment: "/commit/",
      target: commit
    })
  },
  {
    matches: remoteUrl => remoteUrl.hostname === "dev.azure.com"
      || remoteUrl.hostname.endsWith(".visualstudio.com"),
    branchUrl(remoteUrl, branch) {
      remoteUrl.searchParams.set("version", `GB${branch}`);
      return remoteUrl.toString();
    },
    commitUrl: (remoteUrl, commit) => pathTargetUrl({
      remoteUrl,
      segment: "/commit/",
      target: commit
    })
  }
];
const CONVENTIONAL_DEFAULT_BRANCHES = ["main", "master"] as const;

/** Resolve a browsable branch URL for a known remote provider. Unknown hosts
 * safely retain the repository root until a provider entry is added. */
export function repositoryTargetUrl({ remoteUrl, branch, defaultBranch }: {
  remoteUrl: string;
  branch?: string;
  defaultBranch?: string | null;
}): string {
  if (!branch) {
    return remoteUrl;
  }

  const isDefaultBranch = defaultBranch
    ? branch === defaultBranch
    : CONVENTIONAL_DEFAULT_BRANCHES.some(candidate => candidate === branch);
  if (isDefaultBranch) {
    return remoteUrl;
  }

  let parsedRemoteUrl: URL;
  try {
    parsedRemoteUrl = new URL(remoteUrl);
  } catch {
    return remoteUrl;
  }

  const provider = REMOTE_PROVIDERS.find(candidate => candidate.matches(parsedRemoteUrl));
  return provider?.branchUrl(parsedRemoteUrl, branch) ?? remoteUrl;
}

/** Resolve a commit URL through the known remote provider. Unknown or malformed
 * remotes return null rather than fabricating a GitHub-shaped route. */
export function repositoryCommitUrl({ remoteUrl, commit }: {
  remoteUrl: string | null;
  commit: string;
}): string | null {
  if (!remoteUrl) {
    return null;
  }

  let parsedRemoteUrl: URL;
  try {
    parsedRemoteUrl = new URL(remoteUrl);
  } catch {
    return null;
  }

  const provider = REMOTE_PROVIDERS.find(candidate => candidate.matches(parsedRemoteUrl));
  return provider?.commitUrl(parsedRemoteUrl, commit) ?? null;
}

async function readRemoteUrl(project: string): Promise<string | null> {
  try {
    return await vcs.remoteUrl(project);
  } catch {
    showToast("Could not read Git remote");
    return null;
  }
}

async function readDefaultBranch(project: string): Promise<string | null> {
  try {
    return await vcs.defaultBranch(project);
  } catch {
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

  const defaultBranch = branch ? await readDefaultBranch(project) : null;
  await launchRepositoryUrl(
    repositoryTargetUrl({
      remoteUrl,
      branch,
      defaultBranch
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
