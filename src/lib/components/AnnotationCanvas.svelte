<script lang="ts">
  import type { Tool, AnnotationSnapshot } from '$lib/types';

  interface Props {
    /** Base64-encoded RGBA pixel data of the captured image. */
    imageData: string;
    /** Image width in pixels. */
    imageWidth: number;
    /** Image height in pixels. */
    imageHeight: number;
    /** Currently selected annotation tool. */
    tool: Tool;
    /** Current pen/arrow/rect color. */
    color: string;
    /** Callback to notify parent when an undo/redo snapshot is taken. */
    onHistoryChange?: (canUndo: boolean, canRedo: boolean) => void;
  }

  let {
    imageData,
    imageWidth,
    imageHeight,
    tool,
    color,
    onHistoryChange,
  }: Props = $props();

  // ------------------------------------------------------------------
  // Canvas refs & state
  // ------------------------------------------------------------------
  let canvasEl = $state<HTMLCanvasElement | null>(null);
  let ctx = $state<CanvasRenderingContext2D | null>(null);

  let drawing = $state(false);
  let startX = $state(0);
  let startY = $state(0);
  let lastX = $state(0);
  let lastY = $state(0);

  // Text tool state
  let textInput = $state<HTMLInputElement | null>(null);
  let textInputVisible = $state(false);
  let textInputX = $state(0);
  let textInputY = $state(0);
  let textInputValue = $state('');

  // ------------------------------------------------------------------
  // Undo / Redo stack
  // ------------------------------------------------------------------
  const MAX_HISTORY = 50;
  let undoStack = $state<AnnotationSnapshot[]>([]);
  let redoStack = $state<AnnotationSnapshot[]>([]);
  let canUndo = $state(false);
  let canRedo = $state(false);

  // Image element to draw the base capture
  let baseImage = $state<HTMLImageElement | null>(null);

  function updateHistoryState() {
    canUndo = undoStack.length > 0;
    canRedo = redoStack.length > 0;
    onHistoryChange?.(canUndo, canRedo);
  }

  function pushSnapshot() {
    if (!canvasEl) return;
    const dataUrl = canvasEl.toDataURL('image/png');
    undoStack = [...undoStack, { dataUrl }];
    if (undoStack.length > MAX_HISTORY) {
      undoStack = undoStack.slice(undoStack.length - MAX_HISTORY);
    }
    redoStack = [];
    updateHistoryState();
  }

  export function undo() {
    if (undoStack.length === 0 || !canvasEl || !ctx) return;
    // Push current state to redo
    const current = canvasEl.toDataURL('image/png');
    redoStack = [...redoStack, { dataUrl: current }];
    // Pop last undo state
    const snapshot = undoStack[undoStack.length - 1];
    undoStack = undoStack.slice(0, -1);
    restoreSnapshot(snapshot);
    updateHistoryState();
  }

  export function redo() {
    if (redoStack.length === 0 || !canvasEl || !ctx) return;
    // Push current state to undo
    const current = canvasEl.toDataURL('image/png');
    undoStack = [...undoStack, { dataUrl: current }];
    // Pop redo state
    const snapshot = redoStack[redoStack.length - 1];
    redoStack = redoStack.slice(0, -1);
    restoreSnapshot(snapshot);
    updateHistoryState();
  }

  function restoreSnapshot(snapshot: AnnotationSnapshot) {
    if (!canvasEl || !ctx) return;
    const img = new Image();
    img.onload = () => {
      if (!ctx || !canvasEl) return;
      ctx.clearRect(0, 0, canvasEl.width, canvasEl.height);
      ctx.drawImage(img, 0, 0);
    };
    img.src = snapshot.dataUrl;
  }

  // ------------------------------------------------------------------
  // Export annotated image as base64 PNG
  // ------------------------------------------------------------------
  export function getAnnotatedImage(): string | null {
    if (!canvasEl || !baseImage) return null;
    // Create a merged canvas with base image + annotations
    const merged = document.createElement('canvas');
    merged.width = canvasEl.width;
    merged.height = canvasEl.height;
    const mctx = merged.getContext('2d');
    if (!mctx) return null;
    mctx.drawImage(baseImage, 0, 0);
    mctx.drawImage(canvasEl, 0, 0);
    return merged.toDataURL('image/png');
  }

  // ------------------------------------------------------------------
  // Initialisation — load base image onto canvas background
  // ------------------------------------------------------------------
  $effect(() => {
    if (!imageData || !canvasEl) return;
    const img = new Image();
    img.onload = () => {
      baseImage = img;
      canvasEl!.width = imageWidth;
      canvasEl!.height = imageHeight;
      const c = canvasEl!.getContext('2d');
      if (c) {
        ctx = c;
        undoStack = [];
        redoStack = [];
        ctx.drawImage(img, 0, 0);
        pushSnapshot();
      }
    };
    // Convert RGBA base64 to a data URL via a temporary canvas
    // Decode base64 → raw RGBA → putImageData → toDataURL
    const raw = Uint8Array.from(atob(imageData), (c) => c.charCodeAt(0));
    const tmpCanvas = document.createElement('canvas');
    tmpCanvas.width = imageWidth;
    tmpCanvas.height = imageHeight;
    const tmpCtx = tmpCanvas.getContext('2d');
    if (tmpCtx) {
      const imgData = tmpCtx.createImageData(imageWidth, imageHeight);
      imgData.data.set(raw);
      tmpCtx.putImageData(imgData, 0, 0);
      img.src = tmpCanvas.toDataURL('image/png');
    }
  });

  // ------------------------------------------------------------------
  // Drawing helpers
  // ------------------------------------------------------------------
  function getPos(e: MouseEvent | Touch): { x: number; y: number } {
    if (!canvasEl) return { x: 0, y: 0 };
    const rect = canvasEl.getBoundingClientRect();
    const scaleX = canvasEl.width / rect.width;
    const scaleY = canvasEl.height / rect.height;
    return {
      x: (e.clientX - rect.left) * scaleX,
      y: (e.clientY - rect.top) * scaleY,
    };
  }

  function drawPen(fromX: number, fromY: number, toX: number, toY: number) {
    if (!ctx) return;
    ctx.beginPath();
    ctx.moveTo(fromX, fromY);
    ctx.lineTo(toX, toY);
    ctx.strokeStyle = color;
    ctx.lineWidth = 3;
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    ctx.stroke();
  }

  function drawArrowPreview(
    x1: number,
    y1: number,
    x2: number,
    y2: number,
    strokeColor: string,
  ) {
    if (!ctx) return;
    const headLen = 14;
    const angle = Math.atan2(y2 - y1, x2 - x1);

    ctx.beginPath();
    ctx.moveTo(x1, y1);
    ctx.lineTo(x2, y2);
    ctx.strokeStyle = strokeColor;
    ctx.lineWidth = 3;
    ctx.lineCap = 'round';
    ctx.stroke();

    // Arrowhead
    ctx.beginPath();
    ctx.moveTo(x2, y2);
    ctx.lineTo(
      x2 - headLen * Math.cos(angle - Math.PI / 6),
      y2 - headLen * Math.sin(angle - Math.PI / 6),
    );
    ctx.moveTo(x2, y2);
    ctx.lineTo(
      x2 - headLen * Math.cos(angle + Math.PI / 6),
      y2 - headLen * Math.sin(angle + Math.PI / 6),
    );
    ctx.stroke();
  }

  function drawRectPreview(
    x1: number,
    y1: number,
    x2: number,
    y2: number,
    strokeColor: string,
  ) {
    if (!ctx) return;
    ctx.beginPath();
    ctx.strokeStyle = strokeColor;
    ctx.lineWidth = 2;
    ctx.strokeRect(
      Math.min(x1, x2),
      Math.min(y1, y2),
      Math.abs(x2 - x1),
      Math.abs(y2 - y1),
    );
  }

  function applyBlur(x: number, y: number, radius: number) {
    if (!ctx || !canvasEl) return;
    const r = Math.round(radius);
    const sx = Math.round(x - r);
    const sy = Math.round(y - r);
    const sw = r * 2;
    const sh = r * 2;

    // Get the area, pixelate it via downscale+upscale
    const imageData = ctx.getImageData(sx, sy, sw, sh);
    const smallW = Math.max(1, Math.round(sw / 8));
    const smallH = Math.max(1, Math.round(sh / 8));

    const tmpCanvas = document.createElement('canvas');
    tmpCanvas.width = sw;
    tmpCanvas.height = sh;
    const tmpCtx = tmpCanvas.getContext('2d');
    if (!tmpCtx) return;
    tmpCtx.putImageData(imageData, 0, 0);

    const smallCanvas = document.createElement('canvas');
    smallCanvas.width = smallW;
    smallCanvas.height = smallH;
    const smallCtx = smallCanvas.getContext('2d');
    if (!smallCtx) return;
    smallCtx.imageSmoothingEnabled = true;
    smallCtx.drawImage(tmpCanvas, 0, 0, sw, sh, 0, 0, smallW, smallH);

    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(smallCanvas, 0, 0, smallW, smallH, sx, sy, sw, sh);
    ctx.imageSmoothingEnabled = true;
  }

  function applyEraser(x: number, y: number, radius: number) {
    if (!ctx) return;
    ctx.save();
    ctx.globalCompositeOperation = 'destination-out';
    ctx.beginPath();
    ctx.arc(x, y, radius, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
  }

  // ------------------------------------------------------------------
  // Text tool — show and position the text input
  // ------------------------------------------------------------------
  function showTextInput(x: number, y: number) {
    if (textInputVisible) {
      commitText();
    }
    textInputX = x;
    textInputY = y;
    textInputValue = '';
    textInputVisible = true;
    // Focus the input on next tick
    requestAnimationFrame(() => {
      textInput?.focus();
    });
  }

  function commitText() {
    if (!textInputVisible) return;
    if (!ctx || !textInputValue.trim()) {
      textInputVisible = false;
      return;
    }
    pushSnapshot();
    ctx.font = '18px system-ui, sans-serif';
    ctx.fillStyle = color;
    ctx.fillText(textInputValue, textInputX, textInputY);
    textInputVisible = false;
    pushSnapshot();
  }

  // ------------------------------------------------------------------
  // Mouse / Touch event handlers
  // ------------------------------------------------------------------
  function onPointerDown(e: PointerEvent) {
    if (!ctx || !canvasEl) return;
    const pos = getPos(e);
    startX = pos.x;
    startY = pos.y;
    lastX = pos.x;
    lastY = pos.y;

    if (tool === 'text') {
      showTextInput(pos.x, pos.y);
      return;
    }

    drawing = true;
    canvasEl.setPointerCapture(e.pointerId);

    if (tool === 'pen' || tool === 'blur' || tool === 'eraser') {
      redoStack = [];
      updateHistoryState();
      if (tool === 'pen') {
        // Single dot on click
        drawPen(pos.x, pos.y, pos.x, pos.y);
      } else if (tool === 'blur') {
        applyBlur(pos.x, pos.y, 12);
      } else if (tool === 'eraser') {
        applyEraser(pos.x, pos.y, 10);
      }
    }
  }

  function onPointerMove(e: PointerEvent) {
    if (!ctx || !canvasEl || !drawing) return;
    const pos = getPos(e);

    if (tool === 'pen') {
      drawPen(lastX, lastY, pos.x, pos.y);
      lastX = pos.x;
      lastY = pos.y;
    } else if (tool === 'arrow') {
      // Preview: restore snapshot, draw arrow
      if (undoStack.length > 0) {
        const snap = undoStack[undoStack.length - 1];
        restoreSnapshot(snap);
      }
      drawArrowPreview(startX, startY, pos.x, pos.y, color);
    } else if (tool === 'rectangle') {
      if (undoStack.length > 0) {
        const snap = undoStack[undoStack.length - 1];
        restoreSnapshot(snap);
      }
      drawRectPreview(startX, startY, pos.x, pos.y, color);
    } else if (tool === 'blur') {
      applyBlur(pos.x, pos.y, 12);
      lastX = pos.x;
      lastY = pos.y;
    } else if (tool === 'eraser') {
      applyEraser(pos.x, pos.y, 10);
      lastX = pos.x;
      lastY = pos.y;
    }
  }

  function onPointerUp(e: PointerEvent) {
    if (!ctx || !canvasEl || !drawing) return;
    drawing = false;
    canvasEl.releasePointerCapture(e.pointerId);

    if (tool === 'arrow' || tool === 'rectangle' || tool === 'pen' || tool === 'blur' || tool === 'eraser') {
      pushSnapshot();
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'z') {
      e.preventDefault();
      if (e.shiftKey) {
        redo();
      } else {
        undo();
      }
    }
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<div class="canvas-container">
  <canvas
    bind:this={canvasEl}
    class="annotation-canvas"
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    style="cursor: {tool === 'text'
      ? 'text'
      : tool === 'eraser'
        ? 'cell'
        : 'crosshair'}"
  ></canvas>

  {#if textInputVisible}
    {@const leftPct = imageWidth > 0 ? (textInputX / imageWidth) * 100 : 0}
    {@const topPct = imageHeight > 0 ? (textInputY / imageHeight) * 100 : 0}
    <input
      bind:this={textInput}
      class="text-input-overlay"
      style="left: {leftPct}%; top: {topPct}%; color: {color}"
      type="text"
      bind:value={textInputValue}
      onblur={commitText}
      onkeydown={(e) => {
        if (e.key === 'Enter') commitText();
        if (e.key === 'Escape') {
          textInputVisible = false;
        }
      }}
      placeholder="Type..."
    />
  {/if}
</div>

<style>
  .canvas-container {
    position: absolute;
    inset: 0;
    overflow: hidden;
    pointer-events: all;
  }

  .annotation-canvas {
    display: block;
    width: 100%;
    height: 100%;
    image-rendering: auto;
  }

  .text-input-overlay {
    position: absolute;
    background: rgba(0, 0, 0, 0.5);
    border: 1px dashed rgba(255, 255, 255, 0.5);
    border-radius: 2px;
    padding: 2px 4px;
    font: 18px system-ui, sans-serif;
    color: #fff;
    outline: none;
    min-width: 60px;
    z-index: 20;
    transform: translateY(-2px);
  }

  .text-input-overlay::placeholder {
    color: rgba(255, 255, 255, 0.4);
  }
</style>
