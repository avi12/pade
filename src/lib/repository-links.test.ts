import { repositoryTargetUrl } from "@/lib/repository-links";
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
