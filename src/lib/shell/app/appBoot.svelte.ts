import { boot } from "@/lib/shell/app/app-boot-steps";
import type { BootDependencies } from "@/lib/shell/app/app-boot-steps";
import { Phase } from "@/lib/shell/app/screenPhase.svelte";
import { showToast } from "@/lib/stores/ui/toast.svelte";
import { disposeNewProjectDetection } from "@/lib/stores/workspace/active-project/new-project-detection";
import { onDestroy, onMount } from "svelte";

/** The window's cold start: where it lands first - re-attached sessions, its
 *  `w=` route, the launch context, the last project, or a throwaway workspace.
 *  Call during component initialisation; it boots on mount. */
export function startAppBoot(dependencies: BootDependencies) {
  onMount(async () => {
    try {
      await boot(dependencies);
    } catch {
      // Never strand the user on the splash - fall back to the project picker.
      showToast("Startup hit a snag - pick a project to continue");
      dependencies.screen.setPhase(Phase.Project);
    }
  });
  onDestroy(disposeNewProjectDetection);
}
