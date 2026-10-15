/** Overwrite `target` in place with exactly `fresh`'s keys, so an optional key
 *  the fresh copy dropped doesn't survive on the reactive original. */
export function replaceRecord<T extends object>({ target, fresh }: {
  target: T;
  fresh: T;
}) {
  for (const key in target) {
    delete target[key];
  }
  Object.assign(target, fresh);
}

/** Adopt responses only in request order: a response to an older request that
 *  lands after a newer one has been adopted is dropped rather than rolling the
 *  state back. */
export function createVersionedAdopter<T>({ adopt }: {
  adopt: (fresh: T) => void;
}) {
  let nextVersion = 0;
  let adoptedVersion = 0;

  return {
    begin() {
      nextVersion += 1;
      return nextVersion;
    },
    adopt({ fresh, version }: {
      fresh: T;
      version: number;
    }) {
      if (version < adoptedVersion) {
        return false;
      }

      adoptedVersion = version;
      adopt(fresh);
      return true;
    }
  };
}

async function runAfter<T>({ previous, operation }: {
  previous: Promise<unknown>;
  operation: () => Promise<T>;
}) {
  await previous;
  return operation();
}

/** Resolves once `promise` settles either way - the queue's tail must keep
 *  moving past a failed operation, whose rejection its own caller handles. */
async function settled(promise: Promise<unknown>) {
  await Promise.allSettled([promise]);
}

/** Start each enqueued operation only after the previous one has finished. */
export function createSerialQueue() {
  let tail: Promise<unknown> = Promise.resolve();

  return function enqueue<T>(operation: () => Promise<T>) {
    const request = runAfter({
      previous: tail,
      operation
    });
    tail = settled(request);
    return request;
  };
}
