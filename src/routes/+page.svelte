<script lang="ts">
  import Overlay from '$lib/components/Overlay.svelte';
</script>

<svelte:window />

<main>
  <h1>Sin Shot</h1>
  <p>Windows screenshot tool — capture, annotate, share.</p>

  <div class="shortcuts">
    <h3>Shortcuts</h3>
    <ul>
      <li><kbd>Ctrl+Shift+1</kbd> — Full monitor capture</li>
      <li><kbd>Ctrl+Shift+2</kbd> — Area select capture</li>
    </ul>
  </div>
</main>

<!-- Capture overlay — always mounted, controls its own visibility -->
<Overlay />

<style>
  main {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100vh;
    font-family: system-ui, sans-serif;
    background: transparent;
    color: #fff;
  }

  .shortcuts {
    margin-top: 2rem;
    font-size: 0.9rem;
    color: #a78bfa;
  }

  .shortcuts ul {
    list-style: none;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  kbd {
    background: rgba(167, 139, 250, 0.15);
    border: 1px solid rgba(167, 139, 250, 0.3);
    border-radius: 4px;
    padding: 1px 6px;
    font-family: 'Cascadia Code', 'Fira Code', monospace;
    font-size: 0.85rem;
    color: #a78bfa;
  }

  :global(body) {
    margin: 0;
    padding: 0;
    overflow: hidden;
    background: #282a36;
  }
</style>
