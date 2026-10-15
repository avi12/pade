// One frontend path for opening repository and commit URLs. The backend owns
// remote normalization; this module owns browser launch and visible failures.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import { showToast } from "@/lib/stores/ui/toast.svelte";

const TRAILING_SLASH = /\/$/;
/** Azure DevOps addresses a branch as the `version` query `GB<branch>`. */
const AZURE_BRANCH_VERSION_PREFIX = "GB";

type RemoteProvider = {
  matches: (remoteUrl: URL) => boolean;
  branchUrl: (remoteUrl: URL, branch: string) => string;
  commitUrl: (remoteUrl: URL, commit: string) => string;
};

function pathTargetUrl({ remoteUrl, segment, target }: {
  remoteUrl: URL;
  segment: string;
  target: string;
}) {
  return `${remoteUrl.toString().replace(TRAILING_SLASH, "")}${segment}${encodeURIComponent(target)}`;
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
      remoteUrl.searchParams.set("version", `${AZURE_BRANCH_VERSION_PREFIX}${branch}`);
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

/** The remote parsed and paired with the provider that hosts it, or `null` for a
 *  malformed URL or an unknown host. Parsed fresh per call: a provider may
 *  rewrite the URL it is handed. */
function knownRemote(remoteUrl: string) {
  if (!URL.canParse(remoteUrl)) {
    return null;
  }

  const url = new URL(remoteUrl);
  const provider = REMOTE_PROVIDERS.find(candidate => candidate.matches(url));
  return provider ? {
    provider,
    url
  } : null;
}

/** Resolve a browsable branch URL for a known remote provider. Unknown hosts
 * safely retain the repository root until a provider entry is added. */
export function repositoryTargetUrl({ remoteUrl, branch, defaultBranch }: {
  remoteUrl: string;
  branch?: string;
  defaultBranch?: string | null;
}) {
  if (!branch) {
    return remoteUrl;
  }

  const isDefaultBranch = defaultBranch
    ? branch === defaultBranch
    : CONVENTIONAL_DEFAULT_BRANCHES.some(candidate => candidate === branch);
  if (isDefaultBranch) {
    return remoteUrl;
  }

  const remote = knownRemote(remoteUrl);
  return remote?.provider.branchUrl(remote.url, branch) ?? remoteUrl;
}

/** Resolve a commit URL through the known remote provider. Unknown or malformed
 * remotes return null rather than fabricating a GitHub-shaped route. */
export function repositoryCommitUrl({ remoteUrl, commit }: {
  remoteUrl: string | null;
  commit: string;
}) {
  if (!remoteUrl) {
    return null;
  }

  const remote = knownRemote(remoteUrl);
  return remote?.provider.commitUrl(remote.url, commit) ?? null;
}

async function readRemoteUrl(project: string) {
  try {
    return await sendMessage(Command.vcsRemoteUrl, { cwd: project });
  } catch {
    showToast("Could not read Git remote");
    return null;
  }
}

async function readDefaultBranch(project: string) {
  try {
    return await sendMessage(Command.vcsDefaultBranch, { cwd: project });
  } catch {
    return null;
  }
}

async function launchRepositoryUrl(url: string) {
  try {
    await sendMessage(Command.openUrl, { url });
  } catch {
    showToast("Could not open repository in browser");
  }
}

export async function openRepositoryTarget({ project, knownRemoteUrl, branch }: {
  project: string;
  knownRemoteUrl?: string | null;
  branch?: string;
}) {
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
    async function openWhenModified(e: MouseEvent) {
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
