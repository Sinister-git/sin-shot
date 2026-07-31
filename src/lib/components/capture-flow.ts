export interface CapturedImage {
  data: string;
  width: number;
  height: number;
}

export interface AnnotationTransition {
  mode: null;
  flowState: "annotating";
  capturedImage: CapturedImage;
}

/** State shared by full-screen and area capture once an image is available. */
export function annotationTransition(image: CapturedImage): AnnotationTransition {
  return {
    mode: null,
    flowState: "annotating",
    capturedImage: image,
  };
}
