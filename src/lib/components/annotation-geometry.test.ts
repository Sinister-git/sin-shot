/// <reference types="vitest/globals" />

import {
  annotationFrameLayout,
  fitImageFrame,
} from './annotation-geometry';

describe('annotation frame geometry', () => {
  it('keeps the selected frame and controls in one coordinate system', () => {
    const layout = annotationFrameLayout(
      { left: 120, top: 80, width: 640, height: 360 },
      { width: 1280, height: 900 },
    );

    expect(layout.frame).toEqual({ left: 120, top: 80, width: 640, height: 360 });
    expect(layout.toolbar.left).toBe(768);
    expect(layout.toolbar.top).toBe(80);
    expect(layout.actions.left).toBe(120);
    expect(layout.actions.width).toBe(640);
    expect(layout.actions.top).toBe(448);
  });

  it('places controls on available edges for selections at screen boundaries', () => {
    const layout = annotationFrameLayout(
      { left: 0, top: 0, width: 300, height: 700 },
      { width: 340, height: 760 },
    );

    expect(layout.toolbar.left).toBe(292);
    expect(layout.toolbar.top).toBe(0);
    expect(layout.actions.left).toBe(0);
    expect(layout.actions.top).toBe(708);
  });

  it.each([
    [1920, 1080],
    [300, 1200],
    [1200, 300],
    [2, 2],
  ])('fits %dx%d without CSS multiplication', (width, height) => {
    const frame = fitImageFrame(width, height, { width: 900, height: 700 });
    expect(frame.width).toBeGreaterThan(0);
    expect(frame.height).toBeGreaterThan(0);
    expect(frame.width / frame.height).toBeCloseTo(width / height, 2);
  });
});
