// The window's front project: the one source every project-scoped store reads
// (the Change Feed, the workspace members, the task catalog, the followed
// language mix, temp-workspace auto-naming). It holds no copy of its own - it
// reads the window's sessions through the source the window's services hand it
// once - so no store can ever point at a different project than the window.

let source = $state.raw<() => string>(() => "");

/** The project in front of this window; empty while none is. Reactive. */
export function frontProject() {
  return source();
}

/** Where the front project comes from - the window's sessions. Set once, by the
 *  window's services. */
export function setFrontProjectSource(nextSource: () => string) {
  source = nextSource;
}
