<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  // ---------------------------------------------------------------------------
  // Types
  // ---------------------------------------------------------------------------

  interface Settings {
    save_folder: string;
    filename_pattern: string;
    image_format: string;
    jpeg_quality: number;
    start_with_windows: boolean;
    play_sound_on_capture: boolean;
    hotkey_full: string;
    hotkey_area: string;
    server_url: string;
    auto_copy: boolean;
  }

  type Tab = 'general' | 'hotkeys' | 'upload' | 'about';

  interface HotkeyEntry {
    id: string;
    label: string;
    combo: string;
    recording: boolean;
  }

  // ---------------------------------------------------------------------------
  // State (Svelte 5 runes)
  // ---------------------------------------------------------------------------

  let activeTab: Tab = $state('general');
  let settings: Settings = $state({
    save_folder: '',
    filename_pattern: 'screenshot_{date}_{time}',
    image_format: 'png',
    jpeg_quality: 85,
    start_with_windows: false,
    play_sound_on_capture: false,
    hotkey_full: 'Ctrl+Shift+1',
    hotkey_area: 'Ctrl+Shift+2',
    server_url: 'https://screenshots.sinister.ovh/api/upload',
    auto_copy: true,
  });

  let hotkeys: HotkeyEntry[] = $state([
    { id: 'capture_full', label: 'Capture Full Screen', combo: 'Ctrl+Shift+1', recording: false },
    { id: 'capture_area', label: 'Capture Area', combo: 'Ctrl+Shift+2', recording: false },
  ]);

  let toastMessage = $state('');
  let toastVisible = $state(false);

  // ---------------------------------------------------------------------------
  // Toast helper
  // ---------------------------------------------------------------------------

  const TOAST_DURATION = 2200;
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

  function showToast(msg: string) {
    toastMessage = msg;
    toastVisible = true;
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toastVisible = false;
    }, TOAST_DURATION);
  }

  // ---------------------------------------------------------------------------
  // Load settings on mount
  // ---------------------------------------------------------------------------

  onMount(async () => {
    try {
      const loaded = await invoke<Settings>('get_settings');
      settings = loaded;
    } catch (e) {
      console.error('Failed to load settings:', e);
    }
    try {
      const raw: string[] = await invoke('get_hotkeys');
      // Map loaded combos to our known hotkey entries
      if (raw.length >= 1) hotkeys[0].combo = raw[0];
      if (raw.length >= 2) hotkeys[1].combo = raw[1];
    } catch (e) {
      console.error('Failed to load hotkeys:', e);
    }
  });

  // ---------------------------------------------------------------------------
  // Persist settings
  // ---------------------------------------------------------------------------

  async function handleSave() {
    try {
      await invoke('save_settings', { settings });
      showToast('Settings saved');
    } catch (e) {
      console.error('Failed to save settings:', e);
      showToast('Error saving settings');
    }
  }

  // ---------------------------------------------------------------------------
  // Hotkey recording
  // ---------------------------------------------------------------------------

  function startRecording(entry: HotkeyEntry) {
    // If already recording, stop
    if (entry.recording) {
      entry.recording = false;
      return;
    }
    // Stop any other recording
    for (const h of hotkeys) {
      h.recording = false;
    }
    entry.recording = true;
  }

  function handleHotkeyInput(e: KeyboardEvent, entry: HotkeyEntry) {
    if (!entry.recording) return;
    e.preventDefault();
    e.stopPropagation();

    // Ignore lone modifier presses
    if (['Control', 'Shift', 'Alt'].includes(e.key)) return;

    const parts: string[] = [];
    if (e.ctrlKey) parts.push('Ctrl');
    if (e.shiftKey) parts.push('Shift');
    if (e.altKey) parts.push('Alt');

    // Normalise key name
    let key = e.key;
    if (key === ' ') key = 'Space';
    else if (key.length === 1) key = key.toUpperCase();
    else if (key.startsWith('Arrow')) key = key.slice(5);
    else if (key === 'Escape') key = 'Esc';
    else if (key === 'Backspace') key = 'Back';

    parts.push(key);

    const combo = parts.join('+');
    const oldCombo = entry.combo;
    entry.combo = combo;
    entry.recording = false;

    handleHotkeyChange(entry, oldCombo);
  }

  async function handleHotkeyChange(entry: HotkeyEntry, oldCombo: string) {
    // Check for cross-action collisions
    for (const h of hotkeys) {
      if (h.id !== entry.id && h.combo === entry.combo) {
        showToast(`Combo ${entry.combo} already used by "${h.label}"`);
        entry.combo = oldCombo;
        return;
      }
    }
    try {
      // Update settings (save_settings handles unregister/reregister as sole owner)
      if (entry.id === 'capture_full') {
        settings.hotkey_full = entry.combo;
      } else if (entry.id === 'capture_area') {
        settings.hotkey_area = entry.combo;
      }
      await invoke('save_settings', { settings });

      showToast(`Hotkey "${entry.label}" updated to ${entry.combo}`);
    } catch (e) {
      console.error('Failed to update hotkey:', e);
      entry.combo = oldCombo;
      if (entry.id === 'capture_full') {
        settings.hotkey_full = oldCombo;
      } else if (entry.id === 'capture_area') {
        settings.hotkey_area = oldCombo;
      }
      showToast('Failed to update hotkey');
    }
  }

  function captureHotkeyClick(entry: HotkeyEntry) {
    startRecording(entry);
  }

  // ---------------------------------------------------------------------------
  // Hold Ctrl+S to save
  // ---------------------------------------------------------------------------

  function onGlobalKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 's') {
      e.preventDefault();
      handleSave();
    }
  }

  // ---------------------------------------------------------------------------
  // Tab list
  // ---------------------------------------------------------------------------

  const tabs: { id: Tab; label: string }[] = [
    { id: 'general', label: 'General' },
    { id: 'hotkeys', label: 'Hotkeys' },
    { id: 'upload', label: 'Upload' },
    { id: 'about', label: 'About' },
  ];

  // ---------------------------------------------------------------------------
  // Filename pattern tokens
  // ---------------------------------------------------------------------------

  const filenameTokens = [
    { token: '{date}', desc: 'YYYY-MM-DD' },
    { token: '{time}', desc: 'HHmmss' },
    { token: '{random}', desc: '6-char hex' },
  ];
</script>

<svelte:window on:keydown={onGlobalKeydown} />

<!-- ========================================================================= -->
<!-- Toast notification                                                       -->
<!-- ========================================================================= -->
{#if toastVisible}
  <div class="toast" class:visible={toastVisible}>{toastMessage}</div>
{/if}

<!-- ========================================================================= -->
<!-- Chrome                                                                   -->
<!-- ========================================================================= -->
<div class="settings-container">

  <!-- Header -->
  <header class="settings-header">
    <h1 class="settings-title">Settings</h1>
    <button class="save-btn" onclick={handleSave}>
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
        <path d="M2 2h8l4 4v8a1 1 0 01-1 1H3a1 1 0 01-1-1V3a1 1 0 011-1z" stroke="currentColor" stroke-width="1.5" fill="none"/>
        <path d="M4 13V8h6v5" stroke="currentColor" stroke-width="1.5" fill="none"/>
      </svg>
      Save
    </button>
  </header>

  <!-- Tab bar -->
  <div class="tab-bar" role="tablist">
    {#each tabs as tab}
      <button
        class="tab-btn"
        class:active={activeTab === tab.id}
        role="tab"
        aria-selected={activeTab === tab.id}
        onclick={() => (activeTab = tab.id)}
      >
        {tab.label}
      </button>
    {/each}
  </div>

  <!-- Tab panels -->
  <div class="tab-panel" role="tabpanel">

    <!-- ================================================================== -->
    <!-- GENERAL TAB                                                       -->
    <!-- ================================================================== -->
    {#if activeTab === 'general'}
      <div class="settings-group">
        <h2 class="group-title">Save Location</h2>

        <div class="field">
          <label class="field-label" for="save-folder">Default Save Folder</label>
          <div class="field-row">
            <input
              id="save-folder"
              type="text"
              class="text-input mono"
              bind:value={settings.save_folder}
              placeholder="C:\Users\...\Pictures\Sin Shot"
            />
          </div>
        </div>

        <div class="field">
          <label class="field-label" for="filename-pattern">Filename Pattern</label>
          <input
            id="filename-pattern"
            type="text"
            class="text-input mono"
            bind:value={settings.filename_pattern}
          />
          <div class="token-hints">
            {#each filenameTokens as t}
              <span class="token-badge" title={t.desc}>{t.token}</span>
            {/each}
          </div>
        </div>
      </div>

      <div class="settings-group">
        <h2 class="group-title">Image Format</h2>

        <div class="field">
          <label class="field-label" for="image-format">Default Format</label>
          <select id="image-format" class="select-input" bind:value={settings.image_format}>
            <option value="png">PNG</option>
          </select>
          <p class="field-hint">PNG is currently the only supported export format.</p>
        </div>

        {#if settings.image_format === 'jpeg'}
          <div class="field">
            <label class="field-label" for="jpeg-quality-slider">
              JPEG Quality: <span class="quality-value">{settings.jpeg_quality}</span>
            </label>
            <div class="slider-row">
              <span class="slider-label">60</span>
              <input
                id="jpeg-quality-slider"
                type="range"
                min="60"
                max="100"
                step="1"
                class="slider"
                bind:value={settings.jpeg_quality}
              />
              <span class="slider-label">100</span>
            </div>
          </div>
        {/if}
      </div>

      <div class="settings-group">
        <h2 class="group-title">Options</h2>

        <div class="toggle-list">
          <label class="toggle-row">
            <span class="toggle-text">
              <span class="toggle-title">Start with Windows</span>
              <span class="toggle-desc">Launch Sin Shot automatically when you log in</span>
            </span>
            <input type="checkbox" class="toggle-check" bind:checked={settings.start_with_windows} />
          </label>

          <label class="toggle-row">
            <span class="toggle-text">
              <span class="toggle-title">Play sound on capture</span>
              <span class="toggle-desc">Camera shutter sound when screenshot is taken</span>
            </span>
            <input type="checkbox" class="toggle-check" bind:checked={settings.play_sound_on_capture} />
          </label>

        </div>
      </div>

    <!-- ================================================================== -->
    <!-- HOTKEYS TAB                                                       -->
    <!-- ================================================================== -->
    {:else if activeTab === 'hotkeys'}
      <div class="settings-group">
        <h2 class="group-title">Capture Hotkeys</h2>
        <p class="group-desc">Click a field and press your desired key combination.</p>

        <div class="hotkey-list">
          {#each hotkeys as entry, i}
            <div class="hotkey-entry">
              <span class="hotkey-label">{entry.label}</span>
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                class="hotkey-field"
                class:recording={entry.recording}
                class:occupied={!!entry.combo}
                onclick={() => captureHotkeyClick(entry)}
                onkeydown={(e: KeyboardEvent) => handleHotkeyInput(e, entry)}
                tabindex="0"
                role="button"
                aria-label="Record hotkey for {entry.label}"
              >
                {#if entry.recording}
                  <span class="recording-pulse">Press keys…</span>
                {:else if entry.combo}
                  {#each entry.combo.split('+') as part}
                    <kbd class="keycap">{part}</kbd>
                  {/each}
                {:else}
                  <span class="placeholder">Click to record</span>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      </div>

    <!-- ================================================================== -->
    <!-- UPLOAD TAB                                                        -->
    <!-- ================================================================== -->
    {:else if activeTab === 'upload'}
      <div class="settings-group">
        <h2 class="group-title">Upload Server</h2>

        <div class="field">
          <label class="field-label" for="server-url">Server URL</label>
          <input
            id="server-url"
            type="text"
            class="text-input mono"
            bind:value={settings.server_url}
            placeholder="https://screenshots.sinister.ovh/api/upload"
          />
        </div>

        <label class="toggle-row">
          <span class="toggle-text">
            <span class="toggle-title">Auto-copy URL</span>
            <span class="toggle-desc">Copy share link to clipboard immediately after upload</span>
          </span>
          <input type="checkbox" class="toggle-check" bind:checked={settings.auto_copy} />
        </label>
      </div>

    <!-- ================================================================== -->
    <!-- ABOUT TAB                                                         -->
    <!-- ================================================================== -->
    {:else if activeTab === 'about'}
      <div class="settings-group about-section">
        <div class="about-icon">
          <svg width="48" height="48" viewBox="0 0 48 48" fill="none">
            <rect x="4" y="8" width="40" height="32" rx="4" stroke="currentColor" stroke-width="2" fill="none"/>
            <path d="M16 28l6-8 4 4 6-10" stroke="currentColor" stroke-width="2" fill="none" stroke-linecap="round" stroke-linejoin="round"/>
            <circle cx="14" cy="16" r="2" fill="currentColor"/>
          </svg>
        </div>
        <h2 class="about-name">Sin Shot</h2>
        <p class="about-version">Version 0.1.0</p>
        <p class="about-desc">
          Windows screenshot tool — capture, annotate, and share instantly.
          Built with Tauri + Svelte.
        </p>
        <a
          class="about-link"
          href="https://github.com/sinister/sin-shot"
          target="_blank"
          rel="noopener noreferrer"
        >
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8 0C3.58 0 0 3.58 0 8a8 8 0 005.47 7.59c.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82a7.64 7.64 0 014 0c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0016 8c0-4.42-3.58-8-8-8z"/>
          </svg>
          github.com/sinister/sin-shot
        </a>
      </div>
    {/if}

  </div> <!-- /.tab-panel -->

  <!-- Hint bar -->
  <div class="hint-bar">
    <span>Ctrl+S to save</span>
  </div>

</div> <!-- /.settings-container -->

<!-- ========================================================================= -->
<!-- Styles — Dracula dark theme                                             -->
<!-- ========================================================================= -->
<style>
  /* -------------------------------------------------------------------- */
  /* CSS custom properties — Dracula palette                              */
  /* -------------------------------------------------------------------- */
  :global(body) {
    --bg: #282a36;
    --bg-light: #2d2f3d;
    --bg-lighter: #343746;
    --line: #44475a;
    --fg: #f8f8f2;
    --fg-dim: #9a9cb5;
    --comment: #6272a4;
    --purple: #bd93f9;
    --purple-dim: #7a5eb5;
    --cyan: #8be9fd;
    --green: #50fa7b;
    --orange: #ffb86c;
    --pink: #ff79c6;
    --red: #ff5555;
    --yellow: #f1fa8c;
    --radius: 8px;
    --radius-sm: 4px;
    --font: system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif;
    --font-mono: 'Cascadia Code', 'JetBrains Mono', 'Fira Code', 'Consolas', monospace;
  }

  /* -------------------------------------------------------------------- */
  /* Layout                                                               */
  /* -------------------------------------------------------------------- */
  .settings-container {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg);
    color: var(--fg);
    font-family: var(--font);
    user-select: none;
    -webkit-user-select: none;
    overflow: hidden;
  }

  /* -------------------------------------------------------------------- */
  /* Header                                                               */
  /* -------------------------------------------------------------------- */
  .settings-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 18px 24px 10px;
    border-bottom: 1px solid var(--line);
    flex-shrink: 0;
  }

  .settings-title {
    font-size: 18px;
    font-weight: 600;
    margin: 0;
    color: var(--fg);
  }

  .save-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 16px;
    font-size: 13px;
    font-weight: 500;
    font-family: var(--font);
    border-radius: var(--radius-sm);
    border: 1px solid var(--purple);
    background: var(--purple);
    color: var(--bg);
    cursor: pointer;
    transition: background 0.15s;
  }
  .save-btn:hover {
    background: var(--purple-dim);
  }

  /* -------------------------------------------------------------------- */
  /* Tab bar                                                              */
  /* -------------------------------------------------------------------- */
  .tab-bar {
    display: flex;
    gap: 0;
    padding: 0 24px;
    border-bottom: 1px solid var(--line);
    flex-shrink: 0;
  }

  .tab-btn {
    padding: 10px 18px;
    font-size: 13px;
    font-weight: 500;
    font-family: var(--font);
    color: var(--fg-dim);
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    cursor: pointer;
    transition: color 0.15s, border-color 0.15s;
    outline: none;
  }
  .tab-btn:hover {
    color: var(--fg);
  }
  .tab-btn.active {
    color: var(--purple);
    border-bottom-color: var(--purple);
  }

  /* -------------------------------------------------------------------- */
  /* Tab panel (scrollable content)                                       */
  /* -------------------------------------------------------------------- */
  .tab-panel {
    flex: 1;
    overflow-y: auto;
    padding: 20px 24px;
  }

  /* -------------------------------------------------------------------- */
  /* Settings groups                                                      */
  /* -------------------------------------------------------------------- */
  .settings-group {
    margin-bottom: 24px;
  }

  .group-title {
    font-size: 13px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--comment);
    margin: 0 0 12px;
  }

  .group-desc {
    font-size: 13px;
    color: var(--fg-dim);
    margin: -8px 0 14px;
  }

  /* -------------------------------------------------------------------- */
  /* Fields                                                               */
  /* -------------------------------------------------------------------- */
  .field {
    margin-bottom: 14px;
  }

  .field-label {
    display: block;
    font-size: 13px;
    font-weight: 500;
    color: var(--fg);
    margin-bottom: 5px;
  }

  .field-row {
    display: flex;
    gap: 8px;
  }

  .text-input {
    width: 100%;
    padding: 8px 10px;
    font-size: 13px;
    font-family: var(--font);
    color: var(--fg);
    background: var(--bg-light);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    outline: none;
    transition: border-color 0.15s;
    box-sizing: border-box;
  }
  .text-input:focus {
    border-color: var(--purple);
  }
  .text-input.mono {
    font-family: var(--font-mono);
    font-size: 12px;
  }



  .select-input {
    width: 100%;
    padding: 8px 10px;
    font-size: 13px;
    font-family: var(--font);
    color: var(--fg);
    background: var(--bg-light);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    outline: none;
    cursor: pointer;
    box-sizing: border-box;
  }
  .select-input:focus {
    border-color: var(--purple);
  }
  .select-input option {
    background: var(--bg);
    color: var(--fg);
  }

  /* -------------------------------------------------------------------- */
  /* Token hints for filename pattern                                     */
  /* -------------------------------------------------------------------- */
  .token-hints {
    display: flex;
    gap: 6px;
    margin-top: 6px;
  }

  .token-badge {
    font-family: var(--font-mono);
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 99px;
    background: var(--bg-lighter);
    color: var(--cyan);
    border: 1px solid var(--line);
    cursor: default;
  }

  /* -------------------------------------------------------------------- */
  /* Slider                                                               */
  /* -------------------------------------------------------------------- */
  .quality-value {
    color: var(--purple);
    font-weight: 600;
  }

  .slider-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .slider-label {
    font-size: 12px;
    color: var(--fg-dim);
    font-family: var(--font-mono);
  }

  .slider {
    flex: 1;
    -webkit-appearance: none;
    appearance: none;
    height: 6px;
    background: var(--line);
    border-radius: 3px;
    outline: none;
  }
  .slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--purple);
    cursor: pointer;
    border: 2px solid var(--bg);
    box-shadow: 0 0 4px rgba(0,0,0,.3);
  }

  /* -------------------------------------------------------------------- */
  /* Toggle rows                                                          */
  /* -------------------------------------------------------------------- */
  .toggle-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background 0.15s;
    gap: 12px;
  }
  .toggle-row:hover {
    background: var(--bg-light);
  }

  .toggle-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .toggle-title {
    font-size: 13px;
    font-weight: 500;
    color: var(--fg);
  }

  .toggle-desc {
    font-size: 11px;
    color: var(--fg-dim);
  }

  /* Toggle switch */
  .toggle-check {
    -webkit-appearance: none;
    appearance: none;
    width: 38px;
    height: 22px;
    border-radius: 11px;
    background: var(--line);
    position: relative;
    cursor: pointer;
    transition: background 0.2s;
    flex-shrink: 0;
    outline: none;
  }
  .toggle-check::after {
    content: '';
    position: absolute;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--fg);
    top: 2px;
    left: 2px;
    transition: transform 0.2s;
    box-shadow: 0 1px 2px rgba(0,0,0,.3);
  }
  .toggle-check:checked {
    background: var(--purple);
  }
  .toggle-check:checked::after {
    transform: translateX(16px);
  }

  /* -------------------------------------------------------------------- */
  /* Hotkey list                                                          */
  /* -------------------------------------------------------------------- */
  .hotkey-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .hotkey-entry {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 8px 0;
  }

  .hotkey-label {
    font-size: 14px;
    font-weight: 500;
    color: var(--fg);
    flex-shrink: 0;
  }

  .hotkey-field {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 8px 14px;
    min-width: 200px;
    justify-content: center;
    border-radius: var(--radius-sm);
    background: var(--bg-light);
    border: 2px solid transparent;
    cursor: pointer;
    transition: border-color 0.15s, background 0.15s;
    outline: none;
    font-family: var(--font-mono);
    font-size: 13px;
  }
  .hotkey-field:hover {
    border-color: var(--line);
  }
  .hotkey-field:focus {
    border-color: var(--purple);
  }
  .hotkey-field.recording {
    border-color: var(--purple);
    background: var(--bg-lighter);
    animation: pulse-border 1.2s infinite;
  }
  .hotkey-field.occupied {
    border-color: var(--line);
  }

  @keyframes pulse-border {
    0%, 100% { border-color: var(--purple); }
    50% { border-color: var(--pink); }
  }

  .recording-pulse {
    color: var(--pink);
    font-weight: 500;
    font-size: 12px;
  }

  .keycap {
    display: inline-block;
    padding: 2px 7px;
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 500;
    color: var(--fg);
    background: var(--bg-lighter);
    border: 1px solid var(--line);
    border-radius: 3px;
    line-height: 1.5;
  }

  .placeholder {
    color: var(--fg-dim);
    font-style: italic;
    font-size: 12px;
  }

  /* -------------------------------------------------------------------- */
  /* About                                                                */
  /* -------------------------------------------------------------------- */
  .about-section {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    padding: 24px 0;
  }

  .about-icon {
    color: var(--purple);
    margin-bottom: 16px;
  }

  .about-name {
    font-size: 22px;
    font-weight: 700;
    margin: 0 0 4px;
    color: var(--fg);
  }

  .about-version {
    font-size: 13px;
    color: var(--fg-dim);
    margin: 0 0 16px;
    font-family: var(--font-mono);
  }

  .about-desc {
    font-size: 14px;
    color: var(--fg-dim);
    max-width: 340px;
    line-height: 1.5;
    margin: 0 0 20px;
  }

  .about-link {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    font-family: var(--font-mono);
    color: var(--cyan);
    text-decoration: none;
    padding: 8px 16px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line);
    transition: background 0.15s, border-color 0.15s;
  }
  .about-link:hover {
    background: var(--bg-lighter);
    border-color: var(--cyan);
  }

  /* -------------------------------------------------------------------- */
  /* Hint bar                                                             */
  /* -------------------------------------------------------------------- */
  .hint-bar {
    padding: 8px 24px;
    font-size: 11px;
    color: var(--comment);
    border-top: 1px solid var(--line);
    flex-shrink: 0;
    display: flex;
    justify-content: flex-end;
  }

  /* -------------------------------------------------------------------- */
  /* Toast                                                                */
  /* -------------------------------------------------------------------- */
  .toast {
    position: fixed;
    bottom: 24px;
    left: 50%;
    transform: translateX(-50%) translateY(20px);
    padding: 10px 24px;
    background: var(--bg-lighter);
    color: var(--fg);
    font-size: 13px;
    font-weight: 500;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    box-shadow: 0 8px 24px rgba(0,0,0,.4);
    opacity: 0;
    transition: opacity 0.2s, transform 0.25s ease-out;
    pointer-events: none;
    z-index: 1000;
  }
  .toast.visible {
    opacity: 1;
    transform: translateX(-50%) translateY(0);
  }
</style>
