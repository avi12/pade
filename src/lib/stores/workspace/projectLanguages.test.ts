import { Command, EventType } from "@/lib/ipc/messaging";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  type Mock,
  vi
} from "vitest";
import { z } from "zod";

const APP = "C:\\work\\app";
const API = "C:\\work\\api";
const WATCH_SETTLE_MS = 300;
const RECOUNT_SETTLE_MS = 1_500;
const LANGUAGES_ARGUMENTS = z.object({ paths: z.array(z.string()) });

type MessageHandler = (message: { data: unknown }) => void;

const mocks = vi.hoisted<{
  sendMessage: Mock<(command: Command, data?: unknown) => Promise<unknown>>;
  onMessage: Mock<(type: EventType, handler: MessageHandler) => Promise<() => void>>;
}>(() => ({
  sendMessage: vi.fn(),
  onMessage: vi.fn()
}));

vi.mock("@/lib/ipc/messaging", async importOriginal => ({
  ...await importOriginal<typeof import("@/lib/ipc/messaging")>(),
  sendMessage: mocks.sendMessage,
  onMessage: mocks.onMessage
}));

function share(language: string) {
  return {
    language,
    kind: "web",
    perMille: 1000
  };
}

/** The store under test, fresh, plus a way to fire `languages://changed`. */
async function freshStore() {
  vi.resetModules();
  const handlers: MessageHandler[] = [];
  mocks.onMessage.mockImplementation(async (type, handler) => {
    if (type === EventType.languagesChanged) {
      handlers.push(handler);
    }

    return () => undefined;
  });
  const store = await import("@/lib/stores/workspace/projectLanguages.svelte");
  return {
    store,
    announceChange(path: string) {
      for (const handler of handlers) {
        handler({ data: path });
      }
    }
  };
}

function watchedPaths() {
  return mocks.sendMessage.mock.calls
    .filter(([command]) => command === Command.watchLanguages)
    .map(([, data]) => data);
}

describe("followProjects", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    mocks.sendMessage.mockReset();
    mocks.onMessage.mockReset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("watches every followed project once the list settles", async () => {
    mocks.sendMessage.mockResolvedValue({});
    const { store } = await freshStore();

    store.followProjects({
      paths: [APP, API],
      async onchange() {}
    });
    await vi.advanceTimersByTimeAsync(WATCH_SETTLE_MS);

    expect(watchedPaths()).toEqual([{ paths: [APP, API] }]);
  });

  it("tells the window which followed project's mix moved after a change settles", async () => {
    let apiLanguage = "TypeScript";
    mocks.sendMessage.mockImplementation(async (command, data) => {
      if (command !== Command.ideProjectLanguages) {
        return undefined;
      }

      const { paths } = LANGUAGES_ARGUMENTS.parse(data);
      return Object.fromEntries(paths.map(path => [path, [share(path === API ? apiLanguage : "Rust")]]));
    });
    const { store, announceChange } = await freshStore();
    const onchange = vi.fn(async () => undefined);
    store.followProjects({
      paths: [APP, API],
      onchange
    });
    await vi.advanceTimersByTimeAsync(WATCH_SETTLE_MS);

    apiLanguage = "Go";
    announceChange(API);
    announceChange(API);
    await vi.advanceTimersByTimeAsync(RECOUNT_SETTLE_MS);

    expect(onchange).toHaveBeenCalledTimes(1);
    expect(onchange).toHaveBeenCalledWith(API);
    expect(store.projectLanguageShares(API)).toEqual([share("Go")]);
  });
});
