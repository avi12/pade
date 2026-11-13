import { z } from "zod";

import { AccountReading, LimitReset, LimitResetOutcome, Usage } from "@/lib/types/usage";

export const USAGE_COMMANDS = {
  USAGE_GET: "usage_get",
  USAGE_ACCOUNT_AGENT: "usage_account_agent",
  LIMIT_RESET_OFFERS: "limit_reset_offers",
  LIMIT_RESET_CLAIM: "limit_reset_claim"
} as const;

export interface UsageCommandProtocol {
  [USAGE_COMMANDS.USAGE_GET]: (data: { agent: string }) => Usage | null;
  /** Live usage windows for one agent, read from its vendor's own usage
   *  endpoint with the locally-stored token (or the agent's own CLI), and why
   *  there is no reading when the latest ask brought none. */
  [USAGE_COMMANDS.USAGE_ACCOUNT_AGENT]: (data: { agent: string; forceRefresh: boolean }) => AccountReading;
  /** The free usage-limit resets Claude's account can spend now, one per
   *  kind (the server's next kind first) - empty when it holds none. */
  [USAGE_COMMANDS.LIMIT_RESET_OFFERS]: () => LimitReset[];
  /** Spend one grant's reset: clears the limits it covers. Cannot be undone. */
  [USAGE_COMMANDS.LIMIT_RESET_CLAIM]: (data: { grantId: string }) => LimitResetOutcome;
}

export const USAGE_RESPONSE_SCHEMAS = {
  [USAGE_COMMANDS.USAGE_GET]: Usage.nullable(),
  [USAGE_COMMANDS.USAGE_ACCOUNT_AGENT]: AccountReading,
  [USAGE_COMMANDS.LIMIT_RESET_OFFERS]: z.array(LimitReset),
  [USAGE_COMMANDS.LIMIT_RESET_CLAIM]: LimitResetOutcome
} satisfies { [Key in keyof UsageCommandProtocol]: z.ZodType<ReturnType<UsageCommandProtocol[Key]>> };
