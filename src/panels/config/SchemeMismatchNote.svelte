<script lang="ts">
  import { isDarkSurface } from "@/lib/colors";
  import Icon from "@/lib/Icon.svelte";
  import { effective } from "@/lib/prefs.svelte";
  import { terminalSchemes } from "@/lib/stores/terminalSchemes.svelte";
  import { Scheme } from "@/lib/types";

  // Says when a face's terminal-scheme pick fights the app scheme it paints — a
  // dark scheme parked on Light, or a light one on Dark.
  //
  // The control this sits under promises "a light scheme by day, a dark one by
  // night", and nothing checked that the picks keep that promise: a dark scheme
  // in the Light slot silently leaves a near-black terminal inside a white app,
  // which reads as the theme failing to switch rather than as a choice. Only
  // ever said, never overridden — pinning a dark terminal under a light app is a
  // legitimate thing to want, and the pick is the user's.
  //
  // Its own component so the judgement and its wording live in one place rather
  // than inside the picker list's markup, which owns a different concern.
  const { face }: { face: Scheme } = $props();

  /** The chosen scheme for this face, resolved through the same catalogue store
   *  the picker renders from — so the scheme judged here and the one shown in
   *  the row beside it are always one answer, never two. */
  const chosen = $derived.by(() => {
    const name = effective.terminalSchemes[face];
    return terminalSchemes().find(scheme => scheme.name === name) ?? null;
  });

  const warning = $derived.by(() => {
    if (!chosen) {
      return null;
    }

    const schemeIsDark = isDarkSurface(chosen.background);
    if (schemeIsDark === (face === Scheme.enum.dark)) {
      return null;
    }

    if (schemeIsDark) {
      return "Dark scheme — the terminal stays dark by day";
    }

    return "Light scheme — the terminal stays light by night";
  });
</script>

{#if warning}
  <span class="mismatch"><Icon name="alert" />{warning}</span>
{/if}

<style>
  .mismatch {
    display: flex;
    gap: 5px;
    align-items: center;
    min-inline-size: 0;
    color: var(--warning);
    font-size: 0.82em;
  }
</style>
