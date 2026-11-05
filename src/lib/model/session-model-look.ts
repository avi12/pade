// A session's running model as every place that shows it reads it: what its
// agent reported (`sessionModel`) and how that reads (`runningModelLook`),
// ranked on the model's own effort ladder from its agent's catalog. Reads
// reactive stores, so a component's `$derived` re-runs when either changes.

import { runningModelLook } from "@/lib/model/running-model";
import { sessionModel } from "@/lib/stores/agent/context.svelte";
import { modelOption } from "@/lib/stores/model/modelOptions.svelte";

/** Null until the agent has reported a model. Without `command`, the model
 *  family's usual ladder stands in for its own. */
export function sessionModelLook({ sessionId, command }: {
  sessionId: string;
  command?: string;
}) {
  const running = sessionModel(sessionId);
  if (running === null) {
    return null;
  }

  const ladder = command === undefined ? undefined : modelOption({
    command,
    model: running.id
  })?.efforts.map(option => option.id);
  return {
    running,
    ...runningModelLook({
      running,
      ladder
    })
  };
}
