<script lang="ts">
  import type { Tool } from '$lib/types';

  interface Props {
    activeTool: Tool;
    color: string;
    flashTool?: Tool | null;
  }

  let { activeTool = $bindable(), color = $bindable(), flashTool = null }: Props = $props();

  const tools: { id: Tool; label: string; icon: string }[] = [
    { id: 'pen', label: 'Pen', icon: '✏' },
    { id: 'arrow', label: 'Arrow', icon: '➤' },
    { id: 'rectangle', label: 'Rectangle', icon: '▭' },
    { id: 'text', label: 'Text', icon: 'T' },
    { id: 'blur', label: 'Blur', icon: '◉' },
    { id: 'eraser', label: 'Eraser', icon: '⌫' },
  ];

  const presetColors = [
    '#ff0000', '#ff6600', '#ffcc00', '#00cc00',
    '#0066ff', '#9900cc', '#000000', '#ffffff',
  ];

  function selectTool(tool: Tool) {
    activeTool = tool;
  }

  function pickColor(c: string) {
    color = c;
  }
</script>

<nav class="toolbar" aria-label="Annotation tools">
  {#each tools as tool}
    <button
      class="tool-btn"
      class:active={activeTool === tool.id}
      class:flash={flashTool === tool.id}
      title="{tool.label} ({tool.id[0].toUpperCase()})"
      onclick={() => selectTool(tool.id)}
      aria-pressed={activeTool === tool.id}
    >
      <span class="tool-icon">{tool.icon}</span>
    </button>
  {/each}

  <div class="color-swatches">
    {#each presetColors as c}
      <button
        class="swatch"
        class:selected={color === c}
        style="background: {c}"
        title={c}
        onclick={() => pickColor(c)}
        aria-label="Color {c}"
      ></button>
    {/each}
    <input
      type="color"
      class="color-picker"
      value={color}
      oninput={(e) => pickColor(e.currentTarget.value)}
      aria-label="Custom color"
    />
  </div>
</nav>

<style>
  .toolbar {
    position: absolute;
    right: -52px;
    top: 0;
    bottom: 0;
    width: 48px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 8px 4px;
    z-index: 10;
    pointer-events: all;
  }

  .tool-btn {
    width: 36px;
    height: 36px;
    border: none;
    border-radius: 8px;
    background: rgba(30, 30, 30, 0.75);
    color: #ccc;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 16px;
    transition: background 0.15s, color 0.15s;
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
  }

  .tool-btn:hover {
    background: rgba(50, 50, 50, 0.85);
    color: #fff;
  }

  .tool-btn.active {
    background: rgba(70, 130, 255, 0.7);
    color: #fff;
  }

  .tool-btn.flash {
    animation: tool-flash 0.35s ease-out;
  }

  @keyframes tool-flash {
    0% {
      box-shadow: 0 0 4px 2px rgba(70, 130, 255, 0.9);
      background: rgba(70, 130, 255, 0.85);
    }
    100% {
      box-shadow: 0 0 12px 6px rgba(70, 130, 255, 0);
      background: rgba(70, 130, 255, 0.7);
    }
  }

  .tool-icon {
    line-height: 1;
    pointer-events: none;
  }

  .color-swatches {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 6px 0;
  }

  .swatch {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 2px solid transparent;
    cursor: pointer;
    transition: border-color 0.15s, transform 0.15s;
  }

  .swatch:hover {
    transform: scale(1.15);
  }

  .swatch.selected {
    border-color: #fff;
    box-shadow: 0 0 6px rgba(255, 255, 255, 0.5);
  }

  .color-picker {
    width: 24px;
    height: 24px;
    border: none;
    border-radius: 50%;
    cursor: pointer;
    padding: 0;
    background: transparent;
  }

  .color-picker::-webkit-color-swatch-wrapper {
    padding: 0;
  }

  .color-picker::-webkit-color-swatch {
    border: 2px solid #666;
    border-radius: 50%;
  }
</style>
