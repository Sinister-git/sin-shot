/// <reference types="vitest/globals" />
/**
 * Tests for the Overlay component's core coordinate-conversion and
 * selection-rectangle logic.
 *
 * These pure functions mirror the logic in Overlay.svelte without
 * requiring a DOM or Tauri runtime.
 */

import {
  annotationTransition,
  areaExportFinished,
  areaExportRequested,
  areaSelectionCancelled,
  areaSelectionReleased,
} from "./capture-flow";
import { annotationFrameLayout } from './annotation-geometry';
import {
  moveSelection,
  pointInSelection,
  selectionFromPointerRelease,
} from "./selection-geometry";
import {
  findMonitorAtClient,
  monitorToCssRect,
  selectionToDesktopCoords as nativeSelectionToDesktopCoords,
  type PhysicalMonitor,
} from "./monitor-geometry";

// ---------------------------------------------------------------------------
// Types (mirror Overlay.svelte)
// ---------------------------------------------------------------------------

interface Monitor {
  name: string;
  width: number;
  height: number;
  x: number;
  y: number;
  is_primary: boolean;
  scale_factor: number;
}

interface Point {
  x: number;
  y: number;
}

interface SelectionRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

// ---------------------------------------------------------------------------
// Pure logic functions extracted from Overlay.svelte
// ---------------------------------------------------------------------------

/**
 * Compute the window offset (top-left of the bounding box of all monitors)
 * and convert monitor coords from physical pixels to CSS pixels.
 */
function normalizeMonitors(
  monitors: Monitor[],
  dpr: number,
): { monitors: Monitor[]; windowOffset: Point; dpr: number } {
  let minX = Infinity;
  let minY = Infinity;
  for (const m of monitors) {
    if (m.x < minX) minX = m.x;
    if (m.y < minY) minY = m.y;
  }
  const windowOffset = { x: minX / dpr, y: minY / dpr };
  const normalized = monitors.map((m) => ({
    ...m,
    x: m.x / dpr,
    y: m.y / dpr,
    width: m.width / dpr,
    height: m.height / dpr,
  }));
  return { monitors: normalized, windowOffset, dpr };
}

/**
 * Find which monitor the cursor is on.
 * clientX/clientY are window-relative CSS pixels.
 */
function findMonitor(
  clientX: number,
  clientY: number,
  monitors: Monitor[],
  windowOffset: Point,
): number {
  const sx = clientX + windowOffset.x;
  const sy = clientY + windowOffset.y;
  for (let i = 0; i < monitors.length; i++) {
    const m = monitors[i];
    if (sx >= m.x && sx < m.x + m.width && sy >= m.y && sy < m.y + m.height) {
      return i;
    }
  }
  return -1;
}

/**
 * Compute the selection rectangle from start and current mouse positions.
 */
function computeSelectionRect(start: Point, current: Point): SelectionRect {
  return {
    left: Math.min(start.x, current.x),
    top: Math.min(start.y, current.y),
    width: Math.abs(current.x - start.x),
    height: Math.abs(current.y - start.y),
  };
}

/**
 * Convert a window-relative selection rect (CSS pixels) to absolute
 * physical desktop coordinates for the capture backend.
 */
function legacySelectionToDesktopCoords(
  sel: SelectionRect,
  windowOffset: Point,
  dpr: number,
): { x: number; y: number; width: number; height: number } {
  return {
    x: Math.round((sel.left + windowOffset.x) * dpr),
    y: Math.round((sel.top + windowOffset.y) * dpr),
    width: Math.round(sel.width * dpr),
    height: Math.round(sel.height * dpr),
  };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("native monitor geometry", () => {
  const monitors: PhysicalMonitor[] = [
    { name: "Left", width: 2560, height: 1440, x: -2560, y: 120, is_primary: false, scale_factor: 1.25 },
    { name: "Primary", width: 3840, height: 2160, x: 0, y: 0, is_primary: true, scale_factor: 2 },
    { name: "Right", width: 1920, height: 1080, x: 3840, y: 420, is_primary: false, scale_factor: 1 },
  ];

  it("renders physical monitor bounds using the placed overlay scale", () => {
    expect(monitorToCssRect(monitors[0], { x: -2560, y: 0 }, 2)).toEqual({
      left: 0, top: 60, width: 1280, height: 720,
    });
    expect(monitorToCssRect(monitors[2], { x: -2560, y: 0 }, 2)).toEqual({
      left: 3200, top: 210, width: 960, height: 540,
    });
  });

  it("finds monitors across negative origins, vertical offsets, and gaps", () => {
    const origin = { x: -2560, y: 0 };
    expect(findMonitorAtClient(100, 100, monitors, origin, 2)).toBe(0);
    expect(findMonitorAtClient(100, 50, monitors, origin, 2)).toBe(-1); // vertical gap
    expect(findMonitorAtClient(3000, 250, monitors, origin, 2)).toBe(1);
    expect(findMonitorAtClient(3400, 300, monitors, origin, 2)).toBe(2);
  });

  it("converts CSS selection to physical coordinates without per-monitor scale mixing", () => {
    expect(nativeSelectionToDesktopCoords(
      { left: 1280, top: 210, width: 960, height: 540 },
      { x: -2560, y: 0 },
      2,
    )).toEqual({ x: 0, y: 420, width: 1920, height: 1080 });
  });
});

describe("normalizeMonitors", () => {
  it("handles a single 1920×1080 monitor at (0,0) with DPR=1", () => {
    const monitors: Monitor[] = [
      { name: "Main", width: 1920, height: 1080, x: 0, y: 0, is_primary: true, scale_factor: 1 },
    ];
    const result = normalizeMonitors(monitors, 1);
    expect(result.windowOffset).toEqual({ x: 0, y: 0 });
    expect(result.monitors[0].x).toBe(0);
    expect(result.monitors[0].width).toBe(1920);
  });

  it("handles dual monitors side-by-side", () => {
    const monitors: Monitor[] = [
      { name: "Left", width: 1920, height: 1080, x: 0, y: 0, is_primary: true, scale_factor: 1 },
      {
        name: "Right",
        width: 1920,
        height: 1080,
        x: 1920,
        y: 0,
        is_primary: false,
        scale_factor: 1,
      },
    ];
    const result = normalizeMonitors(monitors, 1);
    expect(result.windowOffset).toEqual({ x: 0, y: 0 });
    expect(result.monitors[0].x).toBe(0);
    expect(result.monitors[1].x).toBe(1920);
  });

  it("handles negative monitor offset (secondary left of primary)", () => {
    const monitors: Monitor[] = [
      {
        name: "Left",
        width: 1920,
        height: 1080,
        x: -1920,
        y: 0,
        is_primary: false,
        scale_factor: 1,
      },
      {
        name: "Primary",
        width: 2560,
        height: 1440,
        x: 0,
        y: 0,
        is_primary: true,
        scale_factor: 1,
      },
    ];
    const result = normalizeMonitors(monitors, 1);
    // Window offset should be the leftmost monitor's x coord.
    expect(result.windowOffset).toEqual({ x: -1920, y: 0 });
    // After normalization, the left monitor x should be in CSS coords.
    expect(result.monitors[0].x).toBe(-1920);
    expect(result.monitors[1].x).toBe(0);
  });

  it("handles DPR=2 (Retina/HiDPI)", () => {
    const monitors: Monitor[] = [
      { name: "4K", width: 3840, height: 2160, x: 0, y: 0, is_primary: true, scale_factor: 2 }
    ];
    const result = normalizeMonitors(monitors, 2);
    expect(result.windowOffset).toEqual({ x: 0, y: 0 });
    // Physical 3840 / DPR 2 = 1920 CSS pixels.
    expect(result.monitors[0].width).toBe(1920);
    expect(result.monitors[0].height).toBe(1080);
  });
});

describe("findMonitor", () => {
  const monitors: Monitor[] = [
    {
      name: "Primary",
      width: 1920,
      height: 1080,
      x: 0,
      y: 0,
      is_primary: true,
      scale_factor: 1,
    },
    {
      name: "Right",
      width: 1920,
      height: 1080,
      x: 1920,
      y: 0,
      is_primary: false,
      scale_factor: 1,
    },
  ];

  it("finds cursor on primary monitor", () => {
    const idx = findMonitor(500, 300, monitors, { x: 0, y: 0 });
    expect(idx).toBe(0);
  });

  it("finds cursor on secondary monitor", () => {
    const idx = findMonitor(2500, 500, monitors, { x: 0, y: 0 });
    expect(idx).toBe(1);
  });

  it("returns -1 when cursor is outside all monitors", () => {
    const idx = findMonitor(5000, 5000, monitors, { x: 0, y: 0 });
    expect(idx).toBe(-1);
  });

  it("accounts for window offset with negative monitor x", () => {
    const negMonitors: Monitor[] = [
      {
        name: "Left",
        width: 1920,
        height: 1080,
        x: -1920,
        y: 0,
        is_primary: false,
        scale_factor: 1,
      },
      {
        name: "Right",
        width: 2560,
        height: 1440,
        x: 0,
        y: 0,
        is_primary: true,
        scale_factor: 1,
      },
    ];
    const offset = { x: -1920, y: 0 };
    // Cursor at window-relative (500, 300) => screen (-1420, 300) => should be on Left monitor
    expect(findMonitor(500, 300, negMonitors, offset)).toBe(0);
    // Cursor at window-relative (2500, 500) => screen (580, 500) => should be on Right monitor
    expect(findMonitor(2500, 500, negMonitors, offset)).toBe(1);
  });
});

describe("computeSelectionRect", () => {
  it("creates rect from top-left to bottom-right drag", () => {
    const rect = computeSelectionRect({ x: 100, y: 100 }, { x: 500, y: 400 });
    expect(rect).toEqual({ left: 100, top: 100, width: 400, height: 300 });
  });

  it("handles bottom-right to top-left drag (negative direction)", () => {
    const rect = computeSelectionRect({ x: 500, y: 400 }, { x: 100, y: 100 });
    expect(rect).toEqual({ left: 100, top: 100, width: 400, height: 300 });
  });

  it("handles top-right to bottom-left drag", () => {
    const rect = computeSelectionRect({ x: 500, y: 100 }, { x: 100, y: 400 });
    expect(rect).toEqual({ left: 100, top: 100, width: 400, height: 300 });
  });

  it("returns zero-size rect for click (no drag)", () => {
    const rect = computeSelectionRect({ x: 100, y: 100 }, { x: 100, y: 100 });
    expect(rect).toEqual({ left: 100, top: 100, width: 0, height: 0 });
  });
});

describe("selectionToDesktopCoords", () => {
  it("converts CSS-pixel selection to physical coords at DPR=1", () => {
    const sel: SelectionRect = { left: 100, top: 50, width: 400, height: 300 };
    const result = legacySelectionToDesktopCoords(sel, { x: 0, y: 0 }, 1);
    expect(result).toEqual({ x: 100, y: 50, width: 400, height: 300 });
  });

  it("converts with DPR=2", () => {
    const sel: SelectionRect = { left: 100, top: 50, width: 400, height: 300 };
    const result = legacySelectionToDesktopCoords(sel, { x: 0, y: 0 }, 2);
    expect(result).toEqual({ x: 200, y: 100, width: 800, height: 600 });
  });

  it("accounts for window offset (negative monitor origin)", () => {
    // Monitor at x=-1920 in screen coords, windowOffset.x = -1920.
    // Selection at window-relative (200, 100) => screen (-1720, 100).
    const sel: SelectionRect = { left: 200, top: 100, width: 800, height: 600 };
    const result = legacySelectionToDesktopCoords(sel, { x: -1920, y: 0 }, 1);
    expect(result).toEqual({ x: -1720, y: 100, width: 800, height: 600 });
  });

  it("rounds fractional coordinates", () => {
    const sel: SelectionRect = {
      left: 100.7,
      top: 50.2,
      width: 400.4,
      height: 299.8,
    };
    const result = legacySelectionToDesktopCoords(sel, { x: 0, y: 0 }, 1.5);
    expect(result.x).toBe(151); // (100.7 + 0) * 1.5 = 151.05 -> 151
    expect(result.y).toBe(75); // (50.2 + 0) * 1.5 = 75.3 -> 75
    expect(result.width).toBe(601); // 400.4 * 1.5 = 600.6 -> 601
    expect(result.height).toBe(450); // 299.8 * 1.5 = 449.7 -> 450
  });
});

describe("movable selection geometry", () => {
  const selection = { left: 100, top: 80, width: 300, height: 200 };

  it("recognizes points inside the completed selection", () => {
    expect(pointInSelection({ x: 100, y: 80 }, selection)).toBe(true);
    expect(pointInSelection({ x: 250, y: 180 }, selection)).toBe(true);
    expect(pointInSelection({ x: 401, y: 180 }, selection)).toBe(false);
    expect(pointInSelection({ x: 250, y: 281 }, selection)).toBe(false);
  });

  it("moves a selection by the pointer delta", () => {
    expect(
      moveSelection(selection, { x: 25, y: -30 }, { width: 1000, height: 800 }),
    ).toEqual({
      left: 125,
      top: 50,
      width: 300,
      height: 200,
    });
  });

  it("clamps movement to every overlay edge", () => {
    expect(
      moveSelection(
        selection,
        { x: -500, y: -500 },
        { width: 1000, height: 800 },
      ),
    ).toMatchObject({
      left: 0,
      top: 0,
    });
    expect(
      moveSelection(
        selection,
        { x: 1000, y: 1000 },
        { width: 1000, height: 800 },
      ),
    ).toMatchObject({
      left: 700,
      top: 600,
    });
  });

  it("keeps a selection at the origin when it is larger than the overlay", () => {
    expect(
      moveSelection(
        { left: 0, top: 0, width: 1200, height: 900 },
        { x: 50, y: 50 },
        { width: 1000, height: 800 },
      ),
    ).toEqual({
      left: 0,
      top: 0,
      width: 1200,
      height: 900,
    });
  });
});

describe("selection persistence after mouseup (minimum size check)", () => {
  it("returns a completed selection on valid mouse release", () => {
    expect(
      selectionFromPointerRelease({ x: 500, y: 400 }, { x: 100, y: 100 }),
    ).toEqual({ left: 100, top: 100, width: 400, height: 300 });
  });

  it("rejects selections with any dimension smaller than 2 pixels", () => {
    expect(selectionFromPointerRelease({ x: 0, y: 0 }, { x: 0, y: 0 })).toBeNull();
    expect(selectionFromPointerRelease({ x: 0, y: 0 }, { x: 1, y: 1 })).toBeNull();
    expect(selectionFromPointerRelease({ x: 0, y: 0 }, { x: 1, y: 3 })).toBeNull();
    expect(selectionFromPointerRelease({ x: 0, y: 0 }, { x: 3, y: 1 })).toBeNull();
    expect(selectionFromPointerRelease({ x: 0, y: 0 }, { x: 2, y: 2 })).not.toBeNull();
  });
});

describe('area editor lifecycle', () => {
  const selection = { left: 40, top: 30, width: 400, height: 300 };

  it('shows the selection frame after release without a capture result', () => {
    expect(areaSelectionReleased(selection)).toEqual({ phase: 'annotating', selection });
  });

  it('captures only after an export action and preserves the frame', () => {
    const committing = areaExportRequested(areaSelectionReleased(selection));
    expect(committing.phase).toBe('committing');
    expect(committing.selection).toEqual(selection);
    expect(areaExportFinished(committing).phase).toBe('annotating');
  });

  it('cancels the frame without exporting', () => {
    expect(areaSelectionCancelled()).toEqual({ phase: 'cancelled', selection: null });
  });
});

describe("capture-to-tools transition", () => {
  const image = { data: "base64-png", width: 1920, height: 1080 };

  it("keeps a valid release as a visible editable frame before final capture", () => {
    const selection = selectionFromPointerRelease({ x: 440, y: 330 }, { x: 40, y: 30 });
    expect(selection).toEqual({ left: 40, top: 30, width: 400, height: 300 });
    const layout = annotationFrameLayout(selection!, { width: 1200, height: 800 });
    expect(layout.frame).toEqual(selection);
    expect(layout.actions.left).toBe(selection!.left);
    expect(layout.toolbar.top).toBe(selection!.top);
  });

  it("uses the captured image transition for full-screen capture only", () => {
    expect(annotationTransition(image)).toEqual({
      mode: null,
      flowState: "annotating",
      capturedImage: image,
    });
  });

  it("uses the same annotation path for full-screen capture", () => {
    const transition = annotationTransition(image);
    expect(transition.flowState).toBe("annotating");
    expect(transition.mode).toBeNull();
    expect(transition.capturedImage).toBe(image);
  });
});

// ---------------------------------------------------------------------------
// Dynamic hotkey combo comparison (mirrors Overlay.svelte event handler)
// ---------------------------------------------------------------------------

describe("dynamic hotkey combo comparison", () => {
  /**
   * Pure function that mirrors the hotkey-pressed event handler logic
   * in Overlay.svelte. Instead of hardcoding 'Ctrl+Shift+1'/'Ctrl+Shift+2',
   * it compares against dynamically-loaded combo strings.
   */
  function resolveCaptureMode(
    combo: string,
    hotkeyFull: string,
    hotkeyArea: string,
  ): "full-monitor" | "area-select" | null {
    if (combo === hotkeyFull) return "full-monitor";
    if (combo === hotkeyArea) return "area-select";
    return null;
  }

  it("matches default hotkeys", () => {
    expect(
      resolveCaptureMode("Ctrl+Shift+1", "Ctrl+Shift+1", "Ctrl+Shift+2"),
    ).toBe("full-monitor");
    expect(
      resolveCaptureMode("Ctrl+Shift+2", "Ctrl+Shift+1", "Ctrl+Shift+2"),
    ).toBe("area-select");
  });

  it("matches custom hotkeys loaded from settings", () => {
    // Simulates settings returning Ctrl+Shift+F for full, Ctrl+Shift+A for area
    const hotkeyFull = "Ctrl+Shift+F";
    const hotkeyArea = "Ctrl+Shift+A";

    expect(resolveCaptureMode("Ctrl+Shift+F", hotkeyFull, hotkeyArea)).toBe(
      "full-monitor",
    );
    expect(resolveCaptureMode("Ctrl+Shift+A", hotkeyFull, hotkeyArea)).toBe(
      "area-select",
    );
    expect(
      resolveCaptureMode("Ctrl+Shift+1", hotkeyFull, hotkeyArea),
    ).toBeNull();
    expect(
      resolveCaptureMode("Ctrl+Shift+2", hotkeyFull, hotkeyArea),
    ).toBeNull();
  });

  it("returns null for unrecognized combos", () => {
    expect(
      resolveCaptureMode("Alt+F4", "Ctrl+Shift+1", "Ctrl+Shift+2"),
    ).toBeNull();
    expect(
      resolveCaptureMode("Ctrl+C", "Ctrl+Shift+1", "Ctrl+Shift+2"),
    ).toBeNull();
  });

  it("updates combos dynamically (simulating settings-changed event)", () => {
    // Initial state
    let hotkeyFull = "Ctrl+Shift+1";
    let hotkeyArea = "Ctrl+Shift+2";

    expect(resolveCaptureMode("Ctrl+Shift+1", hotkeyFull, hotkeyArea)).toBe(
      "full-monitor",
    );

    // Simulate receiving a settings-changed event
    hotkeyFull = "Ctrl+Shift+F";
    hotkeyArea = "Ctrl+Shift+A";

    // Old combo should no longer match
    expect(
      resolveCaptureMode("Ctrl+Shift+1", hotkeyFull, hotkeyArea),
    ).toBeNull();
    // New combo should match
    expect(resolveCaptureMode("Ctrl+Shift+F", hotkeyFull, hotkeyArea)).toBe(
      "full-monitor",
    );
    expect(resolveCaptureMode("Ctrl+Shift+A", hotkeyFull, hotkeyArea)).toBe(
      "area-select",
    );
  });
});
