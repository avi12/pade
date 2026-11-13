export interface Sibling {
  id: string;
  elItem: HTMLElement;
  /** Left edge in client coordinates, measured before anything moved. */
  left: number;
  /** Top edge in client coordinates, measured before anything moved. */
  top: number;
  width: number;
  height: number;
}

/** Where the pointer is, in client coordinates. */
export interface Point {
  x: number;
  y: number;
}

export function axisCenter({ slot, isHorizontal }: { slot: Sibling; isHorizontal: boolean }) {
  return isHorizontal ? slot.left + slot.width / 2 : slot.top + slot.height / 2;
}

export function slotTranslate({ from, to }: { from: Sibling; to: Sibling }) {
  return `${to.left - from.left}px ${to.top - from.top}px`;
}

// Measure the visible siblings and their geometry once, the moment the drag
// begins, so the insertion index is always computed against a stable pre-drag
// layout rather than the one the gaps are actively shifting. Hidden siblings
// (display:none → zero size, e.g. an off-screen pane) aren't reorderable.
export function measureSiblings({
  elParent,
  itemSelector,
  idAttribute
}: {
  elParent: HTMLElement;
  itemSelector: string;
  idAttribute: string;
}) {
  const siblings: Sibling[] = [];
  for (const elChild of elParent.children) {
    const isNotElChildInstanceOfHTMLElementOr = !(elChild instanceof HTMLElement) || !elChild.matches(itemSelector);
    if (isNotElChildInstanceOfHTMLElementOr) {
      continue;
    }

    const rectangle = elChild.getBoundingClientRect();
    const id = elChild.getAttribute(idAttribute);
    const isReorderable = rectangle.width > 0 && rectangle.height > 0 && id !== null && id !== "";
    if (!isReorderable) {
      continue;
    }

    siblings.push({
      id,
      elItem: elChild,
      left: rectangle.left,
      top: rectangle.top,
      width: rectangle.width,
      height: rectangle.height
    });
  }

  return siblings;
}

// Slide every non-dragged sibling from its original slot to the slot at its new
// position when the dragged item is inserted at `iBefore` among the others. Each
// is mapped to a real slot's geometry, so variable-width pills close and open
// exactly - no uniform-gap assumption. The siblings' transition is set once on
// lift, so this only writes `translate`.
export function reflowSiblings({
  siblings,
  originalIds,
  iOriginalById,
  draggedId,
  iBefore
}: {
  siblings: Sibling[];
  originalIds: string[];
  iOriginalById: Map<string, number>;
  draggedId: string;
  iBefore: number;
}) {
  const others = originalIds.filter(id => id !== draggedId);
  const order = [...others.slice(0, iBefore), draggedId, ...others.slice(iBefore)];
  for (const [i, id] of order.entries()) {
    const iOriginal = iOriginalById.get(id);
    const isIdEqualsDraggedIdOrI = id === draggedId || iOriginal === undefined;
    if (isIdEqualsDraggedIdOrI) {
      continue;
    }

    const sibling = siblings[iOriginal];
    sibling.elItem.style.translate = slotTranslate({
      from: sibling,
      to: siblings[i]
    });
  }
}
