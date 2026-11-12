import { z } from "zod";

/** Remaining usage / quota for the active agent. Every field is optional so an
 *  adapter can surface only what it has locally; the whole payload is nullable
 *  (`null` = no reliable local signal, UI shows "usage -"). */
export const Usage = z.object({
  /** Percent of quota consumed, 0..100, when a precise number is known. */
  usedPct: z.number().nullish(),
  /** Short meter label (e.g. a plan/tier name). */
  label: z.string(),
  /** ISO-8601 reset time for the quota window, when known. */
  resetsAt: z.string().nullish(),
  /** Where the figure came from - so the UI stays honest about it. */
  source: z.string()
});

export type Usage = z.infer<typeof Usage>;

/** The semantic kind of a rate-limit window. The account endpoint returns a
 *  handful of named windows (the 5-hour session, the 7-day weekly all-models cap,
 *  the monthly spend cap on usage credits) plus any per-model caps; the backend
 *  classifies each. A named window it doesn't recognize arrives as `opaque` -
 *  surfaced honestly, never dropped. */
export const UsageWindowKind = z.enum(["session", "weekly", "model", "credits", "opaque"]);

export type UsageWindowKind = z.infer<typeof UsageWindowKind>;

/** One live rate-limit window mirrored from the same claude.ai OAuth usage
 *  endpoint Claude Code's `/usage` reads: a stable `key`, its `kind`, a human
 *  `label`, the 0..100 `utilization`, and an ISO-8601 reset time when known. */
const UsageWindow = z.object({
  /** Stable identity from the API (`five_hour`, `seven_day`, or a model name). */
  key: z.string(),
  kind: UsageWindowKind,
  /** Human name - a model's display name, or a humanized form of the key. */
  label: z.string(),
  /** Percent of the window consumed (0..100). */
  utilization: z.number(),
  /** ISO-8601 reset time for the window, when known. */
  resetsAt: z.string().nullish()
});

export type UsageWindow = z.infer<typeof UsageWindow>;

/** The state of pay-as-you-go usage credits (Claude's "extra usage"): `off`;
 *  `available` - on, with room left under the monthly cap, so they carry work past
 *  an exhausted window; `unavailable` - on, but the cap is spent or the account
 *  holds them back. */
export const CreditsState = z.enum(["off", "available", "unavailable"]);

export type CreditsState = z.infer<typeof CreditsState>;

/** A prepaid usage-credits balance: what's left, in the currency's major units. */
const CreditBalance = z.object({
  amount: z.number(),
  /** ISO 4217 code (`USD`). */
  currency: z.string()
});

export type CreditBalance = z.infer<typeof CreditBalance>;

/** Live account usage - every rate-limit window the endpoint returns (session,
 *  weekly, and any per-model or other windows), plus the plan label. `null` when
 *  offline / the local token is missing or expired. */
const AccountUsage = z.object({
  windows: z.array(UsageWindow).default([]),
  plan: z.string(),
  source: z.string(),
  /** Stable identity of the underlying billing account (e.g. Codex and opencode
   *  both bill one ChatGPT subscription), so the meter never counts the same
   *  account twice. Absent when the adapter can't name the account. */
  account: z.string().nullish(),
  /** Whether usage credits are on and can carry work past an exhausted window -
   *  what lets a limit-stopped session continue before its reset. Absent for an
   *  account with no credits concept. */
  credits: CreditsState.nullish(),
  /** The prepaid usage-credits balance - present only while extra usage is on
   *  (the backend reads it in that case alone). */
  creditBalance: CreditBalance.nullish(),
  /** What usage credits have cost this month when no monthly cap is set - the
   *  extra-usage row shows it in place of the percentage it cannot have. */
  uncappedSpend: CreditBalance.nullish()
});

export type AccountUsage = z.infer<typeof AccountUsage>;

/** Why an account has no reading (`usage.rs`'s `NoReadingReason`): PADE has no
 *  usage source for the agent; it isn't signed in; the vendor rejected its
 *  sign-in or it expired; or the vendor didn't answer (offline, a refusal). */
export const NoReadingReason = z.enum(["no_source", "signed_out", "sign_in_expired", "unreachable"]);

export type NoReadingReason = z.infer<typeof NoReadingReason>;

/** Why the latest ask brought no reading, and the CLI that fixes a sign-in
 *  problem (`codex` - also for opencode, which reads Codex's sign-in). */
const NoReading = z.object({
  reason: NoReadingReason,
  signInCommand: z.string().nullable()
});

export type NoReading = z.infer<typeof NoReading>;

/** The account command's answer: the reading to show - fresh, or the last good
 *  one held over while the vendor fails - and, when the latest ask brought
 *  none, why. */
export const AccountReading = z.object({
  account: AccountUsage.nullable(),
  noReading: NoReading.nullable()
});

export type AccountReading = z.infer<typeof AccountReading>;

/** One kind of free usage-limit reset Claude's account can spend now - the
 *  grants (launch promotions) that clear the same limits, once each, on
 *  demand. */
export const LimitReset = z.object({
  /** The grant a claim of this kind spends. */
  grantId: z.string(),
  /** That grant's own description. */
  label: z.string(),
  /** Every reset of this kind the account holds, across its grants. */
  resetsLeft: z.number(),
  /** When the first of them expires, ISO-8601. */
  endsAt: z.string().nullish(),
  /** Which limits it frees up, as Claude names the grant. */
  kind: z.enum(["session", "weekly", "fable", "full"]),
  /** Some limit it clears has use on it now; claude.ai greys out a reset
   *  that would clear nothing. */
  hasUsageToClear: z.boolean()
});

export type LimitReset = z.infer<typeof LimitReset>;

/** What spending a reset did, as the server reports it. */
export const LimitResetOutcome = z.object({
  reset: z.boolean(),
  /** The server's verdict (`reset`, `already_used`, `ineligible`, …). */
  result: z.string(),
  reason: z.string().nullish()
});

export type LimitResetOutcome = z.infer<typeof LimitResetOutcome>;
