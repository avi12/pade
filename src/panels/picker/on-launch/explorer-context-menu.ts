import { Command, sendMessage } from "@/lib/ipc/messaging";

/** Add or remove the Windows Explorer "Open in PADE" folder menu. Registering
 *  can fail where other settings cannot (the modern Win11 entry needs Developer
 *  Mode), so this resolves to the failure's message - empty on success - for the
 *  row to report instead of just flipping. */
export async function setExplorerContextMenu(isOn: boolean) {
  const command = isOn ? Command.contextMenuRegister : Command.contextMenuUnregister;
  try {
    await sendMessage(command);
    return "";
  } catch (caughtError) {
    return caughtError instanceof Error ? caughtError.message : String(caughtError);
  }
}
