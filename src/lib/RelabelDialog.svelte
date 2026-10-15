<script lang="ts">
  import { workspace } from "@/lib/bridge";
  import ConfirmDialog from "@/lib/ConfirmDialog.svelte";
  import { errorMessage } from "@/lib/errors";
  import NameField from "@/lib/NameField.svelte";
  import { LabelSource } from "@/lib/types";
  import { fieldError, WorkspaceLabel } from "@/lib/validate";
  import { untrack } from "svelte";

  // Relabel a temp workspace inside PADE: the name the top bar, the switcher and
  // the picker show, never its folder (a live agent holds that as its cwd, which
  // the OS locks against rename). The label lands in settings - the one home every
  // surface reads through `displayName` - and, being the user's, auto-naming never
  // replaces it. One prompt for every entry point: the top-bar switcher opens it
  // `nested` over its menu, the picker as a modal.
  const { path, currentLabel, nested = false, onclose }: {
    path: string;
    /** The label shown today, prefilled for editing ("" when it has none). */
    currentLabel: string;
    nested?: boolean;
    /** Close the prompt - after a save, or on cancel. */
    onclose: () => void;
  } = $props();

  // Seeded once from the label shown when the prompt opened; the field owns it after.
  let value = $state(untrack(() => currentLabel));
  let saving = $state(false);
  let failure = $state<string | null>(null);
  const issue = $derived(
    fieldError({
      schema: WorkspaceLabel,
      raw: value
    })
  );

  async function save() {
    const parsed = WorkspaceLabel.safeParse(value);
    if (!parsed.success) {
      failure = parsed.error.issues[0].message;
      return;
    }

    saving = true;
    failure = null;
    try {
      await workspace.setLabel({
        path,
        name: parsed.data,
        source: LabelSource.enum.manual
      });
      onclose();
    } catch (error) {
      failure = errorMessage({
        error,
        fallback: "Couldn’t relabel that workspace."
      });
    } finally {
      saving = false;
    }
  }
</script>

<ConfirmDialog
  busy={saving}
  busyLabel="Saving…"
  confirmLabel="Save label"
  error={failure ?? issue}
  icon="pencil"
  {nested}
  oncancel={onclose}
  onconfirm={save}
  title="Relabel this workspace"
>
  <NameField
    label="Workspace label"
    onsubmit={save}
    {path}
    placeholder="What are you working on?"
    bind:value
  >
    Only the name PADE shows changes - the folder keeps its path.
  </NameField>
</ConfirmDialog>
