import { describe, expect, it } from "vitest";

import { isGoalActive, observeGoalScreen } from "@/lib/stores/agent/sessionGoals.svelte";

describe("isGoalActive", () => {
  it("follows Claude's footer: on while it shows the goal, off once it doesn't", () => {
    observeGoalScreen({
      id: "goal-footer",
      text: "468481 tokens · ◎ /goal active (1h)"
    });
    expect(isGoalActive("goal-footer")).toBe(true);

    observeGoalScreen({
      id: "goal-footer",
      text: "468481 tokens"
    });
    expect(isGoalActive("goal-footer")).toBe(false);
  });
});
