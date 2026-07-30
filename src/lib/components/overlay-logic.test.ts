/// <reference types="vitest/globals" />
/**
 * Tests for the Overlay component's core coordinate-conversion and
 * selection-rectangle logic.
 *
 * These pure functions mirror the logic in Overlay.svelte without
 * requiring a DOM or Tauri runtime.
 */

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
function selectionToDesktopCoords(
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

describe('normalizeMonitors', () => {
  it('handles a single 1920×1080 monitor at (0,0) with DPR=1', () => {
    const monitors: Monitor[] = [
      { name: 'Main', width: 1920, height: 1080, x: 0, y: 0, is_primary: true },
    ];
    const result = normalizeMonitors(monitors, 1);
    expect(result.windowOffset).toEqual({ x: 0, y: 0 });
    expect(result.monitors[0].x).toBe(0);
    expect(result.monitors[0].width).toBe(1920);
  });

  it('handles dual monitors side-by-side', () => {
    const monitors: Monitor[] = [
      { name: 'Left', width: 1920, height: 1080, x: 0, y: 0, is_primary: true },
      { name: 'Right', width: 1920, height: 1080, x: 1920, y: 0, is_primary: false },
    ];
    const result = normalizeMonitors(monitors, 1);
    expect(result.windowOffset).toEqual({ x: 0, y: 0 });
    expect(result.monitors[0].x).toBe(0);
    expect(result.monitors[1].x).toBe(1920);
  });

  it('handles negative monitor offset (secondary left of primary)', () => {
    const monitors: Monitor[] = [
      { name: 'Left', width: 1920, height: 1080, x: -1920, y: 0, is_primary: false },
      { name: 'Primary', width: 2560, height: 1440, x: 0, y: 0, is_primary: true },
    ];
    const result = normalizeMonitors(monitors, 1);
    // Window offset should be the leftmost monitor's x coord.
    expect(result.windowOffset).toEqual({ x: -1920, y: 0 });
    // After normalization, the left monitor x should be in CSS coords.
    expect(result.monitors[0].x).toBe(-1920);
    expect(result.monitors[1].x).toBe(0);
  });

  it('handles DPR=2 (Retina/HiDPI)', () => {
    const monitors: Monitor[] = [
      { name: '4K', width: 3840, height: 2160, x: 0, y: 0, is_primary: true },
    ];
    const result = normalizeMonitors(monitors, 2);
    expect(result.windowOffset).toEqual({ x: 0, y: 0 });
    // Physical 3840 / DPR 2 = 1920 CSS pixels.
    expect(result.monitors[0].width).toBe(1920);
    expect(result.monitors[0].height).toBe(1080);
  });
});

describe('findMonitor', () => {
  const monitors: Monitor[] = [
    { name: 'Primary', width: 1920, height: 1080, x: 0, y: 0, is_primary: true },
    { name: 'Right', width: 1920, height: 1080, x: 1920, y: 0, is_primary: false },
  ];

  it('finds cursor on primary monitor', () => {
    const idx = findMonitor(500, 300, monitors, { x: 0, y: 0 });
    expect(idx).toBe(0);
  });

  it('finds cursor on secondary monitor', () => {
    const idx = findMonitor(2500, 500, monitors, { x: 0, y: 0 });
    expect(idx).toBe(1);
  });

  it('returns -1 when cursor is outside all monitors', () => {
    const idx = findMonitor(5000, 5000, monitors, { x: 0, y: 0 });
    expect(idx).toBe(-1);
  });

  it('accounts for window offset with negative monitor x', () => {
    const negMonitors: Monitor[] = [
      { name: 'Left', width: 1920, height: 1080, x: -1920, y: 0, is_primary: false },
      { name: 'Right', width: 2560, height: 1440, x: 0, y: 0, is_primary: true },
    ];
    const offset = { x: -1920, y: 0 };
    // Cursor at window-relative (500, 300) => screen (-1420, 300) => should be on Left monitor
    expect(findMonitor(500, 300, negMonitors, offset)).toBe(0);
    // Cursor at window-relative (2500, 500) => screen (580, 500) => should be on Right monitor
    expect(findMonitor(2500, 500, negMonitors, offset)).toBe(1);
  });
});

describe('computeSelectionRect', () => {
  it('creates rect from top-left to bottom-right drag', () => {
    const rect = computeSelectionRect({ x: 100, y: 100 }, { x: 500, y: 400 });
    expect(rect).toEqual({ left: 100, top: 100, width: 400, height: 300 });
  });

  it('handles bottom-right to top-left drag (negative direction)', () => {
    const rect = computeSelectionRect({ x: 500, y: 400 }, { x: 100, y: 100 });
    expect(rect).toEqual({ left: 100, top: 100, width: 400, height: 300 });
  });

  it('handles top-right to bottom-left drag', () => {
    const rect = computeSelectionRect({ x: 500, y: 100 }, { x: 100, y: 400 });
    expect(rect).toEqual({ left: 100, top: 100, width: 400, height: 300 });
  });

  it('returns zero-size rect for click (no drag)', () => {
    const rect = computeSelectionRect({ x: 100, y: 100 }, { x: 100, y: 100 });
    expect(rect).toEqual({ left: 100, top: 100, width: 0, height: 0 });
  });
});

describe('selectionToDesktopCoords', () => {
  it('converts CSS-pixel selection to physical coords at DPR=1', () => {
    const sel: SelectionRect = { left: 100, top: 50, width: 400, height: 300 };
    const result = selectionToDesktopCoords(sel, { x: 0, y: 0 }, 1);
    expect(result).toEqual({ x: 100, y: 50, width: 400, height: 300 });
  });

  it('converts with DPR=2', () => {
    const sel: SelectionRect = { left: 100, top: 50, width: 400, height: 300 };
    const result = selectionToDesktopCoords(sel, { x: 0, y: 0 }, 2);
    expect(result).toEqual({ x: 200, y: 100, width: 800, height: 600 });
  });

  it('accounts for window offset (negative monitor origin)', () => {
    // Monitor at x=-1920 in screen coords, windowOffset.x = -1920.
    // Selection at window-relative (200, 100) => screen (-1720, 100).
    const sel: SelectionRect = { left: 200, top: 100, width: 800, height: 600 };
    const result = selectionToDesktopCoords(sel, { x: -1920, y: 0 }, 1);
    expect(result).toEqual({ x: -1720, y: 100, width: 800, height: 600 });
  });

  it('rounds fractional coordinates', () => {
    const sel: SelectionRect = { left: 100.7, top: 50.2, width: 400.4, height: 299.8 };
    const result = selectionToDesktopCoords(sel, { x: 0, y: 0 }, 1.5);
    expect(result.x).toBe(151); // (100.7 + 0) * 1.5 = 151.05 -> 151
    expect(result.y).toBe(75); // (50.2 + 0) * 1.5 = 75.3 -> 75
    expect(result.width).toBe(601); // 400.4 * 1.5 = 600.6 -> 601
    expect(result.height).toBe(450); // 299.8 * 1.5 = 449.7 -> 450
  });
});

describe('selection persistence after mouseup (minimum size check)', () => {
  it('rejects selections smaller than 2×2 pixels', () => {
    // This mirrors the logic: if w < 2 && h < 2, selection is null.
    function isTooSmall(sel: SelectionRect): boolean {
      return sel.width < 2 && sel.height < 2;
    }
    expect(isTooSmall({ left: 0, top: 0, width: 0, height: 0 })).toBe(true);
    expect(isTooSmall({ left: 0, top: 0, width: 1, height: 1 })).toBe(true);
    expect(isTooSmall({ left: 0, top: 0, width: 1, height: 3 })).toBe(false);
    expect(isTooSmall({ left: 0, top: 0, width: 3, height: 1 })).toBe(false);
    expect(isTooSmall({ left: 0, top: 0, width: 2, height: 2 })).toBe(false);
  });
});
