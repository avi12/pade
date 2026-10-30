import { effortStep, effortTone, modelLabel } from "@/lib/model/model-label";

/** What a session runs, as its agent last reported it (`sessionModel`). */
interface RunningModel {
  id: string;
  name: string;
  effort: string | null;
  isUltracode: boolean;
}

/** How a running model reads wherever a session shows it: the words ("Opus 5.5 ·
 *  high effort", plus Ultracode when on) and the effort tone its name is tinted
 *  in - null for a level no ladder ranks. `ladder` is the model's own effort
 *  levels from its agent's catalog, when known. */
export function runningModelLook({ running, ladder }: {
  running: RunningModel;
  ladder?: readonly string[];
}) {
  const modelAndEffort = modelLabel(running);
  const label = running.isUltracode ? `${modelAndEffort} · Ultracode on` : modelAndEffort;
  const step = running.effort === null ? null : effortStep({
    model: running.id,
    effort: running.effort,
    ladder
  });
  return {
    label,
    tone: step === null ? null : effortTone(step)
  };
}
