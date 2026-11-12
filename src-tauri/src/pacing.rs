//! Usage pacing's pure math, ported from the TypeScript under
//! `src/lib/usage/pacing/` so the backend can run it. No I/O, no Tauri and no
//! state live here: every function takes what it reads and returns what it
//! decides, and each is held to the TypeScript's own output by the golden
//! fixtures in `pacing/fixtures/` (regenerate them with `scripts/pacing-fixtures.ts`).
//!
//! The submodules, one concern each: `units` (the typed numbers the formula
//! computes in), `reset_time` (a window's reset string as a point in time),
//! `usage_pace` (the pace formula), `policy` (its margins), `load_shedding`
//! (sharing an account's slowdown among its sessions), `subagent_steps` (the
//! holds on a running subagent), `account_round` (one round over every session
//! on every account), and the model reads they price by - `model_label`,
//! `model_cost` and `token_price`.

pub mod account_round;
pub mod engine;
pub mod facts;
#[cfg(test)]
mod golden_fixtures;
pub mod load_shedding;
pub mod model_cost;
pub mod model_label;
pub mod policy;
pub mod registry;
pub mod reset_time;
pub mod samples;
pub mod subagent_steps;
#[cfg(test)]
mod test_support;
pub mod token_price;
pub mod units;
pub mod usage_pace;
