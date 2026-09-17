import {
  activityFromTitle,
  AgentActivity,
  agentReportsBusy,
  dropAgentActivity,
  observeAgentActivity,
  readTitles,
  whenAgentSettled
} from "@/lib/stores/agentActivity.svelte";
import { afterEach, describe, expect, it } from "vitest";

const ESCAPE = "";
const BELL = "";
const STRING_TERMINATOR = `${ESCAPE}\\`;

function titleSequence(title: string): string {
  return `${ESCAPE}]0;${title}${BELL}`;
}

const SESSION = "session-under-test";

afterEach(() => dropAgentActivity(SESSION));

describe("activityFromTitle", () => {
  it.each(["◐ Continue AVI learn math 5 units", "◑ Continue AVI learn math 5 units"])(
    "reads the spinner frame %j as busy",
    title => {
      expect(activityFromTitle(title)).toBe(AgentActivity.busy);
    }
  );

  it("reads the resting glyph as idle", () => {
    expect(activityFromTitle("✳ Sentry errors")).toBe(AgentActivity.idle);
  });

  it.each(["Claude Code", "pwsh", "◐Continue", "Rewriting ◐ parser", ""])(
    "reads %j as announcing nothing",
    title => {
      expect(activityFromTitle(title)).toBeNull();
    }
  );
});

describe("readTitles", () => {
  it("collects every title a stretch of output sets, in order", () => {
    const text = `${titleSequence("◐ Work")}frame${titleSequence("◑ Work")}${ESCAPE}]2;✳ Work${STRING_TERMINATOR}`;
    expect(readTitles(text)).toEqual({
      titles: ["◐ Work", "◑ Work", "✳ Work"],
      carried: ""
    });
  });

  it("ignores other operating-system commands", () => {
    const hyperlink = `${ESCAPE}]8;;https://example.com${BELL}link${ESCAPE}]8;;${BELL}`;
    const colorQuery = `${ESCAPE}]11;?${BELL}`;
    expect(readTitles(`${hyperlink}${colorQuery}`)).toEqual({
      titles: [],
      carried: ""
    });
  });

  it("carries a title the chunk cuts off", () => {
    const { titles, carried } = readTitles(`output${titleSequence("◐ Work")}${ESCAPE}]0;✳ Wo`);
    expect(titles).toEqual(["◐ Work"]);
    expect(carried).toBe(`${ESCAPE}]0;✳ Wo`);
  });
});

describe("observeAgentActivity", () => {
  it("follows the latest title the agent sets", () => {
    observeAgentActivity({
      id: SESSION,
      chunk: titleSequence("◐ Work")
    });
    expect(agentReportsBusy(SESSION)).toBe(true);

    observeAgentActivity({
      id: SESSION,
      chunk: `${titleSequence("◑ Work")}${titleSequence("✳ Work")}`
    });
    expect(agentReportsBusy(SESSION)).toBe(false);
  });

  it("keeps the last report through output that sets no title", () => {
    observeAgentActivity({
      id: SESSION,
      chunk: titleSequence("◐ Work")
    });
    observeAgentActivity({
      id: SESSION,
      chunk: "Waiting for 1 dynamic workflow to finish"
    });
    expect(agentReportsBusy(SESSION)).toBe(true);
  });

  it("reads a title split across two chunks", () => {
    observeAgentActivity({
      id: SESSION,
      chunk: titleSequence("◐ Work")
    });
    observeAgentActivity({
      id: SESSION,
      chunk: `frame${ESCAPE}]0;✳ Wo`
    });
    expect(agentReportsBusy(SESSION)).toBe(true);

    observeAgentActivity({
      id: SESSION,
      chunk: `rk${BELL}`
    });
    expect(agentReportsBusy(SESSION)).toBe(false);
  });

  it("clears the report once the agent sets a title without a glyph", () => {
    observeAgentActivity({
      id: SESSION,
      chunk: titleSequence("◐ Work")
    });
    observeAgentActivity({
      id: SESSION,
      chunk: titleSequence("pwsh")
    });
    expect(agentReportsBusy(SESSION)).toBe(false);
  });
});

describe("whenAgentSettled", () => {
  it("resolves at once for an agent that reports nothing", async () => {
    await expect(whenAgentSettled(SESSION)).resolves.toBeUndefined();
  });

  it("waits while the agent is busy and resolves when it goes idle", async () => {
    observeAgentActivity({
      id: SESSION,
      chunk: titleSequence("◐ Work")
    });
    let settled = false;
    async function awaitSettled() {
      await whenAgentSettled(SESSION);
      settled = true;
    }

    const waiting = awaitSettled();

    observeAgentActivity({
      id: SESSION,
      chunk: titleSequence("◑ Work")
    });
    await Promise.resolve();
    expect(settled).toBe(false);

    observeAgentActivity({
      id: SESSION,
      chunk: titleSequence("✳ Work")
    });
    await waiting;
    expect(settled).toBe(true);
  });

  it("resolves when the session is dropped mid-wait", async () => {
    observeAgentActivity({
      id: SESSION,
      chunk: titleSequence("◐ Work")
    });
    const waiting = whenAgentSettled(SESSION);
    dropAgentActivity(SESSION);
    await expect(waiting).resolves.toBeUndefined();
  });
});
