<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import AnnotationCanvas from './AnnotationCanvas.svelte';
  import Toolbar from './Toolbar.svelte';
  import ActionBar from './ActionBar.svelte';
  import type { Tool } from '$lib/types';
  import {
    moveSelection,
    pointInSelection,
    selectionFromPointerRelease,
    type Point,
    type SelectionRect,
  } from './selection-geometry';
  import {
    findMonitorAtClient,
    monitorToCssRect,
    selectionToDesktopCoords,
    type PhysicalMonitor,
  } from './monitor-geometry';
  import {
    annotationTransition,
    areaExportFinished,
    areaExportRequested,
    areaSelectionCancelled,
    areaSelectionReleased,
    type AreaEditorPhase,
    type CapturedImage,
  } from './capture-flow';
  import {
    annotationFrameLayout,
    fitImageFrame,
    rectStyle,
    type AnnotationFrameLayout,
  } from './annotation-geometry';

  // ---------------------------------------------------------------------------
  // Types
  // ---------------------------------------------------------------------------

  type Monitor = PhysicalMonitor;

  interface OverlayGeometry {
    monitors: Monitor[];
    origin_x: number;
    origin_y: number;
    width: number;
    height: number;
    scale_factor: number;
  }

  type CaptureMode = 'full-monitor' | 'area-select' | null;

  // ---------------------------------------------------------------------------
  // Flow state
  // ---------------------------------------------------------------------------

  type FlowState = 'idle' | 'capturing' | 'annotating' | 'uploading';
  let flowState: FlowState = $state('idle');

  // Captured image from the backend
  let capturedImage: CapturedImage | null = $state(null);

  // Annotation state
  let currentTool: Tool = $state('pen');
  let currentColor: string = $state('#ff0000');
  let uploading = $state(false);
  let uploadUrl: string | null = $state(null);
  let wasCopied = $state(false);

  // Keyboard tool-switch flash state
  let flashTool: Tool | null = $state(null);
  let flashTimeout: ReturnType<typeof setTimeout> | null = null;

  // Ref to AnnotationCanvas for export
  let annotationCanvas: AnnotationCanvas | null = $state(null);

  // ---------------------------------------------------------------------------
  // Capture state
  // ---------------------------------------------------------------------------

  let mode: CaptureMode = $state(null);
  // Native contract: monitor bounds, the virtual-desktop origin, and the
  // overlay window bounds are all physical desktop pixels. CSS coordinates
  // are derived only at the webview boundary using the overlay's actual
  // post-placement scale factor. Monitor scale factors are metadata, not a
  // second transform applied to already-physical bounds.
  let monitors: Monitor[] = $state([]);
  let virtualOrigin = $state({ x: 0, y: 0 });
  let overlayScaleFactor = $state(typeof window !== 'undefined' ? window.devicePixelRatio || 1 : 1);
  let entering = $state(false);

  // Full-monitor mode
  let cursorMonitor: number = $state(-1);

  // Area-select mode
  let selecting = $state(false);
  let movingSelection = $state(false);
  let selStart: Point = $state({ x: 0, y: 0 });
  let selCurrent: Point = $state({ x: 0, y: 0 });
  let moveStart: Point = $state({ x: 0, y: 0 });
  let moveOrigin: SelectionRect | null = $state(null);
  let selConfirmed: SelectionRect | null = $state(null);
  // Keep one immutable frame contract through annotation and final export.
  let selectionSnapshot: SelectionRect | null = $state(null);
  let areaPhase: AreaEditorPhase = $state('selecting');
  let areaCapturePending = $state(false);
  let mousePos: Point = $state({ x: 0, y: 0 });
  let captureGeneration = $state(0);

  let areaEditing = $derived(((flowState as FlowState) === 'annotating' || (flowState as FlowState) === 'uploading') && mode === 'area-select' && ((areaPhase as AreaEditorPhase) === 'annotating' || (areaPhase as AreaEditorPhase) === 'committing') && selectionSnapshot !== null);
  let annotationPreview = $derived.by((): CapturedImage | null => {
    if (!areaEditing || !selectionSnapshot) return capturedImage;
    const physical = selectionToDesktopCoords(selectionSnapshot, virtualOrigin, overlayScaleFactor);
    return { data: '', width: physical.width, height: physical.height };
  });
  let annotationLayout = $derived.by((): AnnotationFrameLayout | null => {
    const image = annotationPreview;
    if (!image || image.width < 1 || image.height < 1) return null;
    const viewport = { width: window.innerWidth, height: window.innerHeight };
    if (areaEditing && selectionSnapshot) {
      return annotationFrameLayout(selectionSnapshot, viewport);
    }
    const frame = fitImageFrame(image.width, image.height, viewport);
    const centered = {
      ...frame,
      left: Math.max(0, (viewport.width - frame.width) / 2),
      top: Math.max(0, (viewport.height - frame.height) / 2),
    };
    return annotationFrameLayout(centered, viewport);
  });

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

  // Loaded hotkey combos for dynamic comparison
  let hotkeyFull: string = $state('Ctrl+Shift+1');
  let hotkeyArea: string = $state('Ctrl+Shift+2');

  async function loadHotkeySettings() {
    try {
      const s = await invoke<{ hotkey_full: string; hotkey_area: string }>('get_settings');
      hotkeyFull = s.hotkey_full;
      hotkeyArea = s.hotkey_area;
    } catch {
      // keep defaults
    }
  }

  async function setupListeners() {
    await loadHotkeySettings();

    const u1 = await listen<{ combo: string }>('hotkey-pressed', (event) => {
      const combo = event.payload.combo;
      if (combo === hotkeyFull) {
        enterCaptureMode('full-monitor');
      } else if (combo === hotkeyArea) {
        enterCaptureMode('area-select');
      }
    });

    const u2 = await listen<{ mode: string; geometry?: OverlayGeometry }>('capture-mode-started', (event) => {
      if (event.payload.geometry) applyOverlayGeometry(event.payload.geometry);
      mode = event.payload.mode as CaptureMode;
      flowState = 'capturing';
    });

    const u3 = await listen('capture-mode-cancelled', () => {
      mode = null;
      flowState = 'idle';
    });

    const u4 = await listen<{ hotkey_full: string; hotkey_area: string }>('settings-changed', (event) => {
      hotkeyFull = event.payload.hotkey_full;
      hotkeyArea = event.payload.hotkey_area;
    });

    unlisteners = [u1, u2, u3, u4];
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
    if (flowState !== 'idle') {
      document.body.classList.add('capture-active');
    } else {
      document.body.classList.remove('capture-active');
    }
  });

  // ---------------------------------------------------------------------------
  // Global keyboard handler
  // ---------------------------------------------------------------------------

  function onKeydown(e: KeyboardEvent) {
    if (flowState === 'idle') return;

    // Tool-switching shortcuts (annotation mode only, no modifiers, no input focus)
    if (
      flowState === 'annotating' &&
      !e.ctrlKey &&
      !e.altKey &&
      !e.metaKey &&
      document.activeElement?.tagName !== 'INPUT' &&
      document.activeElement?.tagName !== 'TEXTAREA'
    ) {
      const toolMap: Record<string, Tool> = {
        p: 'pen',
        a: 'arrow',
        r: 'rectangle',
        t: 'text',
        b: 'blur',
        e: 'eraser',
      };
      const t = toolMap[e.key.toLowerCase()];
      if (t) {
        e.preventDefault();
        currentTool = t;
        if (flashTimeout) clearTimeout(flashTimeout);
        flashTool = t;
        flashTimeout = setTimeout(() => {
          flashTool = null;
          flashTimeout = null;
        }, 400);
        return;
      }
    }

    if (e.key === 'Escape') {
      e.preventDefault();
      if (flowState === 'annotating' || flowState === 'uploading') {
        handleCancel();
      } else if (flowState === 'capturing') {
        cancelCapture();
      }
    }

  }

  // ---------------------------------------------------------------------------
  // Capture mode entry / exit
  // ---------------------------------------------------------------------------

  async function enterCaptureMode(m: 'full-monitor' | 'area-select') {
    if (flowState !== 'idle' || entering) return;
    entering = true;
    try {
      try {
        // start_capture enumerates the monitors used to place the native
        // window and returns that same physical-coordinate snapshot. This
        // avoids racing a second monitor enumeration and avoids sampling the
        // compact window's DPR before the overlay is moved.
        const geometry = await invoke<OverlayGeometry>('start_capture', { mode: m });
        applyOverlayGeometry(geometry);
      } catch (err) {
        console.error('start_capture failed:', err);
      }
    } finally {
      entering = false;
    }
  }

  async function cancelCapture() {
    if (entering) return;
    mode = null;
    flowState = 'idle';
    entering = true;
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
    flowState = 'idle';
    monitors = [];
    virtualOrigin = { x: 0, y: 0 };
    overlayScaleFactor = 1;
    cursorMonitor = -1;
    selecting = false;
    movingSelection = false;
    moveOrigin = null;
    selStart = { x: 0, y: 0 };
    selCurrent = { x: 0, y: 0 };
    selConfirmed = null;
    selectionSnapshot = null;
    areaPhase = 'selecting';
    areaCapturePending = false;
    capturedImage = null;
    uploading = false;
    uploadUrl = null;
    wasCopied = false;
    currentTool = 'pen';
    currentColor = '#ff0000';
    captureGeneration++;
  }

  // ---------------------------------------------------------------------------
  // Full-monitor mode — mouse tracking
  // ---------------------------------------------------------------------------

  function onFullMouseMove(e: MouseEvent) {
    cursorMonitor = findMonitorAtClient(
      e.clientX,
      e.clientY,
      monitors,
      virtualOrigin,
      overlayScaleFactor,
    );
  }

  // ---------------------------------------------------------------------------
  // Full-monitor mode — click to capture → annotate
  // ---------------------------------------------------------------------------

  async function onFullClick(_e?: MouseEvent | KeyboardEvent) {
    if (!activeMonitor) return;
    await doFullCapture(activeMonitor.name);
  }

  async function doFullCapture(monitorName: string) {
    const gen = captureGeneration;
    try {
      const result = await invoke<CapturedImage>('capture_full_screen', { monitorName });
      if (captureGeneration !== gen) return;
      transitionToAnnotation(result);
    } catch (err) {
      console.error('capture_full_screen failed:', err);
      await cancelCapture();
    }
  }

  // ---------------------------------------------------------------------------
  // Area-select mode — mouse handlers
  // ---------------------------------------------------------------------------

  function areaBounds(target: EventTarget | null): { width: number; height: number } {
    const overlay = target as HTMLElement | null;
    return {
      width: overlay?.clientWidth || window.innerWidth,
      height: overlay?.clientHeight || window.innerHeight,
    };
  }

  function onAreaPointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    const point = { x: e.clientX, y: e.clientY };
    const overlay = e.currentTarget as HTMLElement;
    overlay.setPointerCapture?.(e.pointerId);

    if (selConfirmed && pointInSelection(point, selConfirmed)) {
      movingSelection = true;
      moveStart = point;
      moveOrigin = { ...selConfirmed };
      return;
    }

    movingSelection = false;
    moveOrigin = null;
    selecting = true;
    selConfirmed = null;
    selStart = point;
    selCurrent = point;
  }

  function onAreaPointerMove(e: PointerEvent) {
    mousePos = { x: e.clientX, y: e.clientY };
    if (selecting) {
      selCurrent = { x: e.clientX, y: e.clientY };
    } else if (movingSelection && moveOrigin) {
      selConfirmed = moveSelection(
        moveOrigin,
        { x: e.clientX - moveStart.x, y: e.clientY - moveStart.y },
        areaBounds(e.currentTarget),
      );
    }
  }

  async function onAreaPointerUp(e: PointerEvent) {
    const overlay = e.currentTarget as HTMLElement;
    overlay.releasePointerCapture?.(e.pointerId);

    if (movingSelection) {
      movingSelection = false;
      moveOrigin = null;
      return;
    }
    if (!selecting) return;
    selecting = false;

    const completedSelection = selectionFromPointerRelease(selStart, selCurrent);
    selConfirmed = completedSelection;
    if (completedSelection) {
      // Release confirms the editable frame, not a native capture. Native
      // capture is deferred until Save, Copy, or Upload commits the frame.
      const editor = areaSelectionReleased(completedSelection);
      selectionSnapshot = editor.selection;
      areaPhase = editor.phase;
      flowState = 'annotating';
    }
  }

  // ---------------------------------------------------------------------------
  // Area-select — final capture/export
  // ---------------------------------------------------------------------------

  function onAnnotationPointerDown(e: PointerEvent) {
    if (!selectionSnapshot || e.button !== 0) return;
    movingSelection = true;
    moveStart = { x: e.clientX, y: e.clientY };
    moveOrigin = { ...selectionSnapshot };
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  }

  function onAnnotationPointerMove(e: PointerEvent) {
    if (!movingSelection || !moveOrigin) return;
    selectionSnapshot = moveSelection(
      moveOrigin,
      { x: e.clientX - moveStart.x, y: e.clientY - moveStart.y },
      { width: window.innerWidth, height: window.innerHeight },
    );
    selConfirmed = selectionSnapshot;
  }

  function onAnnotationPointerUp(e: PointerEvent) {
    if (!movingSelection) return;
    (e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId);
    movingSelection = false;
    moveOrigin = null;
  }

  async function captureAreaForExport(selection: SelectionRect): Promise<CapturedImage | null> {
    if (areaCapturePending) return null;
    areaCapturePending = true;

    const physicalSelection = selectionToDesktopCoords(
      selection,
      virtualOrigin,
      overlayScaleFactor,
    );
    const { width, height } = physicalSelection;

    if (width < 2 || height < 2) {
      areaCapturePending = false;
      return null;
    }

    const gen = captureGeneration;
    let overlayHidden = false;
    try {
      await invoke('hide_capture_overlay');
      overlayHidden = true;
      if (
        captureGeneration !== gen ||
        (flowState !== 'annotating' && flowState !== 'uploading') ||
        mode !== 'area-select'
      ) return null;
      const result = await invoke<CapturedImage>('capture_area', {
        x: physicalSelection.x,
        y: physicalSelection.y,
        width: physicalSelection.width,
        height: physicalSelection.height,
      });
      if (captureGeneration !== gen) return null;
      return result;
    } catch (err) {
      console.error('capture_area failed:', err);
      return null;
    } finally {
      if (
        overlayHidden &&
        captureGeneration === gen &&
        (flowState === 'annotating' || flowState === 'uploading') &&
        mode === 'area-select'
      ) {
        try {
          await invoke('show_capture_overlay');
        } catch (err) {
          console.error('show_capture_overlay failed:', err);
        }
      }
      areaCapturePending = false;
    }
  }

  async function exportCurrentFrame(): Promise<string | null> {
    const shouldCaptureArea = areaEditing && selectionSnapshot;
    if (shouldCaptureArea) {
      areaPhase = areaExportRequested({ phase: areaPhase, selection: selectionSnapshot }).phase;
    }
    const finalImage = shouldCaptureArea && selectionSnapshot
      ? await captureAreaForExport(selectionSnapshot)
      : undefined;
    if (shouldCaptureArea) {
      areaPhase = areaExportFinished({ phase: areaPhase, selection: selectionSnapshot }).phase;
    }
    if (shouldCaptureArea && !finalImage) return null;
    return await (annotationCanvas?.getAnnotatedImage(finalImage ?? undefined) ?? null);
  }

  // ---------------------------------------------------------------------------
  // Post-capture → annotation transition
  // ---------------------------------------------------------------------------

  function transitionToAnnotation(image: CapturedImage) {
    const transition = annotationTransition(image);
    mode = transition.mode; // clear capture mode — we're past capture
    flowState = transition.flowState;
    capturedImage = transition.capturedImage;
  }

  // ---------------------------------------------------------------------------
  // Annotation action handlers
  // ---------------------------------------------------------------------------

  async function handleCopy() {
    const pngBase64 = await exportCurrentFrame();
    if (!pngBase64) return;
    try {
      await invoke('copy_to_clipboard', { imageDataBase64: pngBase64 });
    } catch (err) {
      console.error('copy_to_clipboard failed:', err);
    }
    await finishAnnotation();
  }

  async function handleSave() {
    const pngBase64 = await exportCurrentFrame();
    if (!pngBase64) return;
    try {
      const savedPath = await invoke<string>('save_to_file', { imageDataBase64: pngBase64 });
      console.log('saved to', savedPath);
    } catch (err) {
      console.error('save_to_file failed:', err);
    }
    await finishAnnotation();
  }

  async function handleUpload() {
    const pngBase64 = await exportCurrentFrame();
    if (!pngBase64) return;

    uploading = true;
    flowState = 'uploading';
    uploadUrl = null;

    try {
      const url = await invoke<string>('upload_screenshot', {
        imageDataBase64: pngBase64,
        filename: `sin-shot-${Date.now()}.png`,
      });
      // Convert to short URL: https://sinister.ovh/{key} → https://sinister.ovh/x/{key}
      const shortUrl = url.replace(/\/([^/]+)$/, '/x/$1');
      uploadUrl = shortUrl;

      // Copy the short URL to clipboard
      try {
        await navigator.clipboard.writeText(shortUrl);
        wasCopied = true;
      } catch {
        // best-effort URL copy
      }

      // Show toast briefly then finish
      // Keep visible for 3 seconds so the user sees the toast, then finish
      await new Promise(resolve => setTimeout(resolve, 3000));
    } catch (err) {
      console.error('upload failed:', err);
    }

    uploading = false;
    await finishAnnotation();
  }

  async function handleCancel() {
    areaPhase = areaSelectionCancelled().phase;
    await finishAnnotation();
  }

  async function finishAnnotation() {
    capturedImage = null;
    selectionSnapshot = null;
    areaPhase = 'selecting';
    uploading = false;
    uploadUrl = null;
    wasCopied = false;
    mode = null;
    await cancelCapture();
  }

  // ---------------------------------------------------------------------------
  // Helpers
  // ---------------------------------------------------------------------------

  function applyOverlayGeometry(geometry: OverlayGeometry) {
    monitors = geometry.monitors;
    virtualOrigin = { x: geometry.origin_x, y: geometry.origin_y };
    overlayScaleFactor = geometry.scale_factor > 0 ? geometry.scale_factor : 1;
  }

  function fmtRes(w: number, h: number): string {
    return `${w} × ${h}`;
  }
</script>

<svelte:window onkeydown={onKeydown} />

<!-- ========================================================================= -->
<!-- Full Monitor Mode                                                          -->
<!-- ========================================================================= -->

{#if flowState === 'capturing' && mode === 'full-monitor'}
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
    <div class="dim"></div>

    {#if activeMonitor}
      {@const monitorRect = monitorToCssRect(activeMonitor, virtualOrigin, overlayScaleFactor)}
      <div
        class="monitor-highlight"
        style="
          left: {monitorRect.left}px;
          top: {monitorRect.top}px;
          width: {monitorRect.width}px;
          height: {monitorRect.height}px;
        "
      >
        <div class="monitor-label">
          <span class="monitor-name">{activeMonitor.name}</span>
          <span class="monitor-res">{fmtRes(activeMonitor.width, activeMonitor.height)}</span>
        </div>
      </div>
    {/if}

    {#if !activeMonitor}
      <div class="no-monitor-hint">Move cursor to a monitor to capture</div>
    {/if}
  </div>
{/if}

<!-- ========================================================================= -->
<!-- Area Select Mode                                                          -->
<!-- ========================================================================= -->

{#if flowState === 'capturing' && mode === 'area-select'}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="overlay-area"
    role="application"
    aria-label="Select capture area"
    onpointerdown={onAreaPointerDown}
    onpointermove={onAreaPointerMove}
    onpointerup={onAreaPointerUp}
    onpointercancel={onAreaPointerUp}
  >
    {#if !selectionRect || selectionRect.width < 1 || selectionRect.height < 1}
      <div class="dim"></div>
    {:else}
      <svg class="selection-mask" aria-hidden="true">
        <defs>
          <mask id="selection-cutout" maskUnits="userSpaceOnUse" maskContentUnits="userSpaceOnUse" x="0" y="0" width="100%" height="100%">
            <rect width="100%" height="100%" fill="white" />
            <rect
              x={selectionRect.left}
              y={selectionRect.top}
              width={selectionRect.width}
              height={selectionRect.height}
              fill="black"
            />
          </mask>
        </defs>
        <rect width="100%" height="100%" fill="rgba(0, 0, 0, 0.55)" mask="url(#selection-cutout)" />
      </svg>
    {/if}

    {#if !selecting && !selectionRect}
      <div
        class="cursor-label"
        style="left: {mousePos.x}px; top: {mousePos.y}px;"
      >
        <span class="crosshair">⌖</span>
        <span class="select-hint">Select area</span>
      </div>
    {/if}

    {#if selectionRect && selectionRect.width >= 1 && selectionRect.height >= 1}
      <div
        class="selection-box"
        class:moving={movingSelection}
        style="
          left: {selectionRect.left}px;
          top: {selectionRect.top}px;
          width: {selectionRect.width}px;
          height: {selectionRect.height}px;
        "
      >
        <div class="selection-readout">
          {Math.round(selectionRect.width * overlayScaleFactor)} × {Math.round(selectionRect.height * overlayScaleFactor)}
        </div>
      </div>
    {/if}
  </div>
{/if}

<!-- ========================================================================= -->
<!-- Annotation Mode — captured image + tools                                  -->
<!-- ========================================================================= -->

{#if (flowState === 'annotating' || flowState === 'uploading') && annotationPreview && annotationLayout}
  <div class="annotation-overlay">
    <!-- One backdrop is used in area mode; the selected frame is explicitly cut out. -->
    {#if areaEditing && selectionSnapshot}
      <svg class="annotation-selection-mask" aria-hidden="true">
        <defs>
          <mask id="annotation-selection-cutout" maskUnits="userSpaceOnUse" maskContentUnits="userSpaceOnUse" x="0" y="0" width="100%" height="100%">
            <rect width="100%" height="100%" fill="white" />
            <rect
              x={selectionSnapshot.left}
              y={selectionSnapshot.top}
              width={selectionSnapshot.width}
              height={selectionSnapshot.height}
              fill="black"
            />
          </mask>
        </defs>
        <rect width="100%" height="100%" fill="rgba(0, 0, 0, 0.55)" mask="url(#annotation-selection-cutout)" />
      </svg>
      <!-- The frame remains movable after release; its geometry is the source
           of truth for both the live cutout and the eventual native crop. -->
      <div class="selection-box annotation-selection-box" style={rectStyle(selectionSnapshot)}>
        <!-- Only the measured border handles move the frame; the image remains
             interactive for annotation. -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span class="selection-edge edge-top" onpointerdown={onAnnotationPointerDown} onpointermove={onAnnotationPointerMove} onpointerup={onAnnotationPointerUp} onpointercancel={onAnnotationPointerUp}></span>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span class="selection-edge edge-right" onpointerdown={onAnnotationPointerDown} onpointermove={onAnnotationPointerMove} onpointerup={onAnnotationPointerUp} onpointercancel={onAnnotationPointerUp}></span>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span class="selection-edge edge-bottom" onpointerdown={onAnnotationPointerDown} onpointermove={onAnnotationPointerMove} onpointerup={onAnnotationPointerUp} onpointercancel={onAnnotationPointerUp}></span>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span class="selection-edge edge-left" onpointerdown={onAnnotationPointerDown} onpointermove={onAnnotationPointerMove} onpointerup={onAnnotationPointerUp} onpointercancel={onAnnotationPointerUp}></span>
      </div>
    {:else}
      <div class="annotation-backdrop"></div>
    {/if}

    <div class="annotation-layout">
      <div class="annotation-frame" style={rectStyle(annotationLayout.frame)}>
        <div class="annotation-image-container">
          <AnnotationCanvas
            bind:this={annotationCanvas}
            imageData={capturedImage?.data ?? ''}
            imageWidth={annotationPreview.width}
            imageHeight={annotationPreview.height}
            transparent={areaEditing}
            tool={currentTool}
            color={currentColor}
          />
        </div>
        <Toolbar
          bind:activeTool={currentTool}
          bind:color={currentColor}
          {flashTool}
          layoutStyle={rectStyle({
            left: annotationLayout.toolbar.left - annotationLayout.frame.left,
            top: annotationLayout.toolbar.top - annotationLayout.frame.top,
            width: annotationLayout.toolbar.width,
            height: annotationLayout.toolbar.height,
          })}
        />
      </div>

      <ActionBar
        onCopy={handleCopy}
        onSave={handleSave}
        onUpload={handleUpload}
        onCancel={handleCancel}
        uploading={uploading}
        layoutStyle={rectStyle(annotationLayout.actions)}
      />

      {#if uploadUrl}
        <div class="upload-toast">
          <span class="toast-icon">📋</span>
          <div class="toast-body">
            <span class="toast-title">{wasCopied ? 'URL copied!' : 'Uploaded!'}</span>
            <span class="toast-url">{uploadUrl}</span>
          </div>
        </div>
      {/if}
    </div>
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
    box-sizing: border-box;
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

  .selection-mask {
    position: fixed;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
    z-index: 1000;
  }

  .selection-box {
    position: fixed;
    border: 2px dashed #a78bfa;
    border-radius: 2px;
    box-sizing: border-box;
    background: transparent;
    pointer-events: all;
    cursor: move;
    z-index: 1001;
  }

  .selection-box.moving {
    cursor: grabbing;
  }

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
   * Annotation Mode
   * ------------------------------------------------------------------------- */

  .annotation-overlay {
    position: fixed;
    inset: 0;
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
    user-select: none;
    -webkit-user-select: none;
  }

  .annotation-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.85);
    pointer-events: none;
  }

  .annotation-selection-mask {
    position: fixed;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
    z-index: 0;
  }

  .annotation-layout {
    position: absolute;
    inset: 0;
    z-index: 1;
    pointer-events: none;
  }

  .annotation-frame {
    position: absolute;
    pointer-events: none;
  }

  .annotation-image-container {
    position: absolute;
    inset: 0;
    pointer-events: all;
    overflow: hidden;
    border: 2px solid #a78bfa;
    border-radius: 4px;
    box-sizing: border-box;
    box-shadow: 0 0 40px rgba(0, 0, 0, 0.6);
  }

  .annotation-selection-box {
    z-index: 2;
    pointer-events: none;
    border-style: solid;
  }

  .selection-edge {
    position: absolute;
    pointer-events: all;
    z-index: 3;
  }

  .selection-edge.edge-top,
  .selection-edge.edge-bottom {
    left: 0;
    right: 0;
    height: 10px;
    cursor: move;
  }

  .selection-edge.edge-top { top: -5px; }
  .selection-edge.edge-bottom { bottom: -5px; }

  .selection-edge.edge-left,
  .selection-edge.edge-right {
    top: 0;
    bottom: 0;
    width: 10px;
    cursor: move;
  }

  .selection-edge.edge-left { left: -5px; }
  .selection-edge.edge-right { right: -5px; }

  .upload-toast {
    position: absolute;
    top: 8px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 10px;
    background: rgba(40, 42, 54, 0.95);
    border: 1px solid #50c878;
    border-radius: 8px;
    padding: 10px 18px;
    white-space: nowrap;
    backdrop-filter: blur(8px);
    z-index: 20;
    animation: toast-in 0.25s ease-out;
  }

  @keyframes toast-in {
    from { opacity: 0; transform: translateX(-50%) translateY(6px); }
    to   { opacity: 1; transform: translateX(-50%) translateY(0); }
  }

  .toast-icon {
    font-size: 20px;
    line-height: 1;
  }

  .toast-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .toast-title {
    font-family: 'Segoe UI', system-ui, sans-serif;
    font-size: 13px;
    font-weight: 600;
    color: #50c878;
  }

  .toast-url {
    font-family: 'Cascadia Code', 'Fira Code', monospace;
    font-size: 11px;
    color: #a78bfa;
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
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
