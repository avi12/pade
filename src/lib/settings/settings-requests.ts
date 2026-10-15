// The settings-returning commands, built on `sendMessage`: each runs in one
// serial queue and its fresh settings are adopted into the shared store only if
// no newer request has been adopted meanwhile, so request order - not response
// order - decides what every surface shows.

import { Command, sendMessage } from "@/lib/ipc/messaging";
import type { CommandProtocolMap, SettingsCommand } from "@/lib/ipc/messaging";
import { createSerialQueue } from "@/lib/settings/settings-state";
import { adoptSettings, beginSettingsRequest } from "@/lib/settings/settings.svelte";
import { AddRootStatus } from "@/lib/types";
import type { Settings } from "@/lib/types";

const enqueueSettingsRequest = createSerialQueue();

/** Run a settings-returning request in the serial queue, adopting what it hands
 *  `adopt` only if no newer request has been adopted meanwhile. The version is
 *  taken at call time, so request order - not response order - decides. */
function queueSettingsRequest<T>(request: (adopt: (fresh: Settings) => void) => Promise<T>) {
  const version = beginSettingsRequest();
  return enqueueSettingsRequest(() => request(fresh => adoptSettings({
    fresh,
    version
  })));
}

/** `sendMessage` for a command that answers the whole settings: the reply is
 *  adopted into the settings store (in request order) and handed back. */
export function sendSettingsMessage<Key extends SettingsCommand>(
  command: Key,
  ...data: Parameters<CommandProtocolMap[Key]>
) {
  return queueSettingsRequest(async adopt => {
    const fresh: Settings = await sendMessage(command, ...data);
    adopt(fresh);
    return fresh;
  });
}

/** Add a root folder. Only an `added` outcome carries settings to adopt; a
 *  missing path or a file comes back for the caller to act on. */
export function addRoot(data: Parameters<CommandProtocolMap[Command.workspaceAddRoot]>[0]) {
  return queueSettingsRequest(async adopt => {
    const outcome = await sendMessage(Command.workspaceAddRoot, data);
    if (outcome.status === AddRootStatus.enum.added) {
      adopt(outcome.settings);
    }

    return outcome;
  });
}
