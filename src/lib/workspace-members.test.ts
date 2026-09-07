import type { WorkspaceMember } from "@/lib/types";
import {
  enclosingRepository,
  hasSeveralMembers,
  memberLabel,
  memberOfGroup,
  memberPath,
  repositoryOfGroup,
  ROOT_GROUP_ID
} from "@/lib/workspace-members";
import { describe, expect, it } from "vitest";

const ROOT = "C:\\repositories\\avi\\pade";

function member(overrides: Partial<WorkspaceMember> = {}): WorkspaceMember {
  return {
    path: "",
    name: null,
    ecosystem: null,
    repository: false,
    ...overrides
  };
}

describe("memberPath", () => {
  it("is the root itself for the root member", () => {
    expect(
      memberPath({
        root: ROOT,
        member: member()
      })
    ).toBe(ROOT);
  });

  it("joins a nested member on the platform separator", () => {
    expect(
      memberPath({
        root: ROOT,
        member: member({ path: "apps/web" })
      })
    ).toBe("C:\\repositories\\avi\\pade\\apps\\web");
  });
});

describe("memberLabel", () => {
  it("prefers the name the manifest declares", () => {
    expect(
      memberLabel({
        root: ROOT,
        member: member({
          path: "apps/web",
          name: "@poll/web"
        })
      })
    ).toBe("@poll/web");
  });

  it("falls back to the member's own folder", () => {
    expect(
      memberLabel({
        root: ROOT,
        member: member({ path: "vendor/companion" })
      })
    ).toBe("companion");
  });

  it("names an unnamed root after the workspace folder", () => {
    expect(
      memberLabel({
        root: ROOT,
        member: member()
      })
    ).toBe("pade");
  });
});

describe("memberOfGroup", () => {
  const members = [member({ repository: true }), member({
    path: "apps/web",
    name: "web"
  })];

  it("maps the feed's root group onto the root member", () => {
    expect(
      memberOfGroup({
        groupId: ROOT_GROUP_ID,
        members
      })?.path
    ).toBe("");
  });

  it("maps a member group onto that member", () => {
    expect(
      memberOfGroup({
        groupId: "apps/web",
        members
      })?.name
    ).toBe("web");
  });

  it("has nothing for a group the folder-name convention invented", () => {
    // The fallback grouping only runs when no member was found, so those names
    // are folders, not members — they have no branch of their own to show.
    expect(
      memberOfGroup({
        groupId: "services/mailer",
        members
      })
    ).toBeUndefined();
  });
});

describe("hasSeveralMembers", () => {
  it("is false for a workspace that is only its root", () => {
    expect(hasSeveralMembers([member()])).toBe(false);
  });

  it("is true once a checkout or package joins it", () => {
    expect(
      hasSeveralMembers([member(), member({
        path: "companion",
        repository: true
      })])
    ).toBe(true);
  });
});

describe("enclosingRepository", () => {
  const members = [
    member({ repository: true }),
    member({
      path: "packages/ui",
      name: "@demo/ui"
    }),
    member({
      path: "companion",
      repository: true
    }),
    member({
      path: "companion/app",
      name: "app"
    })
  ];

  it("is the workspace's own checkout for a package that has none", () => {
    // A monorepo package commits to the repo around it — that is the branch its
    // changes actually land on.
    expect(
      enclosingRepository({
        path: "packages/ui",
        members
      })?.path
    ).toBe("");
  });

  it("is the nested checkout itself", () => {
    expect(
      enclosingRepository({
        path: "companion",
        members
      })?.path
    ).toBe("companion");
  });

  it("prefers the deepest checkout above a member", () => {
    expect(
      enclosingRepository({
        path: "companion/app",
        members
      })?.path
    ).toBe("companion");
  });

  it("has nothing when the workspace is not a repo at all", () => {
    expect(
      enclosingRepository({
        path: "packages/ui",
        members: [member(), member({ path: "packages/ui" })]
      })
    ).toBeUndefined();
  });
});

describe("repositoryOfGroup", () => {
  const members = [
    member({ repository: true }),
    member({
      path: "companion",
      repository: true
    })
  ];

  it("gives a nested checkout's group its own branch source", () => {
    expect(
      repositoryOfGroup({
        groupId: "companion",
        members
      })?.path
    ).toBe("companion");
  });

  it("falls back to the workspace's checkout for an invented group", () => {
    // The folder-name convention only runs with no members at all, but a group
    // key that matches nothing must still report the repo those files are in.
    expect(
      repositoryOfGroup({
        groupId: "services/mailer",
        members
      })?.path
    ).toBe("");
  });
});
