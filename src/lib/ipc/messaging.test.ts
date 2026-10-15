import { Command, EventType } from "@/lib/ipc/messaging";
import { describe, expect, it } from "vitest";

const RUST_SOURCES: Record<string, string> = import.meta.glob("/src-tauri/src/**/*.rs", {
  query: "?raw",
  import: "default",
  eager: true
});
const LIB_RS = "/src-tauri/src/lib.rs";
const HANDLER_LIST = /generate_handler!\[([^\]]*)\]/;
const TAURI_OWN_EVENT_PREFIX = "tauri://";

function registeredCommands() {
  const handlerList = HANDLER_LIST.exec(RUST_SOURCES[LIB_RS] ?? "")?.[1] ?? "";
  return handlerList
    .split(",")
    .map(entry => entry.trim().split("::").at(-1) ?? "")
    .filter(name => name.length > 0);
}

describe("IPC protocol drift guard", () => {
  it("names exactly the commands lib.rs registers", () => {
    const registered = registeredCommands();
    expect(registered.length).toBeGreaterThan(0);
    expect(new Set(Object.values(Command))).toEqual(new Set(registered));
  });

  it("listens only to events the backend emits", () => {
    const source = Object.values(RUST_SOURCES).join("\n");
    const appEvents = Object.values(EventType).filter(event => !event.startsWith(TAURI_OWN_EVENT_PREFIX));
    const missing = appEvents.filter(event => !source.includes(`"${event}"`));
    expect(missing).toEqual([]);
  });
});
