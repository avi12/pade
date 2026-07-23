import {
  contextPct,
  dropContext,
  measuredContextPct,
  observeContext,
  observeContextScreen
} from "@/lib/stores/context.svelte";
import { describe, expect, it } from "vitest";

// The parser is internal, so every case drives it through the public API:
// observe a chunk, read the percent back. Each test uses its own session id —
// the store keeps module-level state, and fresh ids keep tests independent.

describe("contextPct via observeContext", () => {
  it("returns null for a session never observed", () => {
    expect(contextPct("never-observed")).toBeNull();
  });

  it("inverts a percent-remaining readout into percent used", () => {
    observeContext({
      id: "remaining-form",
      chunk: "Context: 37% remaining"
    });

    expect(contextPct("remaining-form")).toBe(63);
  });

  it("inverts the 'context left' remaining variant too", () => {
    observeContext({
      id: "context-left-form",
      chunk: "12% context left until compaction"
    });

    expect(contextPct("context-left-form")).toBe(88);
  });

  it("reads a percent-of-context used form directly", () => {
    observeContext({
      id: "used-form",
      chunk: "45% context consumed"
    });

    expect(contextPct("used-form")).toBe(45);
  });

  it("reads OpenCode's footer percent even with main-pane text on the row", () => {
    observeContext({
      id: "opencode-footer",
      chunk: "~ Writing command...\n342.4K (68%)  ctrl+p commands"
    });

    expect(contextPct("opencode-footer")).toBe(68);
  });

  it("ignores a bare parenthesized percent with no footer anchor", () => {
    observeContextScreen({
      id: "bare-percent",
      text: "coverage rose to 40K (12%) this week"
    });

    expect(measuredContextPct("bare-percent")).toBeNull();
  });

  it("trusts OpenCode's sidebar 'Context N tokens P% used' exactly", () => {
    observeContext({
      id: "opencode-sidebar",
      chunk: "Rename to video-time-manager\nContext 14,479 tokens 3% used $0.00 spent\nLSPs are disabled"
    });

    expect(contextPct("opencode-sidebar")).toBe(3);
  });

  it("never reads pasted CSS as a remaining-percent readout", () => {
    // A tool call dumped into the transcript: `left` and percentages with no
    // context/window anchor once produced a phantom "context nearly full".
    observeContextScreen({
      id: "css-junk",
      text: ".chip{left:4px;padding:9px 15px}.bar{width:97%;margin-left:2px}"
    });

    expect(measuredContextPct("css-junk")).toBeNull();
  });

  it("still inverts Claude's anchored 'context left until auto-compact' form", () => {
    observeContext({
      id: "claude-anchored-left",
      chunk: "Context left until auto-compact: 34%"
    });

    expect(contextPct("claude-anchored-left")).toBe(66);
  });

  it("computes the percent from a used/limit token ratio", () => {
    observeContext({
      id: "ratio-plain",
      chunk: "50000/200000 tokens"
    });

    expect(contextPct("ratio-plain")).toBe(25);
  });

  it("scales k and m suffixes in a token ratio", () => {
    observeContext({
      id: "ratio-k",
      chunk: "using 50k / 200k tokens"
    });
    observeContext({
      id: "ratio-m",
      chunk: "1m / 2m tokens in the window"
    });

    expect(contextPct("ratio-k")).toBe(25);
    expect(contextPct("ratio-m")).toBe(50);
  });

  it("strips thousands separators in a token ratio", () => {
    observeContext({
      id: "ratio-commas",
      chunk: "1,500 / 3,000 tokens"
    });

    expect(contextPct("ratio-commas")).toBe(50);
  });

  it("clamps an over-100 remaining percent to zero used", () => {
    observeContext({
      id: "clamp-remaining",
      chunk: "250% remaining"
    });

    expect(contextPct("clamp-remaining")).toBe(0);
  });

  it("clamps an over-100 used percent and an overflowing ratio to 100", () => {
    observeContext({
      id: "clamp-used",
      chunk: "999% context"
    });
    observeContext({
      id: "clamp-ratio",
      chunk: "300k/200k tokens"
    });

    expect(contextPct("clamp-used")).toBe(100);
    expect(contextPct("clamp-ratio")).toBe(100);
  });

  it("falls back to a chars-seen estimate when nothing parses", () => {
    // 80k chars ≈ 20k tokens of a 200k window → 10%.
    observeContext({
      id: "estimate-fallback",
      chunk: "x".repeat(80_000)
    });

    expect(contextPct("estimate-fallback")).toBeCloseTo(10);
  });

  it("accumulates the estimate across chunks and caps it at 100", () => {
    observeContext({
      id: "estimate-accumulates",
      chunk: "x".repeat(40_000)
    });
    observeContext({
      id: "estimate-accumulates",
      chunk: "x".repeat(40_000)
    });
    observeContext({
      id: "estimate-capped",
      chunk: "x".repeat(1_000_000)
    });

    expect(contextPct("estimate-accumulates")).toBeCloseTo(10);
    expect(contextPct("estimate-capped")).toBe(100);
  });

  it("keeps the last parsed percent across later non-matching chunks", () => {
    observeContext({
      id: "parsed-persists",
      chunk: "42% context"
    });
    observeContext({
      id: "parsed-persists",
      chunk: "plain build output with no signal"
    });

    expect(contextPct("parsed-persists")).toBe(42);
  });
});

describe("measuredContextPct — the signal automated decisions gate on", () => {
  it("returns null for a session never observed", () => {
    expect(measuredContextPct("measured-never")).toBeNull();
  });

  it("returns the parsed percent when the agent has reported one", () => {
    observeContext({
      id: "measured-parsed",
      chunk: "8% context left until compaction"
    });

    expect(measuredContextPct("measured-parsed")).toBe(92);
  });

  it("stays null on the byte estimate alone — the estimate never ends a session", () => {
    observeContext({
      id: "measured-estimate-only",
      chunk: "x".repeat(1_000_000)
    });

    // The soft gauge reads full from the bytes, but the measured signal — the
    // one auto-handoff/resume/retry act on — refuses to guess.
    expect(contextPct("measured-estimate-only")).toBe(100);
    expect(measuredContextPct("measured-estimate-only")).toBeNull();
  });
});

// Claude Code runs at up to a 1M-token window; the agent reports usage relative
// to its OWN window, so the parsed percent is window-agnostic — 92% of 1M trips
// the auto-handoff exactly as 92% of 200k would. These pin that the near-limit
// signal auto-handoff gates on lands in the required 90–95% band for a 1M window.
describe("measuredContextPct — 1M context window near the limit", () => {
  it("reads a near-full 1M window (920k/1M) as ≥ the 90% handoff threshold, within 90–95%", () => {
    observeContext({
      id: "ctx-1m-near",
      chunk: "context: 920k / 1m tokens"
    });

    const pct = measuredContextPct("ctx-1m-near");
    expect(pct).toBeCloseTo(92);
    expect(pct).toBeGreaterThanOrEqual(90);
    expect(pct).toBeLessThanOrEqual(95);
  });

  it("parses a '5% context left' readout on a 1M window into 95% used — still in band", () => {
    observeContext({
      id: "ctx-1m-left",
      chunk: "5% context left until compaction"
    });

    expect(measuredContextPct("ctx-1m-left")).toBe(95);
  });

  it("reads 88% from an 880k/1m ratio", () => {
    observeContext({
      id: "ctx-1m-low",
      chunk: "context: 880k / 1m tokens"
    });

    expect(measuredContextPct("ctx-1m-low")).toBeCloseTo(88);
  });
});

// A low handoff threshold needs the fill long before the agent prints its own
// % indicator, so the consumed-tokens counter against the announced window is
// the second agent-vouched source.
describe("measuredContextPct — derived from the tokens counter", () => {
  it("divides the reported total by the announced window", () => {
    observeContextScreen({
      id: "tokens-derived",
      text: "Opus 4.8 (1M context) with low effort"
    });
    observeContextScreen({
      id: "tokens-derived",
      text: "300,000 tokens"
    });

    expect(measuredContextPct("tokens-derived")).toBeCloseTo(30);
  });

  it("keeps the largest counter seen — small per-turn counts never regress it", () => {
    observeContextScreen({
      id: "tokens-max",
      text: "(200K context)"
    });
    observeContextScreen({
      id: "tokens-max",
      text: "46,706 tokens"
    });
    observeContextScreen({
      id: "tokens-max",
      text: "↓ 83 tokens"
    });

    expect(measuredContextPct("tokens-max")).toBeCloseTo(23.353);
  });

  it("assumes the largest window when the banner was never seen — under-reports, never cycles early", () => {
    observeContextScreen({
      id: "tokens-no-window",
      text: "191867 tokens"
    });

    // 191,867 over the 1M fallback ≈ 19% — a 200k session reads low rather
    // than a 1M session reading five times too high.
    expect(measuredContextPct("tokens-no-window")).toBeCloseTo(19.1867);
  });

  it("never mistakes a used/limit ratio's limit side for consumption", () => {
    observeContextScreen({
      id: "tokens-ratio-guard",
      text: "(1M context)"
    });
    observeContextScreen({
      id: "tokens-ratio-guard",
      text: "context: 100k / 1m tokens"
    });

    // The ratio parses to 10% used; the "1m" limit side must not become a
    // reported total that would read as 100%.
    expect(measuredContextPct("tokens-ratio-guard")).toBeCloseTo(10);
  });

  it("the agent's own % indicator still outranks the derived value", () => {
    observeContextScreen({
      id: "tokens-vs-parsed",
      text: "(1M context)"
    });
    observeContextScreen({
      id: "tokens-vs-parsed",
      text: "250,000 tokens"
    });
    observeContextScreen({
      id: "tokens-vs-parsed",
      text: "97% context used"
    });

    expect(measuredContextPct("tokens-vs-parsed")).toBe(97);
  });
});

describe("observeContextScreen", () => {
  it("parses the indicator from rendered rows when the stream split the word", () => {
    // The exact bytes captured from a live Claude session: the renderer skips
    // the already-painted "t" cell with a cursor-forward, so the stream never
    // carries the word "context" intact and the stream parser reads nothing.
    observeContext({
      id: "screen-split",
      chunk: "[10C1.0k tokens)[38;140H97% contex[1C used[40;3H"
    });

    // The split word defeats the % parser, but the intact per-turn tokens
    // counter still yields a (tiny) tokens-derived measurement.
    expect(measuredContextPct("screen-split")).toBeCloseTo(0.1);

    // The rendered screen row always holds the full phrase.
    observeContextScreen({
      id: "screen-split",
      text: "✻ Philosophising… (9s)                97% context used"
    });

    expect(measuredContextPct("screen-split")).toBe(97);
  });

  it("keeps the last parsed percent when a scanned row has no indicator", () => {
    observeContextScreen({
      id: "screen-sticky",
      text: "91% context used"
    });
    observeContextScreen({
      id: "screen-sticky",
      text: "just transcript text"
    });

    expect(measuredContextPct("screen-sticky")).toBe(91);
  });

  it("never inflates the byte estimate — repainted cells are not new output", () => {
    observeContextScreen({
      id: "screen-no-chars",
      text: "plain row with no indicator at all".repeat(100)
    });

    expect(contextPct("screen-no-chars")).toBeNull();
  });
});

describe("dropContext", () => {
  it("forgets the session entirely", () => {
    observeContext({
      id: "dropped-session",
      chunk: "42% context"
    });
    dropContext("dropped-session");

    expect(contextPct("dropped-session")).toBeNull();
  });
});
