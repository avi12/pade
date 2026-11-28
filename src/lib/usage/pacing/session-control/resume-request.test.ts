import { describe, expect, it } from "vitest";

import { resumeRequest } from "@/lib/usage/pacing/session-control/resume-request";

describe("resumeRequest", () => {
  it("says PADE paused the session, not the user", () => {
    const request = resumeRequest({
      subagents: [],
      hasStoppedSubagents: false
    });
    expect(request).toContain("not from the user");
    expect(request).not.toContain("subagent");
  });

  it("names each stopped subagent and asks for it to be restarted", () => {
    const request = resumeRequest({
      subagents: [
        {
          id: "a1",
          description: "Migrate youtube downloader"
        },
        {
          id: "a2",
          description: "Fix server reload"
        }
      ],
      hasStoppedSubagents: true
    });
    expect(request).toContain('"Migrate youtube downloader", "Fix server reload"');
    expect(request).toContain("start a new subagent for each");
    expect(request).toContain("without asking for confirmation");
  });

  it("still asks for a restart when no transcript named the subagents", () => {
    const request = resumeRequest({
      subagents: [
        {
          id: "a1",
          description: null
        }
      ],
      hasStoppedSubagents: true
    });
    expect(request).toContain("stopped your background subagents");
    expect(request).toContain("start a new subagent for each");
  });

  it("is one line, so pasting it submits it once", () => {
    const request = resumeRequest({
      subagents: [],
      hasStoppedSubagents: true
    });
    expect(request).not.toMatch(/[\r\n]/v);
  });
});
