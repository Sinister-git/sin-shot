/** Annotation tool identifiers. */
export type Tool = 'pen' | 'arrow' | 'rectangle' | 'text' | 'blur' | 'eraser';

/** Capture result received from the Rust backend. */
export interface CaptureResult {
  width: number;
  height: number;
  data: string; // base64-encoded RGBA pixels
}

/** A single entry in the annotation undo stack. */
export interface AnnotationSnapshot {
  /** data URL of the annotation canvas at this point in history */
  dataUrl: string;
}
