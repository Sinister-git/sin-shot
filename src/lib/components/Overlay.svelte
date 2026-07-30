<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import AnnotationCanvas from './AnnotationCanvas.svelte';
  import Toolbar from './Toolbar.svelte';
  import ActionBar from './ActionBar.svelte';
  import type { Tool } from '$lib/types';

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
  // Flow state
  // ---------------------------------------------------------------------------

  type FlowState = 'idle' | 'capturing' | 'annotating' | 'uploading';
  let flowState: FlowState = $state('idle');

  // Captured image from the backend
  interface CapturedImage {
    data: string;   // base64-encoded RGBA pixels
    width: number;
    height: number;
  }
  let capturedImage: CapturedImage | null = $state(null);

  // Annotation state
  let currentTool: Tool = $state('pen');
  let currentColor: string = $state('#ff0000');
  let uploading = $state(false);
  let uploadUrl: string | null = $state(null);
  let wasCopied = $state(false);

  // Ref to AnnotationCanvas for export
  let annotationCanvas: AnnotationCanvas | null = $state(null);

  // ---------------------------------------------------------------------------
  // Capture state
  // ---------------------------------------------------------------------------

  let mode: CaptureMode = $state(null);
  let monitors: Monitor[] = $state([]);
  let windowOffset = $state({ x: 0, y: 0 });
  let entering = $state(false);
  let dpr = $state(typeof window !== 'undefined' ? window.devicePixelRatio || 1 : 1);

  // Full-monitor mode
  let cursorMonitor: number = $state(-1);

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
    const u1 = await listen<{ combo: string }>('hotkey-pressed', (event) => {
      const combo = event.payload.combo;
      if (combo === 'Ctrl+Shift+1') {
        enterCaptureMode('full-monitor');
      } else if (combo === 'Ctrl+Shift+2') {
        enterCaptureMode('area-select');
      }
    });

    const u2 = await listen<{ mode: string }>('capture-mode-started', (event) => {
      mode = event.payload.mode as CaptureMode;
      flowState = 'capturing';
    });

    const u3 = await listen('capture-mode-cancelled', () => {
      mode = null;
      flowState = 'idle';
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

    if (e.key === 'Escape') {
      e.preventDefault();
      if (flowState === 'annotating' || flowState === 'uploading') {
        handleCancel();
      } else if (flowState === 'capturing') {
        cancelCapture();
      }
    } else if (e.key === 'Enter' && mode === 'area-select' && selectionRect) {
      e.preventDefault();
      confirmAreaCapture();
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
        monitors = await invoke<Monitor[]>('get_monitors');
      } catch {
        monitors = [];
      }

      if (monitors.length > 0) {
        let minX = Infinity;
        let minY = Infinity;
        for (const m of monitors) {
          if (m.x < minX) minX = m.x;
          if (m.y < minY) minY = m.y;
        }
        const d = window.devicePixelRatio || 1;
        dpr = d;
        windowOffset = { x: minX / d, y: minY / d };
        monitors = monitors.map(m => ({
          ...m,
          x: m.x / d,
          y: m.y / d,
          width: m.width / d,
          height: m.height / d,
        }));
      }

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
    cursorMonitor = -1;
    selecting = false;
    selStart = { x: 0, y: 0 };
    selCurrent = { x: 0, y: 0 };
    selConfirmed = null;
    capturedImage = null;
    uploading = false;
    uploadUrl = null;
    wasCopied = false;
    currentTool = 'pen';
    currentColor = '#ff0000';
  }

  // ---------------------------------------------------------------------------
  // Full-monitor mode — mouse tracking
  // ---------------------------------------------------------------------------

  function onFullMouseMove(e: MouseEvent) {
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
  // Full-monitor mode — click to capture → annotate
  // ---------------------------------------------------------------------------

  async function onFullClick(_e?: MouseEvent | KeyboardEvent) {
    if (!activeMonitor) return;
    await doFullCapture(activeMonitor.name);
  }

  async function doFullCapture(monitorName: string) {
    try {
      const result = await invoke<CapturedImage>('capture_full_screen', { monitorName });
      transitionToAnnotation(result);
    } catch (err) {
      console.error('capture_full_screen failed:', err);
      await cancelCapture();
    }
  }

  // ---------------------------------------------------------------------------
  // Area-select mode — mouse handlers
  // ---------------------------------------------------------------------------

  function onAreaMouseDown(e: MouseEvent) {
    if (e.button !== 0) return;
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

    const w = Math.abs(selCurrent.x - selStart.x);
    const h = Math.abs(selCurrent.y - selStart.y);
    if (w < 2 || h < 2) {
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
  // Area-select — confirm capture → annotate
  // ---------------------------------------------------------------------------

  async function confirmAreaCapture() {
    if (!selectionRect) return;

    const d = window.devicePixelRatio || 1;
    const x = (selectionRect.left + windowOffset.x) * d;
    const y = (selectionRect.top + windowOffset.y) * d;
    const width = selectionRect.width * d;
    const height = selectionRect.height * d;

    if (width < 2 || height < 2) {
      await cancelCapture();
      return;
    }

    try {
      const result = await invoke<CapturedImage>('capture_area', {
        x: Math.round(x),
        y: Math.round(y),
        width: Math.round(width),
        height: Math.round(height),
      });
      transitionToAnnotation(result);
    } catch (err) {
      console.error('capture_area failed:', err);
      await cancelCapture();
    }
  }

  // ---------------------------------------------------------------------------
  // Post-capture → annotation transition
  // ---------------------------------------------------------------------------

  function transitionToAnnotation(image: CapturedImage) {
    mode = null;          // clear capture mode — we're past capture
    flowState = 'annotating';
    capturedImage = image;
  }

  // ---------------------------------------------------------------------------
  // Annotation action handlers
  // ---------------------------------------------------------------------------

  function exportAnnotatedImage(): string | null {
    return annotationCanvas?.getAnnotatedImage() ?? null;
  }

  async function handleCopy() {
    const pngBase64 = exportAnnotatedImage();
    if (!pngBase64) return;
    try {
      await invoke('copy_to_clipboard', { imageDataBase64: pngBase64 });
    } catch (err) {
      console.error('copy_to_clipboard failed:', err);
    }
    await finishAnnotation();
  }

  async function handleSave() {
    const pngBase64 = exportAnnotatedImage();
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
    const pngBase64 = exportAnnotatedImage();
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
    await finishAnnotation();
  }

  async function finishAnnotation() {
    capturedImage = null;
    uploading = false;
    uploadUrl = null;
    wasCopied = false;
    mode = null;
    await cancelCapture();
  }

  // ---------------------------------------------------------------------------
  // Helpers
  // ---------------------------------------------------------------------------

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
    onmousedown={onAreaMouseDown}
    onmousemove={onAreaMouseMove}
    onmouseup={onAreaMouseUp}
  >
    <div class="dim"></div>

    {#if !selecting && !selectionRect}
      <div
        class="cursor-label"
        style="left: {mousePos.x}px; top: {mousePos.y}px;"
      >
        <span class="crosshair">⌖</span>
        <span class="select-hint">Select area</span>
      </div>
    {/if}

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
          {Math.round(selectionRect.width * dpr)} × {Math.round(selectionRect.height * dpr)}
        </div>
      </div>
    {/if}
  </div>
{/if}

<!-- ========================================================================= -->
<!-- Annotation Mode — captured image + tools                                  -->
<!-- ========================================================================= -->

{#if (flowState === 'annotating' || flowState === 'uploading') && capturedImage}
  <div class="annotation-overlay">
    <!-- Dark backdrop -->
    <div class="annotation-backdrop"></div>

    <div class="annotation-layout">
      <div class="annotation-image-container">
        {#if capturedImage}
          <AnnotationCanvas
            bind:this={annotationCanvas}
            imageData={capturedImage.data}
            imageWidth={capturedImage.width}
            imageHeight={capturedImage.height}
            tool={currentTool}
            color={currentColor}
          />
        {/if}
      </div>

      <Toolbar bind:activeTool={currentTool} bind:color={currentColor} />

      <ActionBar
        onCopy={handleCopy}
        onSave={handleSave}
        onUpload={handleUpload}
        onCancel={handleCancel}
        uploading={uploading}
      />

      <!-- Upload result toast -->
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

  .selection-box {
    position: fixed;
    border: 2px dashed #a78bfa;
    border-radius: 2px;
    background: transparent;
    box-shadow: 0 0 0 9999px rgba(0, 0, 0, 0.55);
    pointer-events: none;
    z-index: 1001;
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

  .annotation-layout {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    max-width: 95vw;
    max-height: 92vh;
    z-index: 1;
  }

  .annotation-image-container {
    position: relative;
    pointer-events: all;
    /* Constrain to viewport with padding for toolbar and action bar */
    max-width: calc(95vw - 120px);
    max-height: calc(92vh - 80px);
    overflow: hidden;
    border-radius: 4px;
    box-shadow: 0 0 40px rgba(0, 0, 0, 0.6);
  }

  .upload-toast {
    position: absolute;
    top: -52px;
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
