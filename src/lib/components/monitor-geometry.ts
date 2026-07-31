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

export interface CssRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export function monitorToCssRect(
  monitor: PhysicalMonitor,
  virtualOrigin: PhysicalPoint,
  overlayScaleFactor: number,
): CssRect {
  return {
    left: (monitor.x - virtualOrigin.x) / overlayScaleFactor,
    top: (monitor.y - virtualOrigin.y) / overlayScaleFactor,
    width: monitor.width / overlayScaleFactor,
    height: monitor.height / overlayScaleFactor,
  };
}

export function findMonitorAtClient(
  clientX: number,
  clientY: number,
  monitors: PhysicalMonitor[],
  virtualOrigin: PhysicalPoint,
  overlayScaleFactor: number,
): number {
  const physicalX = virtualOrigin.x + clientX * overlayScaleFactor;
  const physicalY = virtualOrigin.y + clientY * overlayScaleFactor;

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
  virtualOrigin: PhysicalPoint,
  overlayScaleFactor: number,
): PhysicalSelection {
  return {
    x: Math.round(virtualOrigin.x + selection.left * overlayScaleFactor),
    y: Math.round(virtualOrigin.y + selection.top * overlayScaleFactor),
    width: Math.round(selection.width * overlayScaleFactor),
    height: Math.round(selection.height * overlayScaleFactor),
  };
}
