<script lang="ts">
  import type { AddRootOutcome, Ide, ProjectEntry } from "@/lib/types";
  import AddRootForm from "@/panels/picker/AddRootForm.svelte";
  import type { WorkspaceLifecycle } from "@/panels/picker/lifecycle.svelte";
  import RootProjects from "@/panels/picker/RootProjects.svelte";

  // Root folders: add one (a typed path with live validation + directory
  // autocomplete, or the native folder picker), remove one, and browse the
  // projects detected inside each. Root persistence reaches shared settings and
  // the per-root scan stays with the parent via onadd / onremove.
  const { roots, projectsByRoot, ides, lifecycle, onopen, onadd, onremove }: {
    roots: string[];
    projectsByRoot: Record<string, ProjectEntry[]>;
    ides: Ide[];
    lifecycle: WorkspaceLifecycle;
    onopen: (target: { path: string }) => void;
    onadd: (path: string, options: {
      create: boolean;
    }) => Promise<AddRootOutcome>;
    onremove: (path: string) => Promise<void>;
  } = $props();

  let addRootForm = $state<{ focusAddRoot: () => void } | null>(null);

  /** Focus the add-root field - how the quick-start form's "New root folder…"
   *  menu option lands the user here. Focus itself scrolls the field into view. */
  export function focusAddRoot() {
    addRootForm?.focusAddRoot();
  }
</script>

<section class="roots">
  <h2>Root folders</h2>
  <p class="hint">
    A root is a folder your projects live in - a home base for starting new work.
    Add one and PADE lists every project inside it below; open any in a click, and
    projects you create or clone there show up here automatically.
  </p>
  <AddRootForm bind:this={addRootForm} {onadd} {roots} />

  {#each roots as root (root)}
    <RootProjects
      {ides}
      {lifecycle}
      {onopen}
      {onremove}
      projects={projectsByRoot[root] ?? []}
      {root}
    />
  {/each}
</section>
