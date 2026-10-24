<script lang="ts">
  import { CardViewKind } from "@/lib/feed/preview/card-view";
  import type { MediaView } from "@/lib/feed/preview/card-view";

  // An image or video card's picture or clip, in place of a text diff.
  const { view, name }: {
    view: MediaView;
    /** The file's name, the picture's alt text. */
    name: string;
  } = $props();
</script>

<div class="media-wrapper">
  {#if view.kind === CardViewKind.Video}
    <!-- Muted + looping so an opened card plays like a moving
         thumbnail; the controls unmute or scrub it. -->
    <video autoplay controls loop muted playsinline src={view.url}></video>
  {:else}
    <img alt={name} loading="lazy" src={view.url} />
  {/if}
</div>

<style>
  /* A subtle checkerboard (a token-tinted `color-mix`, so it adapts to
     light/dark on its own) sits under the media, letting transparent PNG/SVG
     regions read in both themes. */
  .media-wrapper {
    --checker: color-mix(in sRGB, var(--on-surface) 7%, transparent);

    display: grid;
    place-items: center;
    overflow: auto;
    max-block-size: 300px;
    padding: 12px;
    background-color: var(--code-background);
    background-image:
      linear-gradient(45deg, var(--checker) 25%, transparent 25%),
      linear-gradient(-45deg, var(--checker) 25%, transparent 25%),
      linear-gradient(45deg, transparent 75%, var(--checker) 75%),
      linear-gradient(-45deg, transparent 75%, var(--checker) 75%);
    background-position:
      0 0,
      0 8px,
      8px -8px,
      -8px 0;
    background-size: 16px 16px;

    img,
    video {
      object-fit: contain;
      block-size: auto;
      max-block-size: 276px;
      max-inline-size: 100%;
      border-radius: var(--radius-small);

      /* A hairline keeps a light image legible against the light checkerboard. */
      box-shadow: 0 0 0 1px color-mix(in sRGB, var(--outline) 60%, transparent);
    }
  }

  /* Fades and slides up as it replaces the source view - pure CSS on the
     entering element (never a View Transition, which would snapshot it). */
  @media (prefers-reduced-motion: no-preference) {
    .media-wrapper {
      @starting-style {
        opacity: 0%;
        translate: 0 8px;
      }

      transition:
        opacity 200ms var(--ease),
        translate 200ms var(--ease);
    }
  }
</style>
