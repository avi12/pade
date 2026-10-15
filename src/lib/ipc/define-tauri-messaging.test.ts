import { defineTauriMessaging } from "@/lib/ipc/define-tauri-messaging";
import {
  beforeEach,
  describe,
  expect,
  it,
  vi
} from "vitest";
import { z } from "zod";

const WINDOW_LABEL = "main-window";

type Deliver = (received: { payload: unknown }) => void;

const mocks = vi.hoisted(() => ({
  invoke: vi.fn<(command: string, data?: unknown) => Promise<unknown>>(),
  listen: vi.fn<(event: string, deliver: Deliver, options: unknown) => Promise<() => void>>()
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));
vi.mock("@/lib/ipc/platform", () => ({ windowLabel: () => WINDOW_LABEL }));

interface TestCommands {
  greet: (data: { name: string }) => string;
  ping: () => null;
}

interface TestEvents {
  tick: (data: { count: number }) => void;
}

const { sendMessage, onMessage } = defineTauriMessaging<TestCommands, TestEvents>({
  responseSchemas: {
    greet: z.string(),
    ping: z.null()
  },
  payloadSchemas: {
    tick: z.object({ count: z.number() })
  }
});

describe("defineTauriMessaging", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.listen.mockReset();
  });

  it("invokes the command with its argument object and parses the reply", async () => {
    mocks.invoke.mockResolvedValue("hello ada");

    await expect(sendMessage("greet", { name: "ada" })).resolves.toBe("hello ada");
    expect(mocks.invoke).toHaveBeenCalledWith("greet", { name: "ada" });
  });

  it("invokes a no-argument command without an argument object", async () => {
    mocks.invoke.mockResolvedValue(null);

    await expect(sendMessage("ping")).resolves.toBeNull();
    expect(mocks.invoke).toHaveBeenCalledWith("ping", undefined);
  });

  it("rejects a reply that does not match the command's schema", async () => {
    mocks.invoke.mockResolvedValue(42);

    await expect(sendMessage("greet", { name: "ada" })).rejects.toThrow();
  });

  it("passes the backend's rejection through unchanged", async () => {
    mocks.invoke.mockRejectedValue("not a repository");

    await expect(sendMessage("greet", { name: "ada" })).rejects.toBe("not a repository");
  });

  it("listens scoped to this window and hands each parsed payload over", async () => {
    const unlisten = vi.fn();
    mocks.listen.mockResolvedValue(unlisten);
    const handler = vi.fn();

    await expect(onMessage("tick", handler)).resolves.toBe(unlisten);
    const [event, deliver, options] = mocks.listen.mock.calls[0] ?? [];
    expect(event).toBe("tick");
    expect(options).toEqual({ target: WINDOW_LABEL });

    deliver?.({
      payload: {
        count: 3
      }
    });
    expect(handler).toHaveBeenCalledWith({
      data: {
        count: 3
      }
    });
    expect(() => deliver?.({
      payload: {
        count: "three"
      }
    })).toThrow();
  });
});
