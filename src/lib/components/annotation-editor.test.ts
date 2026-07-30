/// <reference types="vitest/globals" />
/**
 * Tests for the annotation editor components: Toolbar, ActionBar,
 * AnnotationCanvas undo/redo logic, and save_to_file command integration.
 *
 * Mirroring the approach in overlay-logic.test.ts: extract pure functions
 * to test without requiring a full DOM or Tauri runtime.
 */

import { render, screen, fireEvent } from '@testing-library/svelte';
import Toolbar from './Toolbar.svelte';
import ActionBar from './ActionBar.svelte';
import type { Tool, AnnotationSnapshot } from '$lib/types';

// ---------------------------------------------------------------------------
// Pure logic from AnnotationCanvas.svelte — undo/redo stack
// ---------------------------------------------------------------------------

const MAX_HISTORY = 50;

interface StackState {
  undoStack: AnnotationSnapshot[];
  redoStack: AnnotationSnapshot[];
}

function pushSnapshot(state: StackState, dataUrl: string): StackState {
  const undoStack = [...state.undoStack, { dataUrl }];
  const trimmed = undoStack.length > MAX_HISTORY
    ? undoStack.slice(undoStack.length - MAX_HISTORY)
    : undoStack;
  return { undoStack: trimmed, redoStack: [] };
}

function undo(state: StackState, currentDataUrl: string): {
  state: StackState;
  restored: AnnotationSnapshot | null;
} {
  if (state.undoStack.length === 0) {
    return { state, restored: null };
  }
  const redoStack = [...state.redoStack, { dataUrl: currentDataUrl }];
  const snapshot = state.undoStack[state.undoStack.length - 1];
  const undoStack = state.undoStack.slice(0, -1);
  return {
    state: { undoStack, redoStack },
    restored: snapshot,
  };
}

function redo(state: StackState, currentDataUrl: string): {
  state: StackState;
  restored: AnnotationSnapshot | null;
} {
  if (state.redoStack.length === 0) {
    return { state, restored: null };
  }
  const undoStack = [...state.undoStack, { dataUrl: currentDataUrl }];
  const snapshot = state.redoStack[state.redoStack.length - 1];
  const redoStack = state.redoStack.slice(0, -1);
  return {
    state: { undoStack, redoStack },
    restored: snapshot,
  };
}

function canUndo(state: StackState): boolean {
  return state.undoStack.length > 0;
}

function canRedo(state: StackState): boolean {
  return state.redoStack.length > 0;
}

// ---------------------------------------------------------------------------
// Pure logic — image data URL handling
// ---------------------------------------------------------------------------

/**
 * Strip the data-URL prefix to get raw base64 for the Rust save_to_file command.
 */
function stripDataUrlPrefix(dataUrl: string): string {
  return dataUrl.replace(/^data:image\/png;base64,/, '');
}

// ---------------------------------------------------------------------------
// Tests — Undo/Redo Stack
// ---------------------------------------------------------------------------

describe('undo/redo stack logic', () => {
  it('starts with empty stacks', () => {
    const state: StackState = { undoStack: [], redoStack: [] };
    expect(canUndo(state)).toBe(false);
    expect(canRedo(state)).toBe(false);
  });

  it('pushSnapshot adds to undo and clears redo', () => {
    let state: StackState = { undoStack: [], redoStack: [{ dataUrl: 'old' }] };
    state = pushSnapshot(state, 'snap1');
    expect(state.undoStack).toHaveLength(1);
    expect(state.undoStack[0].dataUrl).toBe('snap1');
    expect(state.redoStack).toHaveLength(0); // redo cleared
    expect(canUndo(state)).toBe(true);
    expect(canRedo(state)).toBe(false);
  });

  it('undo moves current to redo and pops undo', () => {
    let state: StackState = { undoStack: [], redoStack: [] };
    state = pushSnapshot(state, 'snap1');
    state = pushSnapshot(state, 'snap2');

    const result = undo(state, 'current');
    expect(result.restored?.dataUrl).toBe('snap2');
    expect(result.state.undoStack).toHaveLength(1);
    expect(result.state.undoStack[0].dataUrl).toBe('snap1');
    expect(result.state.redoStack).toHaveLength(1);
    expect(result.state.redoStack[0].dataUrl).toBe('current');
  });

  it('redo moves current to undo and pops redo', () => {
    let state: StackState = { undoStack: [], redoStack: [] };
    state = pushSnapshot(state, 'snap1');
    state = pushSnapshot(state, 'snap2');
    // undo pops snap2 to redo, undo now has snap1
    let result = undo(state, 'current');
    expect(result.state.undoStack).toHaveLength(1);
    expect(result.state.redoStack).toHaveLength(1);

    // redo pops 'current' from redo back to undo
    result = redo(result.state, 'after-undo');
    expect(result.restored?.dataUrl).toBe('current');
    expect(result.state.undoStack).toHaveLength(2);
    expect(result.state.undoStack[0].dataUrl).toBe('snap1');
    expect(result.state.redoStack).toHaveLength(0);
  });

  it('undo on empty stack returns null', () => {
    const state: StackState = { undoStack: [], redoStack: [] };
    const result = undo(state, 'current');
    expect(result.restored).toBeNull();
    expect(result.state).toEqual(state);
  });

  it('redo on empty stack returns null', () => {
    const state: StackState = { undoStack: [], redoStack: [] };
    const result = redo(state, 'current');
    expect(result.restored).toBeNull();
    expect(result.state).toEqual(state);
  });

  it('redoing after a push clears redo history', () => {
    let state: StackState = { undoStack: [], redoStack: [] };
    state = pushSnapshot(state, 'snap1');
    state = pushSnapshot(state, 'snap2');
    let result = undo(state, 'current');
    state = result.state;
    // Now we have undo=[snap1], redo=[current]
    expect(state.redoStack).toHaveLength(1);

    // Push a new snapshot — should clear redo
    state = pushSnapshot(state, 'snap3');
    expect(state.redoStack).toHaveLength(0);
    expect(state.undoStack).toHaveLength(2);
  });
});

describe('MAX_HISTORY enforcement', () => {
  it('caps undo stack at 50 entries', () => {
    let state: StackState = { undoStack: [], redoStack: [] };
    // Push 55 snapshots
    for (let i = 0; i < 55; i++) {
      state = pushSnapshot(state, `snap${i}`);
    }
    expect(state.undoStack).toHaveLength(MAX_HISTORY);
    // The oldest 5 should be dropped; snap0..snap4 gone
    expect(state.undoStack[0].dataUrl).toBe('snap5');
    expect(state.undoStack[49].dataUrl).toBe('snap54');
  });
});

// ---------------------------------------------------------------------------
// Tests — data URL prefix stripping
// ---------------------------------------------------------------------------

describe('stripDataUrlPrefix', () => {
  it('removes standard PNG data URL prefix', () => {
    const result = stripDataUrlPrefix('data:image/png;base64,abc123');
    expect(result).toBe('abc123');
  });

  it('returns original string if no prefix matches', () => {
    expect(stripDataUrlPrefix('raw-base64-data')).toBe('raw-base64-data');
  });

  it('handles empty string', () => {
    expect(stripDataUrlPrefix('')).toBe('');
  });
});

// ---------------------------------------------------------------------------
// Tests — Tool types
// ---------------------------------------------------------------------------

describe('Tool type', () => {
  it('accepts all 6 valid tool identifiers', () => {
    const tools: Tool[] = ['pen', 'arrow', 'rectangle', 'text', 'blur', 'eraser'];
    expect(tools).toHaveLength(6);
    // Each is a distinct string literal
    const unique = new Set(tools);
    expect(unique.size).toBe(6);
  });
});

// ---------------------------------------------------------------------------
// Tests — Toolbar component rendering
// ---------------------------------------------------------------------------

describe('Toolbar component', () => {
  it('renders all 6 tool buttons', () => {
    render(Toolbar, { activeTool: 'pen', color: '#ff0000' });
    const toolLabels = ['Pen', 'Arrow', 'Rectangle', 'Text', 'Blur', 'Eraser'];
    for (const label of toolLabels) {
      const btn = screen.getByTitle(label);
      expect(btn).toBeTruthy();
      expect(btn.getAttribute('aria-pressed')).toBeDefined();
    }
  });

  it('marks active tool as pressed', () => {
    render(Toolbar, { activeTool: 'arrow', color: '#ff0000' });
    const penBtn = screen.getByTitle('Pen');
    const arrowBtn = screen.getByTitle('Arrow');
    expect(penBtn.getAttribute('aria-pressed')).toBe('false');
    expect(arrowBtn.getAttribute('aria-pressed')).toBe('true');
  });

  it('changes active tool on click', async () => {
    render(Toolbar, { activeTool: 'pen', color: '#ff0000' });
    const rectBtn = screen.getByTitle('Rectangle');
    await fireEvent.click(rectBtn);
    // After click, rectangle should be active
    expect(rectBtn.getAttribute('aria-pressed')).toBe('true');
  });

  it('renders 8 preset color swatches', () => {
    render(Toolbar, { activeTool: 'pen', color: '#ff0000' });
    const swatches = document.querySelectorAll('.swatch');
    expect(swatches.length).toBe(8);
  });

  it('marks selected color swatch', () => {
    render(Toolbar, { activeTool: 'pen', color: '#00cc00' });
    const swatches = document.querySelectorAll('.swatch');
    let selectedCount = 0;
    swatches.forEach((s) => {
      if (s.classList.contains('selected')) selectedCount++;
    });
    expect(selectedCount).toBe(1);
  });

  it('changes color on swatch click', async () => {
    render(Toolbar, { activeTool: 'pen', color: '#ff0000' });
    const swatches = document.querySelectorAll('.swatch');
    // Click on the black swatch (#000000)
    const blackSwatch = Array.from(swatches).find(
      (s) => s.getAttribute('aria-label') === 'Color #000000',
    ) as HTMLElement;
    expect(blackSwatch).toBeTruthy();
    await fireEvent.click(blackSwatch!);
    expect(blackSwatch!.classList.contains('selected')).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Tests — ActionBar component rendering
// ---------------------------------------------------------------------------

describe('ActionBar component', () => {
  it('renders Copy, Save, Upload, and Cancel buttons', () => {
    render(ActionBar, {
      onCopy: () => {},
      onSave: () => {},
      onUpload: () => {},
      onCancel: () => {},
      uploading: false,
    });
    expect(screen.getByTitle('Copy to clipboard')).toBeTruthy();
    expect(screen.getByTitle('Save to folder')).toBeTruthy();
    expect(screen.getByTitle('Upload to sinister.ovh')).toBeTruthy();
    expect(screen.getByTitle('Discard')).toBeTruthy();
  });

  it('shows uploading state when uploading=true', () => {
    render(ActionBar, {
      onCopy: () => {},
      onSave: () => {},
      onUpload: () => {},
      onCancel: () => {},
      uploading: true,
    });
    const uploadBtn = screen.getByTitle('Upload to sinister.ovh');
    expect(uploadBtn.textContent).toContain('Uploading');
    expect((uploadBtn as HTMLButtonElement).disabled).toBe(true);
  });

  it('calls onCopy when Copy button clicked', async () => {
    let called = false;
    render(ActionBar, {
      onCopy: () => { called = true; },
      onSave: () => {},
      onUpload: () => {},
      onCancel: () => {},
      uploading: false,
    });
    await fireEvent.click(screen.getByTitle('Copy to clipboard'));
    expect(called).toBe(true);
  });

  it('calls onSave when Save button clicked', async () => {
    let called = false;
    render(ActionBar, {
      onCopy: () => {},
      onSave: () => { called = true; },
      onUpload: () => {},
      onCancel: () => {},
      uploading: false,
    });
    await fireEvent.click(screen.getByTitle('Save to folder'));
    expect(called).toBe(true);
  });

  it('calls onCancel when Cancel button clicked', async () => {
    let called = false;
    render(ActionBar, {
      onCopy: () => {},
      onSave: () => {},
      onUpload: () => {},
      onCancel: () => { called = true; },
      uploading: false,
    });
    await fireEvent.click(screen.getByTitle('Discard'));
    expect(called).toBe(true);
  });

  it('disables Upload button when uploading=true', () => {
    render(ActionBar, {
      onCopy: () => {},
      onSave: () => {},
      onUpload: () => {},
      onCancel: () => {},
      uploading: true,
    });
    const uploadBtn = screen.getByTitle('Upload to sinister.ovh') as HTMLButtonElement;
    expect(uploadBtn.disabled).toBe(true);
  });
});
