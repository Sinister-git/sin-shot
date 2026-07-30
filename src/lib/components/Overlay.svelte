<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';

  // ---------------------------------------------------------------------------
  // Types
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

  type CaptureMode = 'full-monitor' | 'area-select' | null;

  // ---------------------------------------------------------------------------
  // State
  // ---------------------------------------------------------------------------

  let mode: CaptureMode = $state(null);
  let monitors: Monitor[] = $state([]);
  let primaryOffset = $state({ x: 0, y: 0 });
  let windowOffset = $state({ x: 0, y: 0 });
  let entering = $state(false);

  // Full-monitor mode
  let cursorMonitor: number = $state(-1); // index into monitors[]

  // Area-select mode
  let selecting = $state(false);
  let selStart: Point = $state({ x: 0, y: 0 });
  let selCurrent: Point = $state({ x: 0, y: 0 });
  let selConfirmed: SelectionRect | null = $state(null);
  let mousePos: Point = $state({ x: 0, y: 0 });

  // ---------------------------------------------------------------------------
  // Derived
  // ---------------------------------------------------------------------------

  let selectionRect = $derived.by((): SelectionRect | null => {
    if (selecting) {
      return {
        left: Math.min(selStart.x, selCurrent.x),
        top: Math.min(selStart.y, selCurrent.y),
        width: Math.abs(selCurrent.x - selStart.x),
        height: Math.abs(selCurrent.y - selStart.y),
      };
    }
    return selConfirmed;
  });

  let activeMonitor = $derived.by((): Monitor | null => {
    if (cursorMonitor < 0 || cursorMonitor >= monitors.length) return null;
    return monitors[cursorMonitor];
  });

  // ---------------------------------------------------------------------------
  // Event listeners
  // ---------------------------------------------------------------------------

  let unlisteners: UnlistenFn[] = [];

  async function setupListeners() {
    // Listen for hotkey-pressed events from the Rust hotkey thread.
    const u1 = await listen<{ combo: string }>('hotkey-pressed', (event) => {
      const combo = event.payload.combo;
      if (combo === 'Ctrl+Shift+1') {
        enterCaptureMode('full-monitor');
      } else if (combo === 'Ctrl+Shift+2') {
        enterCaptureMode('area-select');
      }
    });

    // Listen for capture-mode-started (emitted by start_capture after window resize).
    const u2 = await listen<{ mode: string }>('capture-mode-started', (event) => {
      mode = event.payload.mode as CaptureMode;
    });

    // Listen for cancellation.
    const u3 = await listen('capture-mode-cancelled', () => {
      mode = null;
    });

    unlisteners = [u1, u2, u3];
  }

  function cleanupListeners() {
    for (const u of unlisteners) {
      u();
    }
    unlisteners = [];
  }

  // ---------------------------------------------------------------------------
  // Lifecycle
  // ---------------------------------------------------------------------------

  let setupPromise: Promise<void> | null = null;

  $effect(() => {
    setupPromise = setupListeners();
    return () => {
      cleanupListeners();
      setupPromise?.then(() => cleanupListeners());
    };
  });

  $effect(() => {
    if (mode !== null) {
      document.body.classList.add('capture-active');
    } else {
      document.body.classList.remove('capture-active');
    }
  });

  // Global keyboard handler
  function onKeydown(e: KeyboardEvent) {
    if (mode === null) return;

    if (e.key === 'Escape') {
      e.preventDefault();
      cancelCapture();
    } else if (e.key === 'Enter' && mode === 'area-select' && selectionRect) {
      e.preventDefault();
      confirmAreaCapture();
    }
  }

  // ---------------------------------------------------------------------------
  // Capture mode entry / exit
  // ---------------------------------------------------------------------------

  async function enterCaptureMode(m: 'full-monitor' | 'area-select') {
    if (mode !== null || entering) return;
    entering = true;
    try {
      // Fetch monitor information before resizing the window.
      try {
        monitors = await invoke<Monitor[]>('get_monitors');
      } catch {
        monitors = [];
      }

      // Compute the window offset (top-left of bounding box).
      if (monitors.length > 0) {
        let minX = Infinity;
        let minY = Infinity;
        for (const m of monitors) {
          if (m.x < minX) minX = m.x;
          if (m.y < minY) minY = m.y;
        }
        windowOffset = { x: minX, y: minY };
      }

      // Find primary monitor offset.
      const primary = monitors.find((m) => m.is_primary);
      if (primary) {
        primaryOffset = { x: primary.x, y: primary.y };
      }

      // Tell Rust to resize and show the overlay window.
      try {
        await invoke('start_capture', { mode: m });
      } catch (err) {
        console.error('start_capture failed:', err);
      }
    } finally {
      entering = false;
    }
  }

  async function cancelCapture() {
    mode = null;
    try {
      await invoke('cancel_capture');
    } catch (err) {
      console.error('cancel_capture failed:', err);
    }
    resetState();
  }

  function resetState() {
    entering = false;
    mode = null;
    monitors = [];
    cursorMonitor = -1;
    selecting = false;
    selStart = { x: 0, y: 0 };
    selCurrent = { x: 0, y: 0 };
    selConfirmed = null;
  }

  // ---------------------------------------------------------------------------
  // Full-monitor mode — mouse tracking
  // ---------------------------------------------------------------------------

  function onFullMouseMove(e: MouseEvent) {
    // Determine which monitor the cursor is on.
    // Mouse event coordinates are relative to the window. Convert to screen
    // coordinates by adding the window offset.
    const sx = e.clientX + windowOffset.x;
    const sy = e.clientY + windowOffset.y;

    let found = -1;
    for (let i = 0; i < monitors.length; i++) {
      const m = monitors[i];
      if (sx >= m.x && sx < m.x + m.width && sy >= m.y && sy < m.y + m.height) {
        found = i;
        break;
      }
    }
    cursorMonitor = found;
  }

  // ---------------------------------------------------------------------------
  // Full-monitor mode — click to capture
  // ---------------------------------------------------------------------------

  async function onFullClick(_e?: MouseEvent | KeyboardEvent) {
    if (!activeMonitor) return;
    await doFullCapture(activeMonitor.name);
  }

  async function doFullCapture(monitorName: string) {
    try {
      const result = await invoke('capture_full_screen', { monitorName });
      console.log('captured full monitor', monitorName, result);
      // TODO: post-capture flow (show ActionBar, annotation, etc.)
    } catch (err) {
      console.error('capture_full_screen failed:', err);
    }
    await cancelCapture();
  }

  // ---------------------------------------------------------------------------
  // Area-select mode — mouse handlers
  // ---------------------------------------------------------------------------

  function onAreaMouseDown(e: MouseEvent) {
    if (e.button !== 0) return; // left button only
    selecting = true;
    selStart = { x: e.clientX, y: e.clientY };
    selCurrent = { x: e.clientX, y: e.clientY };
  }

  function onAreaMouseMove(e: MouseEvent) {
    mousePos = { x: e.clientX, y: e.clientY };
    if (selecting) {
      selCurrent = { x: e.clientX, y: e.clientY };
    }
  }

  function onAreaMouseUp(_e: MouseEvent) {
    if (!selecting) return;
    selecting = false;

    // Persist the selection so it stays visible while the user chooses
    // to confirm (Enter) or cancel (Escape).
    const w = Math.abs(selCurrent.x - selStart.x);
    const h = Math.abs(selCurrent.y - selStart.y);
    if (w < 2 && h < 2) {
      selConfirmed = null;
    } else {
      selConfirmed = {
        left: Math.min(selStart.x, selCurrent.x),
        top: Math.min(selStart.y, selCurrent.y),
        width: w,
        height: h,
      };
    }
  }

  // ---------------------------------------------------------------------------
  // Area-select — confirm capture
  // ---------------------------------------------------------------------------

  async function confirmAreaCapture() {
    if (!selectionRect) return;

    // Selection rect coordinates are window-relative. Convert to absolute
    // desktop coordinates by adding the window offset (top-left of the
    // monitor bounding box). The backend captures all monitors, stitches
    // them into a virtual-desktop image, and crops to this rect.
    const x = selectionRect.left + windowOffset.x;
    const y = selectionRect.top + windowOffset.y;
    const width = selectionRect.width;
    const height = selectionRect.height;

    if (width < 2 || height < 2) {
      await cancelCapture();
      return;
    }

    try {
      const result = await invoke('capture_area', {
        x: Math.round(x),
        y: Math.round(y),
        width: Math.round(width),
        height: Math.round(height),
      });
      console.log('captured area', result);
      // TODO: post-capture flow
    } catch (err) {
      console.error('capture_area failed:', err);
    }
    await cancelCapture();
  }

  // ---------------------------------------------------------------------------
  // Helpers
  // ---------------------------------------------------------------------------

  /** Format a pixel dimension, e.g. "1920 × 1080". */
  function fmtRes(w: number, h: number): string {
    return `${w} × ${h}`;
  }
</script>

<svelte:window onkeydown={onKeydown} />

<!-- ========================================================================= -->
<!-- Full Monitor Mode                                                          -->
<!-- ========================================================================= -->

{#if mode === 'full-monitor'}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="overlay-full"
    role="button"
    tabindex="-1"
    aria-label="Click to capture monitor"
    onmousemove={onFullMouseMove}
    onclick={onFullClick}
    onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') onFullClick(e); }}
  >
    <!-- Dim overlay -->
    <div class="dim"></div>

    <!-- Monitor highlight -->
    {#if activeMonitor}
      {@const mx = activeMonitor.x - windowOffset.x}
      {@const my = activeMonitor.y - windowOffset.y}
      <div
        class="monitor-highlight"
        style="
          left: {mx}px;
          top: {my}px;
          width: {activeMonitor.width}px;
          height: {activeMonitor.height}px;
        "
      >
        <div class="monitor-label">
          <span class="monitor-name">{activeMonitor.name}</span>
          <span class="monitor-res">{fmtRes(activeMonitor.width, activeMonitor.height)}</span>
        </div>
      </div>
    {/if}

    <!-- No monitor hint (cursor outside known monitors) -->
    {#if !activeMonitor}
      <div class="no-monitor-hint">Move cursor to a monitor to capture</div>
    {/if}
  </div>
{/if}

<!-- ========================================================================= -->
<!-- Area Select Mode                                                          -->
<!-- ========================================================================= -->

{#if mode === 'area-select'}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="overlay-area"
    role="application"
    aria-label="Select capture area"
    onmousedown={onAreaMouseDown}
    onmousemove={onAreaMouseMove}
    onmouseup={onAreaMouseUp}
  >
    <!-- Dim overlay -->
    <div class="dim"></div>

    <!-- Pre-selection: show cursor label -->
    {#if !selecting && !selectionRect}
      <div
        class="cursor-label"
        style="left: {mousePos.x}px; top: {mousePos.y}px;"
      >
        <span class="crosshair">⌖</span>
        <span class="select-hint">Select area</span>
      </div>
    {/if}

    <!-- Selection rectangle with cutout -->
    {#if selectionRect}
      <div
        class="selection-box"
        style="
          left: {selectionRect.left}px;
          top: {selectionRect.top}px;
          width: {selectionRect.width}px;
          height: {selectionRect.height}px;
        "
      >
        <div class="selection-readout">
          {Math.round(selectionRect.width)} × {Math.round(selectionRect.height)}
        </div>
      </div>
    {/if}
  </div>
{/if}

<!-- ========================================================================= -->
<!-- Styles                                                                    -->
<!-- ========================================================================= -->

<style>
  /* ---------------------------------------------------------------------------
   * Shared
   * ------------------------------------------------------------------------- */

  .dim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    pointer-events: none;
  }

  /* ---------------------------------------------------------------------------
   * Full Monitor Mode
   * ------------------------------------------------------------------------- */

  .overlay-full {
    position: fixed;
    inset: 0;
    cursor: crosshair;
    z-index: 1000;
    user-select: none;
    -webkit-user-select: none;
  }

  .monitor-highlight {
    position: absolute;
    border: 3px solid #a78bfa;
    border-radius: 4px;
    box-shadow: 0 0 20px rgba(167, 139, 250, 0.35), inset 0 0 20px rgba(167, 139, 250, 0.08);
    pointer-events: none;
    transition: left 0.08s ease-out, top 0.08s ease-out, width 0.08s ease-out,
      height 0.08s ease-out;
  }

  .monitor-label {
    position: absolute;
    top: -42px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    background: rgba(40, 42, 54, 0.92);
    border: 1px solid #a78bfa;
    border-radius: 8px;
    padding: 6px 16px;
    white-space: nowrap;
    backdrop-filter: blur(8px);
  }

  .monitor-name {
    font-family: 'Segoe UI', system-ui, sans-serif;
    font-size: 13px;
    font-weight: 600;
    color: #f8f8f2;
  }

  .monitor-res {
    font-family: 'Cascadia Code', 'Fira Code', monospace;
    font-size: 12px;
    color: #a78bfa;
  }

  .no-monitor-hint {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    font-family: 'Segoe UI', system-ui, sans-serif;
    font-size: 16px;
    color: rgba(255, 255, 255, 0.5);
  }

  /* ---------------------------------------------------------------------------
   * Area Select Mode
   * ------------------------------------------------------------------------- */

  .overlay-area {
    position: fixed;
    inset: 0;
    cursor: crosshair;
    z-index: 1000;
    user-select: none;
    -webkit-user-select: none;
  }

  /* Cursor label before dragging */
  .cursor-label {
    position: fixed;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    transform: translate(-50%, 24px);
    pointer-events: none;
    z-index: 1001;
  }

  .crosshair {
    font-size: 28px;
    color: #a78bfa;
    line-height: 1;
  }

  .select-hint {
    font-family: 'Segoe UI', system-ui, sans-serif;
    font-size: 12px;
    color: #a78bfa;
    background: rgba(40, 42, 54, 0.88);
    border-radius: 6px;
    padding: 4px 10px;
    white-space: nowrap;
    backdrop-filter: blur(4px);
  }

  /* Selection rectangle — cutout effect via box-shadow */
  .selection-box {
    position: fixed;
    border: 2px dashed #a78bfa;
    border-radius: 2px;
    background: transparent;
    box-shadow: 0 0 0 9999px rgba(0, 0, 0, 0.55);
    pointer-events: none;
    z-index: 1001;
  }

  /* Pixel size readout at bottom-right of selection */
  .selection-readout {
    position: absolute;
    bottom: -32px;
    right: 0;
    font-family: 'Cascadia Code', 'Fira Code', monospace;
    font-size: 12px;
    color: #f8f8f2;
    background: rgba(40, 42, 54, 0.92);
    border: 1px solid #a78bfa;
    border-radius: 6px;
    padding: 4px 10px;
    white-space: nowrap;
    backdrop-filter: blur(8px);
  }

  /* ---------------------------------------------------------------------------
   * Global resets
   * ------------------------------------------------------------------------- */

  :global(body) {
    margin: 0;
    padding: 0;
    overflow: hidden;
  }

  :global(body.capture-active) {
    background: transparent;
  }
</style>
