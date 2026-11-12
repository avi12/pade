import { AccountReading, LimitReset, LimitResetOutcome, Usage } from "@/lib/types/usage";
import { z } from "zod";

export const USAGE_COMMANDS = {
  usageGet: "usage_get",
  usageAccountAgent: "usage_account_agent",
  limitResetOffers: "limit_reset_offers",
  limitResetClaim: "limit_reset_claim"
} as const;

export interface UsageCommandProtocol {
  [USAGE_COMMANDS.usageGet]: (data: { agent: string }) => Usage | null;
  /** Live usage windows for one agent, read from its vendor's own usage
   *  endpoint with the locally-stored token (or the agent's own CLI), and why
   *  there is no reading when the latest ask brought none. */
  [USAGE_COMMANDS.usageAccountAgent]: (data: {
    agent: string;
    forceRefresh: boolean;
  }) => AccountReading;
  /** The free usage-limit resets Claude's account can spend now, one per
   *  kind (the server's next kind first) - empty when it holds none. */
  [USAGE_COMMANDS.limitResetOffers]: () => LimitReset[];
  /** Spend one grant's reset: clears the limits it covers. Cannot be undone. */
  [USAGE_COMMANDS.limitResetClaim]: (data: { grantId: string }) => LimitResetOutcome;
}

export const USAGE_RESPONSE_SCHEMAS = {
  [USAGE_COMMANDS.usageGet]: Usage.nullable(),
  [USAGE_COMMANDS.usageAccountAgent]: AccountReading,
  [USAGE_COMMANDS.limitResetOffers]: z.array(LimitReset),
  [USAGE_COMMANDS.limitResetClaim]: LimitResetOutcome
} satisfies { [Key in keyof UsageCommandProtocol]: z.ZodType<ReturnType<UsageCommandProtocol[Key]>> };
