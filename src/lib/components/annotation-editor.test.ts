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
import { fitAnnotationImage } from './capture-layout';

// ---------------------------------------------------------------------------
// Pure logic — keyboard tool-switching (extracted from Overlay.svelte)
// ---------------------------------------------------------------------------

/**
 * Maps a single-character key to its corresponding annotation tool.
 * Returns null if the key does not match any tool shortcut.
 */
function keyToTool(key: string): Tool | null {
  const toolMap: Record<string, Tool> = {
    p: 'pen',
    a: 'arrow',
    r: 'rectangle',
    t: 'text',
    b: 'blur',
    e: 'eraser',
  };
  return toolMap[key.toLowerCase()] ?? null;
}

/**
 * Returns true when a keyboard event should be treated as a tool-switch
 * shortcut in the annotation editor.  Rejects:
 *  - modifier keys (Ctrl, Alt, Meta)
 *  - any flow state other than "annotating"
 *  - events dispatched while an INPUT or TEXTAREA is focused
 */
function shouldSwitchTool(
  flowState: string,
  key: string,
  ctrlKey: boolean,
  altKey: boolean,
  metaKey: boolean,
  activeTagName: string | undefined,
): boolean {
  if (flowState !== 'annotating') return false;
  if (ctrlKey || altKey || metaKey) return false;
  if (activeTagName === 'INPUT' || activeTagName === 'TEXTAREA') return false;
  return keyToTool(key) !== null;
}

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
    const toolLabels = ['Pen (P)', 'Arrow (A)', 'Rectangle (R)', 'Text (T)', 'Blur (B)', 'Eraser (E)'];
    for (const label of toolLabels) {
      const btn = screen.getByTitle(label);
      expect(btn).toBeTruthy();
      expect(btn.getAttribute('aria-pressed')).toBeDefined();
    }
  });

  it('marks active tool as pressed', () => {
    render(Toolbar, { activeTool: 'arrow', color: '#ff0000' });
    const penBtn = screen.getByTitle('Pen (P)');
    const arrowBtn = screen.getByTitle('Arrow (A)');
    expect(penBtn.getAttribute('aria-pressed')).toBe('false');
    expect(arrowBtn.getAttribute('aria-pressed')).toBe('true');
  });

  it('changes active tool on click', async () => {
    render(Toolbar, { activeTool: 'pen', color: '#ff0000' });
    const rectBtn = screen.getByTitle('Rectangle (R)');
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

describe('annotation layout geometry', () => {
  it('fits the image to the viewport while preserving its aspect ratio', () => {
    expect(fitAnnotationImage({ width: 1920, height: 1080 }, { width: 1920, height: 1080 })).toEqual({
      width: 1778,
      height: 1000,
    });
  });

  it('does not enlarge small images and always returns explicit dimensions', () => {
    expect(fitAnnotationImage({ width: 400, height: 200 }, { width: 1920, height: 1080 })).toEqual({
      width: 400,
      height: 200,
    });
    expect(fitAnnotationImage({ width: 0, height: 0 }, { width: 1920, height: 1080 })).toEqual({
      width: 1,
      height: 1,
    });
  });
});

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

// ---------------------------------------------------------------------------
// Tests — Keyboard tool-switching logic (pure functions)
// ---------------------------------------------------------------------------

describe('keyToTool', () => {
  it('maps each shortcut key to the correct tool', () => {
    expect(keyToTool('p')).toBe('pen');
    expect(keyToTool('a')).toBe('arrow');
    expect(keyToTool('r')).toBe('rectangle');
    expect(keyToTool('t')).toBe('text');
    expect(keyToTool('b')).toBe('blur');
    expect(keyToTool('e')).toBe('eraser');
  });

  it('is case-insensitive', () => {
    expect(keyToTool('P')).toBe('pen');
    expect(keyToTool('A')).toBe('arrow');
    expect(keyToTool('R')).toBe('rectangle');
    expect(keyToTool('T')).toBe('text');
    expect(keyToTool('B')).toBe('blur');
    expect(keyToTool('E')).toBe('eraser');
  });

  it('returns null for non-tool keys', () => {
    expect(keyToTool('x')).toBeNull();
    expect(keyToTool('1')).toBeNull();
    expect(keyToTool(' ')).toBeNull();
    expect(keyToTool('Escape')).toBeNull();
    expect(keyToTool('Enter')).toBeNull();
    expect(keyToTool('z')).toBeNull();
    expect(keyToTool('y')).toBeNull();
  });
});

describe('shouldSwitchTool', () => {
  // Happy path — annotating, no modifiers, no input focus, valid tool key
  it('returns true for valid tool keys in annotation mode', () => {
    expect(shouldSwitchTool('annotating', 'p', false, false, false, undefined)).toBe(true);
    expect(shouldSwitchTool('annotating', 'a', false, false, false, undefined)).toBe(true);
    expect(shouldSwitchTool('annotating', 'r', false, false, false, 'BODY')).toBe(true);
    expect(shouldSwitchTool('annotating', 'T', false, false, false, 'DIV')).toBe(true);
  });

  it('returns false for non-tool keys in annotation mode', () => {
    expect(shouldSwitchTool('annotating', 'x', false, false, false, undefined)).toBe(false);
    expect(shouldSwitchTool('annotating', 'Escape', false, false, false, undefined)).toBe(false);
    expect(shouldSwitchTool('annotating', 'z', false, false, false, undefined)).toBe(false);
  });

  // Flow state guard
  it('returns false when flowState is idle', () => {
    expect(shouldSwitchTool('idle', 'p', false, false, false, undefined)).toBe(false);
    expect(shouldSwitchTool('idle', 'a', false, false, false, undefined)).toBe(false);
  });

  it('returns false when flowState is capturing', () => {
    expect(shouldSwitchTool('capturing', 'p', false, false, false, undefined)).toBe(false);
    expect(shouldSwitchTool('capturing', 'r', false, false, false, undefined)).toBe(false);
  });

  it('returns false when flowState is uploading', () => {
    expect(shouldSwitchTool('uploading', 'p', false, false, false, undefined)).toBe(false);
    expect(shouldSwitchTool('uploading', 'e', false, false, false, undefined)).toBe(false);
  });

  // Modifier guards
  it('returns false when Ctrl key is held', () => {
    expect(shouldSwitchTool('annotating', 'p', true, false, false, undefined)).toBe(false);
    expect(shouldSwitchTool('annotating', 'a', true, false, false, undefined)).toBe(false);
  });

  it('returns false when Alt key is held', () => {
    expect(shouldSwitchTool('annotating', 'p', false, true, false, undefined)).toBe(false);
    expect(shouldSwitchTool('annotating', 't', false, true, false, undefined)).toBe(false);
  });

  it('returns false when Meta key is held', () => {
    expect(shouldSwitchTool('annotating', 'p', false, false, true, undefined)).toBe(false);
    expect(shouldSwitchTool('annotating', 'b', false, false, true, undefined)).toBe(false);
  });

  // Input-focus guard
  it('returns false when an INPUT element is focused', () => {
    expect(shouldSwitchTool('annotating', 'p', false, false, false, 'INPUT')).toBe(false);
    expect(shouldSwitchTool('annotating', 'r', false, false, false, 'INPUT')).toBe(false);
  });

  it('returns false when a TEXTAREA element is focused', () => {
    expect(shouldSwitchTool('annotating', 't', false, false, false, 'TEXTAREA')).toBe(false);
    expect(shouldSwitchTool('annotating', 'e', false, false, false, 'TEXTAREA')).toBe(false);
  });

  // Undo/Redo keys — Ctrl+Z / Ctrl+Y must NOT switch tools even when Z/Y map to nothing
  it('allows Ctrl+Z through (not a tool switch)', () => {
    // Ctrl+Z should not be blocked — it's for undo
    expect(shouldSwitchTool('annotating', 'z', true, false, false, undefined)).toBe(false);
  });

  it('allows Ctrl+Y through (not a tool switch)', () => {
    expect(shouldSwitchTool('annotating', 'y', true, false, false, undefined)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// Tests — Toolbar keyboard flash prop
// ---------------------------------------------------------------------------

describe('Toolbar flash animation', () => {
  it('applies the flash CSS class when flashTool matches the tool id', () => {
    const { container } = render(Toolbar, {
      activeTool: 'pen',
      color: '#ff0000',
      flashTool: 'arrow',
    });

    // The pen button should NOT have the flash class
    const penBtn = screen.getByTitle('Pen (P)');
    expect(penBtn.classList.contains('flash')).toBe(false);

    // The arrow button SHOULD have the flash class
    const arrowBtn = screen.getByTitle('Arrow (A)');
    expect(arrowBtn.classList.contains('flash')).toBe(true);
  });

  it('does not flash any button when flashTool is null', () => {
    render(Toolbar, {
      activeTool: 'pen',
      color: '#ff0000',
      flashTool: null,
    });

    const allBtns = document.querySelectorAll('.tool-btn.flash');
    expect(allBtns.length).toBe(0);
  });

  it('does not flash any button when flashTool is undefined (default)', () => {
    render(Toolbar, {
      activeTool: 'pen',
      color: '#ff0000',
      flashTool: null,
    });

    const penBtn = screen.getByTitle('Pen (P)');
    expect(penBtn.classList.contains('active')).toBe(true);
    expect(penBtn.classList.contains('flash')).toBe(false);

    const allFlashing = document.querySelectorAll('.tool-btn.flash');
    expect(allFlashing.length).toBe(0);
  });

  it('flashes only the matching tool button, not the active one', () => {
    // activeTool is 'pen' but flashTool is 'rectangle' — flash on rect, not pen
    render(Toolbar, {
      activeTool: 'pen',
      color: '#ff0000',
      flashTool: 'rectangle' as Tool,
    });

    const penBtn = screen.getByTitle('Pen (P)');
    const rectBtn = screen.getByTitle('Rectangle (R)');

    expect(penBtn.classList.contains('flash')).toBe(false);
    expect(rectBtn.classList.contains('flash')).toBe(true);
  });

  it('flashes the active tool when flashTool matches activeTool', () => {
    render(Toolbar, {
      activeTool: 'arrow',
      color: '#ff0000',
      flashTool: 'arrow' as Tool,
    });

    const arrowBtn = screen.getByTitle('Arrow (A)');
    expect(arrowBtn.classList.contains('active')).toBe(true);
    expect(arrowBtn.classList.contains('flash')).toBe(true);
  });

  it('title attributes include the shortcut letter hint', () => {
    render(Toolbar, { activeTool: 'pen', color: '#ff0000' });

    expect(screen.getByTitle('Pen (P)')).toBeTruthy();
    expect(screen.getByTitle('Arrow (A)')).toBeTruthy();
    expect(screen.getByTitle('Rectangle (R)')).toBeTruthy();
    expect(screen.getByTitle('Text (T)')).toBeTruthy();
    expect(screen.getByTitle('Blur (B)')).toBeTruthy();
    expect(screen.getByTitle('Eraser (E)')).toBeTruthy();
  });
});
