// The one place an agent's account usage is read from the backend, and the
// newest reading of each agent's account it brought. Everything that shows or
// acts on an account's windows - the usage meter, pacing, its tab badge, the
// handoff, the pickers - reads through here, so the same window never shows
// two numbers because two readers asked at different times.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import type { AccountUsage } from "@/lib/types";
import { SvelteMap } from "svelte/reactivity";

const latest = new SvelteMap<string, AccountUsage>();

/** Read an agent's account and keep the reading as its newest. A reading with
 *  no account (signed out, a refused request) leaves the newest one standing:
 *  each reader decides what a missing reading means for it. */
export async function fetchAccountReading({ agent, forceRefresh = false }: {
  agent: string;
  forceRefresh?: boolean;
}) {
  const reading = await sendMessage(Command.usageAccountAgent, {
    agent,
    forceRefresh
  });
  if (reading.account !== null) {
    latest.set(agent, reading.account);
  }

  return reading;
}

/** An agent's newest account reading (reactive), or null before its first. */
export function latestAccount(agent: string) {
  return latest.get(agent) ?? null;
}
