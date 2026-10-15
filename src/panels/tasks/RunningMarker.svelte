<!--
  The running marker a task row's name trails while the agent runs that task:
  a pulsing dot and a "running" tag that springs out of it.
-->
<script lang="ts"></script>

<span class="run-dot" aria-hidden="true"></span>
<output class="run-tag">running</output>

<style>
  /* Green, dim-flashing while the agent is running this task. It springs in
     first, then settles into the shared breathing pulse. */
  .run-dot {
    flex: none;
    block-size: 7px;
    inline-size: 7px;
    border-radius: var(--radius-full);
    background: var(--tertiary);
    animation:
      run-dot-pop 420ms var(--spring) both,
      pulse 1100ms var(--ease) 420ms infinite;
  }

  /* Material 3 Expressive: the tag grows out of the dot with a spring's
     overshoot, so starting a task reads as something arriving. */
  .run-tag {
    display: inline-block;
    color: var(--tertiary);
    font-weight: 700;
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    transform-origin: left center;
    animation: run-tag-spring 480ms var(--spring) both;
  }

  /* Cyberpunk: a HUD readout decodes instead of springing. The tag types itself
     on one letter per step (RUNNING is seven) under a power-on flicker, and the
     dot blinks hard rather than breathing - the skin snaps, it does not glide.
     The skin also bevels every corner, and a full-size chamfer on a 7px dot cuts
     it down to a diagonal line - so the dot was invisible here. A 2px bevel
     keeps it a solid HUD pip. */
  :global(:root[data-palette="cyberpunk"]) .run-dot {
    border-radius: 2px;
    animation:
      run-dot-pop 160ms steps(2, jump-end) both,
      blink 1000ms steps(1, jump-end) 160ms infinite;
  }

  :global(:root[data-palette="cyberpunk"]) .run-tag {
    animation:
      run-tag-decode 420ms steps(7, jump-end) both,
      run-tag-flicker 420ms linear both;
  }

  @keyframes run-dot-pop {
    from {
      scale: 0;
    }

    to {
      scale: 1;
    }
  }

  @keyframes run-tag-spring {
    from {
      opacity: 0%;
      scale: 0.6;
      translate: -6px 0;
    }

    to {
      opacity: 100%;
      scale: 1;
      translate: 0 0;
    }
  }

  @keyframes run-tag-decode {
    from {
      clip-path: inset(0 100% 0 0);
    }

    to {
      clip-path: inset(0 0 0 0);
    }
  }

  @keyframes run-tag-flicker {
    0% {
      opacity: 0%;
    }

    12% {
      opacity: 100%;
    }

    22% {
      opacity: 25%;
    }

    36% {
      opacity: 100%;
    }

    48% {
      opacity: 45%;
    }

    62%,
    100% {
      opacity: 100%;
    }
  }
</style>
