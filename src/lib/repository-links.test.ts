import { repositoryCommitUrl, repositoryTargetUrl } from "@/lib/repository-links";
import { describe, expect, it } from "vitest";

describe("repositoryTargetUrl", () => {
  it("maps branch routes through each registered provider", () => {
    expect(
      repositoryTargetUrl({
        remoteUrl: "https://github.com/avi/pade",
        branch: "feature/ui"
      })
    )
      .toBe("https://github.com/avi/pade/tree/feature%2Fui");
    expect(
      repositoryTargetUrl({
        remoteUrl: "https://gitlab.com/avi/pade",
        branch: "feature"
      })
    )
      .toBe("https://gitlab.com/avi/pade/-/tree/feature");
    expect(
      repositoryTargetUrl({
        remoteUrl: "https://bitbucket.org/avi/pade",
        branch: "feature"
      })
    )
      .toBe("https://bitbucket.org/avi/pade/src/feature");
    expect(
      repositoryTargetUrl({
        remoteUrl: "https://codeberg.org/avi/pade",
        branch: "feature"
      })
    )
      .toBe("https://codeberg.org/avi/pade/src/branch/feature");
  });

  it("uses the Azure branch query and safely falls back for unknown hosts", () => {
    expect(
      repositoryTargetUrl({
        remoteUrl: "https://dev.azure.com/avi/project/_git/pade",
        branch: "feature"
      })
    ).toBe("https://dev.azure.com/avi/project/_git/pade?version=GBfeature");
    expect(
      repositoryTargetUrl({
        remoteUrl: "https://git.example/avi/pade",
        branch: "main"
      })
    )
      .toBe("https://git.example/avi/pade");
  });

  it("keeps the remote root for its default branch", () => {
    expect(
      repositoryTargetUrl({
        remoteUrl: "https://github.com/avi/pade",
        branch: "trunk",
        defaultBranch: "trunk"
      })
    ).toBe("https://github.com/avi/pade");
    expect(
      repositoryTargetUrl({
        remoteUrl: "https://gitlab.com/avi/pade",
        branch: "main"
      })
    ).toBe("https://gitlab.com/avi/pade");
  });
});

describe("repositoryCommitUrl", () => {
  it("maps commit routes through each registered provider", () => {
    expect(
      repositoryCommitUrl({
        remoteUrl: "https://github.com/avi/pade",
        commit: "abc123"
      })
    )
      .toBe("https://github.com/avi/pade/commit/abc123");
    expect(
      repositoryCommitUrl({
        remoteUrl: "https://gitlab.com/avi/pade",
        commit: "abc123"
      })
    )
      .toBe("https://gitlab.com/avi/pade/-/commit/abc123");
    expect(
      repositoryCommitUrl({
        remoteUrl: "https://bitbucket.org/avi/pade",
        commit: "abc123"
      })
    )
      .toBe("https://bitbucket.org/avi/pade/commits/abc123");
    expect(
      repositoryCommitUrl({
        remoteUrl: "https://codeberg.org/avi/pade",
        commit: "abc123"
      })
    )
      .toBe("https://codeberg.org/avi/pade/commit/abc123");
    expect(
      repositoryCommitUrl({
        remoteUrl: "https://dev.azure.com/avi/project/_git/pade",
        commit: "abc123"
      })
    )
      .toBe("https://dev.azure.com/avi/project/_git/pade/commit/abc123");
    expect(
      repositoryCommitUrl({
        remoteUrl: "https://avi.visualstudio.com/project/_git/pade",
        commit: "abc123"
      })
    )
      .toBe("https://avi.visualstudio.com/project/_git/pade/commit/abc123");
  });

  it("does not invent commit routes for unsupported remotes", () => {
    expect(
      repositoryCommitUrl({
        remoteUrl: "https://git.example/avi/pade",
        commit: "abc123"
      })
    ).toBeNull();
    expect(
      repositoryCommitUrl({
        remoteUrl: null,
        commit: "abc123"
      })
    ).toBeNull();
  });
});
