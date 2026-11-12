# Pacing engine in Rust - contract (shadow phase)

Goal: move usage pacing's DECISIONS from the frontend (TypeScript, one copy per window, elected leader) into
one Rust engine. This document is the contract for phase 1 (SHADOW): the Rust engine computes what it WOULD
decide, from facts the windows report, and compares with what the TypeScript actually decided. It acts on
nothing. Phase 2 (a later, separate change) switches the engine on and deletes the TypeScript copies.

## Principles (non-negotiable, from AGENTS.md and the user)
- Strong types: newtypes for epoch ms / percentages / rates, enums for closed sets, discriminated unions in TS
  (zod, `z.infer`), no stringly-typed state, no `any`, no `as` casts in Rust, exhaustive matches.
- DRY / SSOT: the pure math already exists in Rust (`src-tauri/src/pacing/*`, ported from TS and
  parity-tested with golden fixtures). REUSE it; never re-implement a formula. Constants live in exactly one
  place per language; any value that must exist on both sides is mirrored with a comment naming the other
  side AND is covered by a test or a golden fixture so drift fails CI.
- SoC: a PURE decision core (no I/O, no clock, no threads, fully unit-testable) separate from adapters
  (IPC commands, the tick scheduler, persistence, the shadow log). `lib.rs` only wires.
- Frontend rules: IPC only through `@/lib/ipc/messaging` (`Command`/`EventType` member + protocol-map entry +
  zod schema, all in its `ipc/domains/<domain>.ts`; the drift-guard test fails until all exist), `@/` imports, no `void`, etc.
- Rust rules: invoke the `rust-best-practices` skill; commands that do non-trivial work are `async fn`;
  never hold the app-wide mutex across I/O.

## What reports what (phase 1)
The frontend keeps owning everything only a window can see (screen parsing, model switching, terminal keys).
Each scan (every 60 s, per window) it sends ONE report to the backend:

```
pacing_report(report: PacingReport) -> ()           // Command.pacingReport, NO_REPLY

PacingReport  { windowLabel: string, at: EpochMs, sessions: SessionFacts[], observed: ObservedDecision[] }

SessionFacts  {
  id: string,                      // PADE session id
  conversationId: string | null,   // the agent's conversation id (control-file key)
  agentId: string,                 // "claude" | "codex" | ... (the usage account key)
  command: string,                 // the agent's command (control-file spelling)
  name: string,
  isWorking: bool,                 // working per the app (agentReportsBusy || status working)
  backgroundAgentCount: u32,
  isPacingOn: bool,                // the project's switch
  isHandingOff: bool, hasUsageLimit: bool,
  isSkipped: bool,                 // "Skip this window" active
  isContinuedRun: bool,            // the user's continue-anyway shield active
  isShielded: bool,                // handoff successor shield active
  isModelSwitchApplying: bool,
  workingSince: EpochMs | null,    // start of the current working stretch
  workingShare: f64,               // 0..1 smoothed working share
  ownLoad: f64, subagents: [{ id, description: string|null, load: f64 }],   // measured/estimated burn shares
  choice: ChoiceFacts | null,
  pace: PaceFacts | null           // the TS-owned pause state, null when not paused
}
ChoiceFacts { model: string|null, effort: string|null, original: {model,effort}|null,
              canStepDown: { effort: bool, model: bool },
              canStepUp: bool,                 // pacing has stepped this session down (`isSteppedDown`): there is a step to undo
              restoreAt: EpochMs | null,       // ADDED (rust agent): when the stepped-down choice goes back to the user's own
              effortSaving: f64, modelSaving: f64 }   // fraction of own load the next step of each kind saves
PaceFacts   { phase: "pausing"|"paused"|"overridden", windowKind: "session"|"weekly",
              readyAt: EpochMs, continueAt: EpochMs, restUntil: EpochMs|null, hasStoppedWork: bool,
              isHeld: bool, pausedAt: EpochMs,
              rates: Record<string, f64> }     // ADDED (rust agent): window key -> burn rate (percent per ms) when it last held the session

ObservedDecision { sessionId: string, at: EpochMs, kind: ObservedKind }
ObservedKind  = "stepEffortDown" | "stepModelDown" | "stepUp" | "rest" | "stop" | "resume" | "release" | "none"
```

Meaning of each kind (the TS call site it stands for; the engine's `Decision` maps one to one):

| Kind | TS decision point |
| --- | --- |
| `stepEffortDown` / `stepModelDown` | `stepModel` took the step (`adapter.stepDown`, effort / model) |
| `stepUp` | `restoreAssessed` stepped one notch back up (`adapter.stepUp`) |
| `rest` / `stop` | `pause` with a rest / a stop |
| `resume` | `resume`: lifted a pause, from `releaseOnePerAccount`, `continueAtReset` or a stand-down |
| `release` | `forgetPace` without a continue: an overridden session released, or one owned elsewhere |
| `none` | the session was looked at and nothing was decided (ignored by the comparison) |

`isRestPending` in the TS (`phase == pausing && restMs !== null`) is `phase == pausing && restUntil !== null`
in the engine: the two are always set together, so `restMs` is not sent.
(Use exactly these names/shapes; Rust: `#[serde(rename_all = "camelCase")]`, enums with serde names above;
TS: zod schemas, types via `z.infer`. Any field you find you genuinely cannot supply or need, add on BOTH
sides in the same change and note it in your report.)

Usage readings (windows, utilization, resets) are NOT reported: the engine reads them itself from the existing
Rust usage module (`usage_account_agent`'s data path), once per tick per agent - one SSOT.

## Rust side (new, under `src-tauri/src/pacing/`)
Reuse existing modules: `usage_pace`, `load_shedding`, `account_round`, `subagent_steps`, `policy`,
`token_price`, `units`, `reset_time`. Add (one file per concern):
- `facts.rs`   - the types above (serde, strong newtypes), plus `Decision` (see below).
- `registry.rs`- latest report per window label with freshness expiry (replaces the BroadcastChannel round
                 reports; reuse `account_round::fresh_reports`/`round_leader` semantics where they apply; there is no leader
                 election in the backend - one engine).
- `samples.rs` - per account-window burn samples (replaces the TS `samples` map + localStorage): record one
                 reading per tick while any session of the account works, bounded history
                 (RATE_HISTORY), persisted under the PADE data dir so a restart keeps it. Uses `usage_pace::burn_rate`.
- `engine.rs`  - the PURE core: `fn decide(inputs: &EngineInputs) -> EngineOutcome` (CHANGED from `Vec<Decision>`:
                 the outcome also carries the round's memory - subagent steps, when each session was stepped, when each
                 account last released one - and the accounts whose burn samples must start over). Port the decision logic of
                 `src/lib/stores/usage/usagePacing.svelte.ts` (assess working sessions, climb the step ladder, rest vs stop
                 via `work_rhythm`, advance paused sessions, release/continue, restore with room, the account-wide round)
                 as a pure function over `EngineInputs { now, sessions, accounts (readings), samples, ... }`.
                 `Decision` is an enum: StepEffortDown{session}, StepModelDown{session}, StepUp{session},
                 Rest{session, until}, Stop{session}, Resume{session}, Release{session},
                 Throttle{session, throttle} (round output), RestoreChoice{session}.
- `shadow.rs`  - compares `Decision`s with the window's `ObservedDecision`s (match by session + kind within one
                 scan), appends one JSONL line per scan to `<data dir>/pacing-shadow.jsonl` (agree/disagree,
                 both sides, the relevant facts) - bounded size (rotate/truncate), never blocks.
- `pacing.rs`  - wiring: the `pacing_report` command (async; clone/release locks before I/O), a tick task
                 (every 60 s via the Tauri async runtime, event/lifecycle driven where possible) that reads usage,
                 samples, runs `decide`, records the shadow comparison; and `pacing_shadow_summary` command
                 returning counts (agree/disagree per kind, last N disagreements) for inspection.
Shadow mode means: NO control-file writes, NO events that change behaviour, NO PTY writes.

## Frontend side
- `src/lib/stores/usage/pacingReport.svelte.ts`: builds `PacingReport` from the existing stores each scan
  (reads from `usagePacing`'s own state - do not recompute anything that already has an owner) and sends
  it via `sendMessage(Command.pacingReport, ...)`. Records each decision the TS actually takes during the scan as
  `ObservedDecision`s (a small typed recorder the TS pacing loop calls at its decision points).
- zod schemas next to the protocol (`@/lib/ipc/messaging` + `src/lib/types/<domain>.ts` conventions), types via
  `z.infer`; a drift-guard test must pass.
- Minimal, surgical edits to `usagePacing.svelte.ts`: only call sites for the recorder + the report; no
  behaviour change. Pure helpers in `src/lib/usage/pacing/`.

## Verification the work must include
- Rust: unit tests for every `decide` branch (ported from the behaviours in `usage-pacing.test.ts`, the
  `usage-pace`/`account-round` tests and the TS code paths), registry freshness, samples persistence round trip,
  shadow comparison logic.
- Frontend: tests for the report builder (pure part) and the recorder.
- All gates green: `pnpm check`, `pnpm lint` (clippy -D warnings, cargo fmt, fallow), `pnpm test`.
- ARCHITECTURE.md module map + docs/requirements.md note (shadow phase) in the same change.

## Shadow comparison and summary (as built)
- `Decision::RestoreChoice` (back on the user's own model and effort) and `Decision::Throttle` have no `ObservedKind`:
  the TypeScript records nothing for them, so neither is compared.
- Comparison is a multiset of (session, kind) per tick. A step the TypeScript took that the engine did not decide is a
  disagreement at once (`decided: null`). A step the engine decided that no window has reported yet waits ONE tick: if
  the same step shows up observed by then it agrees (with lag), otherwise it is a disagreement (`observed: null`). An
  observed `none` is never counted on its own.
- `pacing_shadow_summary` replies `{ counts: { <ObservedKind>: { agree, disagree } }, lastDisagreements: [{ at, sessionId,
  observed, decided }] }`: `counts` omits kinds with no samples, `lastDisagreements` keeps the latest 20. Counts live in
  memory (reset on restart); the JSONL log under the data dir (`pacing-shadow.jsonl`, rotated at 1 MiB to
  `pacing-shadow.1.jsonl`) is the durable record, one line per tick that compared anything.
- `pacing_report` and `pacing_shadow_summary` are synchronous commands: each is an in-memory lookup (AGENTS.md allows it).
- The burn samples (`pacing-samples.json`) are one book for the whole backend, not one per window.
