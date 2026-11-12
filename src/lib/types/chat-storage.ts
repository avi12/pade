import { z } from "zod";

/** One conversation an agent recorded for a project - what its own `/resume`
 *  picker lists. `resumeArguments` reopen it for an agent PADE can't pin by
 *  `conversationId` at spawn (Codex). */
export const RecordedConversation = z.object({
  agentId: z.string(),
  conversationId: z.string(),
  title: z.string().nullable(),
  updatedAt: z.number(),
  resumeArguments: z.array(z.string())
});

export type RecordedConversation = z.infer<typeof RecordedConversation>;

/** An agent's conversation store changed: `project` is the listed project it
 *  concerns, as a window asked about it, or null when the store names no
 *  project (Codex, Copilot) and every listed one should re-read. */
export const RecordedConversationsChanged = z.object({
  project: z.string().nullable()
});

export type RecordedConversationsChanged = z.infer<typeof RecordedConversationsChanged>;

/** Some chats' disk use: all of them, or what a cleanup removes. */
const ChatBytes = z.object({
  bytes: z.number().nonnegative(),
  chatCount: z.number().int().nonnegative()
});

/** One chat a cleanup removes, as the confirmation names it. */
const DeletedChat = z.object({
  /** What the confirmation hands back to delete this chat. */
  id: z.string(),
  agentId: z.string(),
  /** The agent's own title for it, when it gave one. */
  title: z.string().nullable(),
  /** The folder it ran in, when its files say. */
  project: z.string().nullable(),
  bytes: z.number().nonnegative(),
  /** Milliseconds since the Unix epoch. */
  updatedAt: z.number()
});

export type DeletedChat = z.infer<typeof DeletedChat>;

/** The disk the agents' recorded chats take, per agent, and what the limits
 *  asked about would delete (`cleanup`), its largest chats by name. */
export const ChatStorage = ChatBytes.extend({
  agents: z.array(ChatBytes.extend({ agentId: z.string() })),
  cleanup: ChatBytes.extend({ chats: z.array(DeletedChat) })
});

export type ChatStorage = z.infer<typeof ChatStorage>;
