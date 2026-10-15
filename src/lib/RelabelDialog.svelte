<script lang="ts">
  import { workspace } from "@/lib/bridge";
  import NameDialog from "@/lib/NameDialog.svelte";
  import { LabelSource } from "@/lib/types";
  import { WorkspaceLabel } from "@/lib/validate";

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
</script>

<NameDialog
  busyLabel="Saving…"
  confirmLabel="Save label"
  failureFallback="Couldn’t relabel that workspace"
  initialValue={currentLabel}
  label="Workspace label"
  {nested}
  {onclose}
  onsave={async name => {
    await workspace.setLabel({
      path,
      name,
      source: LabelSource.enum.manual
    });
  }}
  {path}
  placeholder="What are you working on?"
  schema={WorkspaceLabel}
  title="Relabel this workspace"
>
  Only the name PADE shows changes - the folder keeps its path
</NameDialog>
