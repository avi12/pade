<script lang="ts">
  import type { OpenTarget } from "@/lib/types";
  import type { HomeStart } from "@/panels/picker/home-composer";
  import { startFromHome } from "@/panels/picker/new-project";
  import type { CreateTarget } from "@/panels/picker/new-project";

  // The composer's Start: carries out what the composer resolved, busy while
  // a project is created or a throwaway workspace started.
  const { resolution, rawName, rawPrompt, agent, onopen, onprobe }: {
    resolution: HomeStart;
    rawName: string;
    rawPrompt: string;
    agent: string | null;
    onopen: (target: OpenTarget) => void;
    onprobe: (target: CreateTarget) => void;
  } = $props();

  let isStarting = $state(false);
  const isEnabled = $derived(resolution.kind !== "none" && !isStarting);
  const label = $derived.by(() => {
    if (isStarting) {
      return "Starting";
    }

    if (resolution.kind === "create") {
      return "Create and open";
    }

    return "Start";
  });

  async function start() {
    isStarting = true;
    try {
      await startFromHome({
        resolution,
        rawName,
        rawPrompt,
        agent,
        onopen,
        onprobe
      });
    } finally {
      isStarting = false;
    }
  }
</script>

<button class="go" disabled={!isEnabled} onclick={async () => isEnabled && await start()} type="button">
  {label}
</button>

<style>
  .go {
    flex: none;
  }
</style>
