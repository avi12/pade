// A typed request/event layer over Tauri's IPC, shaped like
// `@webext-core/messaging`: a protocol map keyed by enum members, plus
// `sendMessage` / `onMessage`. Each reply and payload is parsed with its zod
// schema, so malformed data fails loudly at the boundary rather than deep in
// the UI.

import { windowLabel } from "@/lib/ipc/platform";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { z } from "zod";

/** One protocol entry: what the caller sends in, what comes back. */
type ProtocolEntry = (data: never) => unknown;

/** A command's single argument object, or nothing for a no-argument command. */
type CommandData<Entry extends ProtocolEntry> = Parameters<Entry>;

/** An event's payload - the one argument its protocol entry takes. */
type EventPayload<Entry extends ProtocolEntry> = Parameters<Entry>[0];

export function defineTauriMessaging<
  CommandProtocolMap extends { [CommandKey in keyof CommandProtocolMap]: ProtocolEntry },
  EventProtocolMap extends { [EventKey in keyof EventProtocolMap]: ProtocolEntry }
>({ responseSchemas, payloadSchemas }: {
  responseSchemas: { [CommandKey in keyof CommandProtocolMap]: z.ZodType<ReturnType<CommandProtocolMap[CommandKey]>> };
  payloadSchemas: { [EventKey in keyof EventProtocolMap]: z.ZodType<EventPayload<EventProtocolMap[EventKey]>> };
}) {
  /** Invoke `command` with its argument object (Tauri camelCases the Rust
   *  parameter names) and parse the reply with the command's schema. A Rust
   *  `Err` rejects with the backend's own message, unchanged. */
  async function sendMessage<CommandKey extends keyof CommandProtocolMap & string>(
    command: CommandKey,
    ...data: CommandData<CommandProtocolMap[CommandKey]>
  ) {
    const [argumentObject] = data;
    return responseSchemas[command].parse(await invoke(command, argumentObject));
  }

  /** Listen for `type`, parsing each payload before `handler` sees it. Scoped
   *  to this window's own label so a sibling window's targeted emit (the
   *  backend routes each window's file-watch changes back to it with `emit_to`)
   *  never leaks in; an app-wide `emit` broadcast still reaches every window's
   *  scoped listener. Resolves to the unlisten function. */
  function onMessage<EventKey extends keyof EventProtocolMap & string>(
    type: EventKey,
    handler: (message: { data: EventPayload<EventProtocolMap[EventKey]> }) => void
  ) {
    return listen(type, received => handler({ data: payloadSchemas[type].parse(received.payload) }), {
      target: windowLabel()
    });
  }

  return {
    onMessage,
    sendMessage
  };
}
