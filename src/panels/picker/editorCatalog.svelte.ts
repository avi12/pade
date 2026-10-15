import { Command, sendMessage } from "@/lib/ipc/messaging";
import { sendSettingsMessage } from "@/lib/settings/settings-requests";
import { settings } from "@/lib/settings/settings.svelte";
import type { EditorKind, Ide } from "@/lib/types";

/** The machine's editors as the picker knows them: the detected editors, the
 *  project kinds the rules engine shows (straight from the backend registry, its
 *  single home, in render/priority order) and the editor ids that suit each kind
 *  (kind → ordered), so a per-kind editor menu offers only fitting editors. One
 *  instance per picker - every section reads the same lists. */
export function createEditorCatalog() {
  let ides = $state<Ide[]>([]);
  let kinds = $state<EditorKind[]>([]);
  let kindOptions = $state<Record<string, string[]>>({});

  async function redetectEditors() {
    ides = await sendMessage(Command.ideDetect);
  }

  return {
    get ides() {
      return ides;
    },
    get kinds() {
      return kinds;
    },
    get kindOptions() {
      return kindOptions;
    },
    /** Full detection - editors, kinds and per-kind options (≈1.8s of
     *  registry/PATH probing, so only mount pays for it). */
    async detect() {
      const [detectedIdes, detectedKinds, detectedKindOptions] = await Promise.all([
        sendMessage(Command.ideDetect),
        sendMessage(Command.ideKinds),
        sendMessage(Command.ideKindOptions)
      ]);
      ides = detectedIdes;
      kinds = detectedKinds;
      kindOptions = detectedKindOptions;
    },
    /** Add an editor by executable path. The backend validates the executable and
     *  persists it; the detected list refreshes so it appears in every menu.
     *  Resolves to the new editor's label on success, or the rejection message. */
    async add(path: string) {
      try {
        await sendSettingsMessage(Command.ideAddEditor, { path });
        await redetectEditors();
        const added = settings.prefs.addedEditors?.find(editor => editor.path === path.trim());
        return { label: added?.label ?? "Editor" };
      } catch (error) {
        return { error: typeof error === "string" ? error : "Couldn’t add that editor" };
      }
    },
    /** Re-probe the machine for installed editors (the Editors "Reload" button).
     *  Resolves to how many were auto-detected (added editors aren't "detected")
     *  for the button's toast. */
    async rescan() {
      await redetectEditors();
      const addedIds = (settings.prefs.addedEditors ?? []).map(editor => editor.id);
      return ides.filter(editor => !addedIds.includes(editor.id)).length;
    },
    /** Drop a user-added editor, then refresh the detected list so it leaves
     *  every menu. The returned settings are adopted before detection refreshes. */
    async remove(id: string) {
      await sendSettingsMessage(Command.ideRemoveEditor, { id });
      await redetectEditors();
    }
  };
}
