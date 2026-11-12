// Golden fixtures for the Rust port of the usage-pacing math: it sweeps a grid of
// inputs (and a seeded random scatter) through the TypeScript functions and
// returns what they answer, so `src-tauri/src/pacing/golden_fixtures.rs` can hold
// the Rust functions to the same numbers. `pnpm pacing:fixtures` (from the repo
// root) writes them to `src-tauri/src/pacing/fixtures/`; run it whenever a pacing
// rule changes and commit the JSON it rewrites. The generators live per domain in
// `scripts/pacing-fixtures/`.
//
// Everything here is deterministic: a fixed clock and a seeded generator, so a
// rerun with the TypeScript unchanged rewrites byte-identical files.

import { roundScenarios } from "./pacing-fixtures/account-round-fixture";
import { loadSheddingFixture } from "./pacing-fixtures/load-shedding-fixture";
import { subagentStepsFixture } from "./pacing-fixtures/subagent-steps-fixture";
import { usagePaceFixture } from "./pacing-fixtures/usage-pace-fixture";

/** Every fixture file's contents, by the name it is written under
 *  (`src-tauri/src/pacing/fixtures/<name>.json`, by `pacing-fixtures.mjs`). */
export const pacingFixtures = {
  usage_pace: usagePaceFixture(),
  load_shedding: loadSheddingFixture(),
  subagent_steps: subagentStepsFixture(),
  account_round: {
    scenarios: roundScenarios()
  }
};
