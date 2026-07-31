import type { SelectionRect } from './selection-geometry';

export interface CapturedImage {
  data: string;
  width: number;
  height: number;
}

export type AreaEditorPhase = 'selecting' | 'annotating' | 'committing' | 'cancelled';

export interface AreaEditorState {
  phase: AreaEditorPhase;
  selection: SelectionRect | null;
}

/** Release creates the visible editor frame; it never captures pixels. */
export function areaSelectionReleased(selection: SelectionRect): AreaEditorState {
  return { phase: 'annotating', selection: { ...selection } };
}

/** Save/copy/upload are the only transition that requests a native crop. */
export function areaExportRequested(state: AreaEditorState): AreaEditorState {
  if (state.phase !== 'annotating' || !state.selection) return state;
  return { ...state, phase: 'committing' };
}

export function areaExportFinished(state: AreaEditorState): AreaEditorState {
  return state.phase === 'committing' ? { ...state, phase: 'annotating' } : state;
}

export function areaSelectionCancelled(): AreaEditorState {
  return { phase: 'cancelled', selection: null };
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
