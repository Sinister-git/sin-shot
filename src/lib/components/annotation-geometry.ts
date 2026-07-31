export interface GeometryRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface AnnotationFrameLayout {
  frame: GeometryRect;
  toolbar: GeometryRect;
  actions: GeometryRect;
}

/**
 * The dimensions here are the measured footprints of the controls in their
 * compact layout. They are inputs to one shared frame model, rather than CSS
 * nudges. The selected frame itself is never resized by this function.
 */
export const ANNOTATION_LAYOUT = {
  controlGap: 8,
  toolbarWidth: 48,
  // Measured compact footprints of the six tool buttons, swatches, and the
  // four action buttons. The viewport clamps these for short/narrow screens.
  toolbarHeight: 520,
  actionWidth: 360,
  actionHeight: 52,
} as const;

export function fitImageFrame(
  imageWidth: number,
  imageHeight: number,
  viewport: { width: number; height: number },
  reservedWidth = ANNOTATION_LAYOUT.toolbarWidth + ANNOTATION_LAYOUT.controlGap,
  reservedHeight = ANNOTATION_LAYOUT.actionHeight + ANNOTATION_LAYOUT.controlGap,
): GeometryRect {
  const sourceWidth = Math.max(1, imageWidth);
  const sourceHeight = Math.max(1, imageHeight);
  const availableWidth = Math.max(1, viewport.width - reservedWidth);
  const availableHeight = Math.max(1, viewport.height - reservedHeight);
  const scale = Math.min(1, availableWidth / sourceWidth, availableHeight / sourceHeight);

  return {
    left: 0,
    top: 0,
    width: Math.max(1, Math.round(sourceWidth * scale)),
    height: Math.max(1, Math.round(sourceHeight * scale)),
  };
}

/**
 * Place controls relative to the selected frame. Prefer the outside edge,
 * then use the opposite edge or an in-frame placement when the selection is
 * against a viewport boundary. The returned coordinates are all in the same
 * viewport coordinate system as the selection.
 */
export function annotationFrameLayout(
  selection: GeometryRect,
  viewport: { width: number; height: number },
  metrics = ANNOTATION_LAYOUT,
): AnnotationFrameLayout {
  const gap = metrics.controlGap;
  const toolbarOnRight = selection.left + selection.width + gap + metrics.toolbarWidth <= viewport.width;
  const toolbarOnLeft = selection.left >= gap + metrics.toolbarWidth;
  const toolbarLeft = toolbarOnRight
    ? selection.left + selection.width + gap
    : toolbarOnLeft
      ? selection.left - gap - metrics.toolbarWidth
      : Math.max(0, viewport.width - metrics.toolbarWidth);

  const toolbarHeight = Math.min(
    Math.max(1, viewport.height - gap * 2),
    Math.max(selection.height, metrics.toolbarHeight),
  );
  const toolbarTop = Math.min(
    Math.max(0, selection.top),
    Math.max(0, viewport.height - toolbarHeight),
  );
  const actionWidth = Math.max(selection.width, metrics.actionWidth);
  const centeredActionLeft = selection.left + (selection.width - actionWidth) / 2;
  const actionLeft = Math.min(
    Math.max(0, centeredActionLeft),
    Math.max(0, viewport.width - actionWidth),
  );
  const actionBelow = selection.top + selection.height + gap + metrics.actionHeight <= viewport.height;
  const actionTop = actionBelow
    ? selection.top + selection.height + gap
    : Math.max(0, selection.top - gap - metrics.actionHeight);

  return {
    frame: { ...selection },
    toolbar: {
      left: toolbarLeft,
      top: toolbarTop,
      width: metrics.toolbarWidth,
      height: toolbarHeight,
    },
    actions: {
      left: actionLeft,
      top: actionTop,
      width: actionWidth,
      height: metrics.actionHeight,
    },
  };
}

export function rectStyle(rect: GeometryRect): string {
  return `left: ${rect.left}px; top: ${rect.top}px; width: ${rect.width}px; height: ${rect.height}px;`;
}
