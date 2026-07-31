export interface ImageSize {
  width: number;
  height: number;
}

/**
 * Fit an image inside the annotation overlay while reserving space for the
 * controls. The returned dimensions are explicit so the canvas's absolute
 * child cannot collapse its container.
 */
export function fitAnnotationImage(
  image: ImageSize,
  viewport: ImageSize,
  reservedWidth = 120,
  reservedHeight = 80,
): ImageSize {
  if (image.width <= 0 || image.height <= 0) return { width: 1, height: 1 };

  const maxWidth = Math.max(1, viewport.width - reservedWidth);
  const maxHeight = Math.max(1, viewport.height - reservedHeight);
  const scale = Math.min(1, maxWidth / image.width, maxHeight / image.height);

  return {
    width: Math.max(1, Math.round(image.width * scale)),
    height: Math.max(1, Math.round(image.height * scale)),
  };
}
