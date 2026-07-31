export interface SelectionRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface Point {
  x: number;
  y: number;
}

export interface Bounds {
  width: number;
  height: number;
}

/**
 * Normalize a pointer drag into a selection, rejecting clicks and tiny drags.
 * This is shared by the pointer-release handler and its focused tests.
 */
export function selectionFromPointerRelease(
  start: Point,
  current: Point,
  minimumSize = 2,
): SelectionRect | null {
  const selection = {
    left: Math.min(start.x, current.x),
    top: Math.min(start.y, current.y),
    width: Math.abs(current.x - start.x),
    height: Math.abs(current.y - start.y),
  };

  return selection.width < minimumSize || selection.height < minimumSize
    ? null
    : selection;
}

/** Return true when a pointer is inside the completed selection. */
export function pointInSelection(
  point: Point,
  selection: SelectionRect,
): boolean {
  return (
    point.x >= selection.left &&
    point.x <= selection.left + selection.width &&
    point.y >= selection.top &&
    point.y <= selection.top + selection.height
  );
}

/**
 * Move a selection by a pointer delta, keeping it within the overlay bounds.
 * The selection dimensions are preserved, including when it reaches an edge.
 */
export function moveSelection(
  selection: SelectionRect,
  delta: Point,
  bounds: Bounds,
): SelectionRect {
  const maxLeft = Math.max(0, bounds.width - selection.width);
  const maxTop = Math.max(0, bounds.height - selection.height);

  return {
    ...selection,
    left: Math.min(maxLeft, Math.max(0, selection.left + delta.x)),
    top: Math.min(maxTop, Math.max(0, selection.top + delta.y)),
  };
}
