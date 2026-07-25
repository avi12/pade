export function replaceRecord<T extends object>(target: T, fresh: T): void {
  for (const key in target) {
    delete target[key];
  }
  Object.assign(target, fresh);
}

export function createVersionedAdopter<T>({ adopt }: {
  adopt: (fresh: T) => void;
}) {
  let nextVersion = 0;
  let adoptedVersion = 0;

  return {
    begin(): number {
      nextVersion += 1;
      return nextVersion;
    },
    adopt(fresh: T, version: number): boolean {
      if (version < adoptedVersion) {
        return false;
      }

      adoptedVersion = version;
      adopt(fresh);
      return true;
    }
  };
}

export function createSerialQueue() {
  let tail = Promise.resolve();

  return function enqueue<T>(operation: () => Promise<T>): Promise<T> {
    const request = tail.then(operation);
    tail = request.then(() => undefined, () => undefined);
    return request;
  };
}
