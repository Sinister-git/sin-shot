/**
 * Native monitor geometry contract.
 *
 * Monitor positions and dimensions are physical desktop pixels. The overlay
 * window has one CSS-to-physical scale after it has been placed; use that
 * scale only at the webview boundary. Do not divide each monitor by its own
 * Windows scale factor: doing so mixes logical monitor coordinates with the
 * native physical virtual-desktop coordinate space.
 */

export interface PhysicalMonitor {
  name: string;
  width: number;
  height: number;
  x: number;
  y: number;
  is_primary: boolean;
  scale_factor: number;
}

export interface PhysicalPoint {
  x: number;
  y: number;
}

/**
 * The post-placement WebView client rectangle in physical desktop pixels.
 * Native window outer bounds must not be substituted for this value.
 */
export interface OverlayClientGeometry {
  origin: PhysicalPoint;
  width: number;
  height: number;
  scaleFactor: number;
}

export interface CssRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** Validate the native-to-WebView geometry contract before using it. */
export function isValidOverlayClientGeometry(geometry: OverlayClientGeometry): boolean {
  return Number.isFinite(geometry.origin.x) &&
    Number.isFinite(geometry.origin.y) &&
    Number.isFinite(geometry.width) && geometry.width > 0 &&
    Number.isFinite(geometry.height) && geometry.height > 0 &&
    Number.isFinite(geometry.scaleFactor) && geometry.scaleFactor > 0;
}

export function monitorToCssRect(
  monitor: PhysicalMonitor,
  clientOrigin: PhysicalPoint,
  overlayScaleFactor: number,
): CssRect {
  return {
    left: (monitor.x - clientOrigin.x) / overlayScaleFactor,
    top: (monitor.y - clientOrigin.y) / overlayScaleFactor,
    width: monitor.width / overlayScaleFactor,
    height: monitor.height / overlayScaleFactor,
  };
}

export function findMonitorAtClient(
  clientX: number,
  clientY: number,
  monitors: PhysicalMonitor[],
  clientOrigin: PhysicalPoint,
  overlayScaleFactor: number,
): number {
  const physicalX = clientOrigin.x + clientX * overlayScaleFactor;
  const physicalY = clientOrigin.y + clientY * overlayScaleFactor;

  return monitors.findIndex((monitor) =>
    physicalX >= monitor.x &&
    physicalX < monitor.x + monitor.width &&
    physicalY >= monitor.y &&
    physicalY < monitor.y + monitor.height,
  );
}

export interface CssSelection {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface PhysicalSelection {
  x: number;
  y: number;
  width: number;
  height: number;
}

export function selectionToDesktopCoords(
  selection: CssSelection,
  clientOrigin: PhysicalPoint,
  overlayScaleFactor: number,
): PhysicalSelection {
  return {
    x: Math.round(clientOrigin.x + selection.left * overlayScaleFactor),
    y: Math.round(clientOrigin.y + selection.top * overlayScaleFactor),
    width: Math.round(selection.width * overlayScaleFactor),
    height: Math.round(selection.height * overlayScaleFactor),
  };
}
