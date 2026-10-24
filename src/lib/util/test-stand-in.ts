// A stand-in for a DOM object the node test environment doesn't have (an
// Element, a PointerEvent): only the members a test lists exist, so the code
// under test must read nothing else. Tests only - one home for the one place a
// value takes a type it doesn't fully have.

export function testStandIn<T extends object>(members: Partial<T>): T {
  return Object.assign(Object.create(null), members);
}
