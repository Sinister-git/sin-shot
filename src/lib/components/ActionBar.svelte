<script lang="ts">
  interface Props {
    onCopy: () => void;
    onSave: () => void;
    onUpload: () => void;
    onCancel: () => void;
    uploading: boolean;
  }

  let { onCopy, onSave, onUpload, onCancel, uploading = false }: Props = $props();
</script>

<div class="action-bar" role="toolbar" aria-label="Capture actions">
  <button class="action-btn copy" onclick={onCopy} title="Copy to clipboard">
    <span class="icon">📋</span>
    <span class="label">Copy</span>
  </button>
  <button class="action-btn save" onclick={onSave} title="Save to folder">
    <span class="icon">💾</span>
    <span class="label">Save</span>
  </button>
  <button
    class="action-btn upload"
    onclick={onUpload}
    disabled={uploading}
    title="Upload to sinister.ovh"
  >
    <span class="icon">{uploading ? '⏳' : '☁️'}</span>
    <span class="label">{uploading ? 'Uploading...' : 'Upload'}</span>
  </button>
  <button class="action-btn cancel" onclick={onCancel} title="Discard">
    <span class="icon">✕</span>
    <span class="label">Cancel</span>
  </button>
</div>

<style>
  .action-bar {
    position: absolute;
    bottom: -52px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    gap: 8px;
    z-index: 10;
    pointer-events: all;
  }

  .action-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 8px 16px;
    border: none;
    border-radius: 8px;
    background: rgba(30, 30, 30, 0.8);
    color: #ddd;
    font-size: 14px;
    cursor: pointer;
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    transition: background 0.15s, color 0.15s;
    white-space: nowrap;
  }

  .action-btn:hover:not(:disabled) {
    background: rgba(50, 50, 50, 0.9);
    color: #fff;
  }

  .action-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .action-btn.cancel:hover:not(:disabled) {
    background: rgba(200, 50, 50, 0.7);
  }

  .action-btn.upload:hover:not(:disabled) {
    background: rgba(50, 120, 220, 0.7);
  }

  .icon {
    font-size: 16px;
    line-height: 1;
    pointer-events: none;
  }

  .label {
    pointer-events: none;
  }
</style>
