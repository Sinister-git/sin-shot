<script module lang="ts">
  // Google Identity Services types (loaded from CDN at runtime)
  declare const google: {
    accounts: {
      id: {
        initialize: (config: { client_id: string; callback: (resp: { credential: string }) => void }) => void;
        renderButton: (el: HTMLElement | null, options: Record<string, unknown>) => void;
        disableAutoSelect: () => void;
      };
    };
  };
</script>

<script lang="ts">
  import { onMount } from 'svelte';
  import {
    fetchGallery,
    deleteImage,
    saveToken,
    clearToken,
    getGoogleClientId,
    formatBytes,
    formatDate,
    type GalleryImage,
  } from '$lib/gallery-api';

  // ── Auth state ──────────────────────────────────────────────────────

  let isSignedIn = $state(false);
  let userName = $state('');
  let userAvatar = $state('');
  let authError = $state('');
  let gisReady = $state(false);

  // ── Gallery state ───────────────────────────────────────────────────

  let images: GalleryImage[] = $state([]);
  let loading = $state(true);
  let error = $state('');

  // ── Search / filter ─────────────────────────────────────────────────

  let searchQuery = $state('');
  let sortBy = $state<'newest' | 'oldest' | 'name' | 'size'>('newest');

  // ── Toast ───────────────────────────────────────────────────────────

  let toastMessage = $state('');
  let toastVisible = $state(false);
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

  function showToast(msg: string) {
    toastMessage = msg;
    toastVisible = true;
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toastVisible = false;
    }, 2500);
  }

  // ── Google Sign-In ──────────────────────────────────────────────────

  function initGoogleSignIn() {
    const clientId = getGoogleClientId();
    if (!clientId) {
      authError = 'Google Sign-In is not configured (missing GOOGLE_CLIENT_ID).';
      return;
    }

    // Load the GIS library dynamically
    const script = document.createElement('script');
    script.src = 'https://accounts.google.com/gsi/client';
    script.async = true;
    script.onload = () => {
      try {
        google.accounts.id.initialize({
          client_id: clientId,
          callback: handleCredentialResponse,
        });
        gisReady = true;

        // Render the button into our container
        google.accounts.id.renderButton(
          document.getElementById('google-signin-btn'),
          {
            theme: 'filled_black',
            size: 'large',
            text: 'signin_with',
            shape: 'rectangular',
            width: 280,
          }
        );
      } catch (e) {
        authError = `Failed to initialize Google Sign-In: ${e}`;
      }
    };
    script.onerror = () => {
      authError = 'Failed to load Google Sign-In library.';
    };
    document.head.appendChild(script);
  }

  // Sign In With Google callback — receives an ID token
  function handleCredentialResponse(response: { credential: string }) {
    const idToken = response.credential;
    saveToken(idToken);

    // Decode the JWT payload to get user info
    try {
      const payload = JSON.parse(atob(idToken.split('.')[1]));
      userName = payload.name || payload.email || 'User';
      userAvatar = payload.picture || '';
    } catch {
      userName = 'User';
    }

    isSignedIn = true;
    authError = '';
    loadImages();
  }

  function signOut() {
    clearToken();
    isSignedIn = false;
    userName = '';
    userAvatar = '';
    images = [];
    // Revoke Google session
    try {
      google.accounts.id.disableAutoSelect();
    } catch {
      // GIS may not be loaded
    }
  }

  // ── Data fetching ───────────────────────────────────────────────────

  async function loadImages() {
    loading = true;
    error = '';
    try {
      images = await fetchGallery();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load gallery';
    } finally {
      loading = false;
    }
  }

  // ── Actions ─────────────────────────────────────────────────────────

  async function handleDelete(img: GalleryImage) {
    if (!confirm(`Delete ${img.filename}? This cannot be undone.`)) return;
    try {
      await deleteImage(img.key);
      images = images.filter((i) => i.key !== img.key);
      showToast('Image deleted');
    } catch (e) {
      showToast(e instanceof Error ? e.message : 'Failed to delete');
    }
  }

  async function handleCopyLink(img: GalleryImage) {
    try {
      await navigator.clipboard.writeText(img.url);
      showToast('Link copied!');
    } catch {
      // Fallback for older browsers
      const ta = document.createElement('textarea');
      ta.value = img.url;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand('copy');
      document.body.removeChild(ta);
      showToast('Link copied!');
    }
  }

  // ── Filtered & sorted images ────────────────────────────────────────

  const filteredImages = $derived.by(() => {
    let result = [...images];

    // Search filter
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase().trim();
      result = result.filter(
        (img) =>
          img.filename.toLowerCase().includes(q) ||
          img.key.toLowerCase().includes(q) ||
          formatDate(img.uploaded_at).toLowerCase().includes(q)
      );
    }

    // Sort
    switch (sortBy) {
      case 'newest':
        result.sort((a, b) => b.uploaded_at.localeCompare(a.uploaded_at));
        break;
      case 'oldest':
        result.sort((a, b) => a.uploaded_at.localeCompare(b.uploaded_at));
        break;
      case 'name':
        result.sort((a, b) => a.filename.localeCompare(b.filename));
        break;
      case 'size':
        result.sort((a, b) => b.size_bytes - a.size_bytes);
        break;
    }

    return result;
  });

  // ── Init ────────────────────────────────────────────────────────────

  onMount(() => {
    // Check if already signed in (token in localStorage)
    const token = typeof localStorage !== 'undefined' ? localStorage.getItem('gallery_id_token') : null;
    if (token) {
      // Decode to check expiry
      try {
        const payload = JSON.parse(atob(token.split('.')[1]));
        const exp = payload.exp * 1000;
        if (Date.now() < exp) {
          userName = payload.name || payload.email || 'User';
          userAvatar = payload.picture || '';
          isSignedIn = true;
          loadImages();
          return;
        }
      } catch {
        // Token invalid, proceed to sign-in
      }
      clearToken();
    }
    initGoogleSignIn();
  });
</script>

<svelte:head>
  <title>Sin Shot — Gallery</title>
</svelte:head>

<!-- ========================================================================= -->
<!-- Not signed in — show sign-in screen                                     -->
<!-- ========================================================================= -->
{#if !isSignedIn}
  <div class="auth-screen">
    <div class="auth-card">
      <div class="auth-logo">
        <svg width="64" height="64" viewBox="0 0 48 48" fill="none">
          <rect x="4" y="8" width="40" height="32" rx="4" stroke="currentColor" stroke-width="2" fill="none"/>
          <path d="M16 28l6-8 4 4 6-10" stroke="currentColor" stroke-width="2" fill="none" stroke-linecap="round" stroke-linejoin="round"/>
          <circle cx="14" cy="16" r="2" fill="currentColor"/>
        </svg>
      </div>
      <h1 class="auth-title">Sin Shot Gallery</h1>
      <p class="auth-desc">Sign in with your Google account to browse your screenshots.</p>

      {#if authError}
        <div class="auth-error">{authError}</div>
      {/if}

      <!-- Google Sign-In button rendered by GIS -->
      <div id="google-signin-btn" class="google-btn-wrapper"></div>

      {#if !gisReady && !authError}
        <button class="signin-btn" disabled>
          <div class="spinner-sm"></div>
          Loading sign-in…
        </button>
      {/if}
    </div>
  </div>

<!-- ========================================================================= -->
<!-- Signed in — show gallery                                                -->
<!-- ========================================================================= -->
{:else}
  <div class="gallery-container">
    <!-- Header -->
    <header class="gallery-header">
      <div class="header-left">
        <svg width="28" height="28" viewBox="0 0 48 48" fill="none" class="header-logo">
          <rect x="4" y="8" width="40" height="32" rx="4" stroke="currentColor" stroke-width="2" fill="none"/>
          <path d="M16 28l6-8 4 4 6-10" stroke="currentColor" stroke-width="2" fill="none" stroke-linecap="round" stroke-linejoin="round"/>
          <circle cx="14" cy="16" r="2" fill="currentColor"/>
        </svg>
        <h1 class="header-title">Gallery</h1>
        <span class="image-count">{images.length} screenshot{images.length !== 1 ? 's' : ''}</span>
      </div>

      <div class="header-right">
        <div class="search-box">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="search-icon">
            <circle cx="11" cy="11" r="8"/>
            <path d="m21 21-4.35-4.35"/>
          </svg>
          <input
            type="text"
            class="search-input"
            placeholder="Search screenshots…"
            bind:value={searchQuery}
          />
        </div>

        <select class="sort-select" bind:value={sortBy}>
          <option value="newest">Newest first</option>
          <option value="oldest">Oldest first</option>
          <option value="name">By name</option>
          <option value="size">By size</option>
        </select>

        <button class="refresh-btn" onclick={loadImages} disabled={loading}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class:spin={loading}>
            <path d="M21 2v6h-6M3 12a9 9 0 0115.36-6.36L21 8M3 22v-6h6M21 12a9 9 0 01-15.36 6.36L3 16"/>
          </svg>
          Refresh
        </button>

        <!-- User pill -->
        <div class="user-pill">
          {#if userAvatar}
            <img src={userAvatar} alt="" class="user-avatar" />
          {/if}
          <span class="user-name">{userName}</span>
          <button class="signout-btn" onclick={signOut} title="Sign out">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M9 21H5a2 2 0 01-2-2V5a2 2 0 012-2h4M16 17l5-5-5-5M21 12H9"/>
            </svg>
          </button>
        </div>
      </div>
    </header>

    <!-- Error banner -->
    {#if error}
      <div class="error-banner">
        <span>{error}</span>
        <button onclick={loadImages}>Retry</button>
      </div>
    {/if}

    <!-- Content -->
    <main class="gallery-content">
      {#if loading}
        <div class="loading-state">
          <div class="spinner"></div>
          <span>Loading screenshots…</span>
        </div>
      {:else if filteredImages.length === 0}
        <div class="empty-state">
          {#if searchQuery}
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
              <circle cx="11" cy="11" r="8"/>
              <path d="m21 21-4.35-4.35"/>
            </svg>
            <h2>No matches</h2>
            <p>No screenshots match "{searchQuery}". Try a different search.</p>
          {:else}
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
              <rect x="3" y="3" width="18" height="18" rx="2"/>
              <circle cx="8.5" cy="8.5" r="1.5"/>
              <path d="m21 15-5-5L5 21"/>
            </svg>
            <h2>No screenshots yet</h2>
            <p>Uploaded screenshots from the Sin Shot desktop app will appear here.</p>
          {/if}
        </div>
      {:else}
        <!-- Grid -->
        <div class="image-grid">
          {#each filteredImages as img (img.key)}
            <div class="image-card">
              <a href={img.url} target="_blank" rel="noopener noreferrer" class="image-link">
                <div class="image-preview">
                  <img
                    src={img.url}
                    alt={img.filename}
                    loading="lazy"
                  />
                </div>
              </a>
              <div class="image-info">
                <div class="image-meta">
                  <span class="image-date">{formatDate(img.uploaded_at)}</span>
                  <span class="image-size">{formatBytes(img.size_bytes)}</span>
                </div>
                <div class="image-url-row">
                  <code class="image-url">{img.url}</code>
                </div>
                <div class="image-actions">
                  <button class="action-btn copy-btn" onclick={() => handleCopyLink(img)} title="Copy link">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <rect x="9" y="9" width="13" height="13" rx="2"/>
                      <path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/>
                    </svg>
                    Copy
                  </button>
                  <button class="action-btn delete-btn" onclick={() => handleDelete(img)} title="Delete">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <path d="M3 6h18M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2"/>
                      <line x1="10" y1="11" x2="10" y2="17"/>
                      <line x1="14" y1="11" x2="14" y2="17"/>
                    </svg>
                    Delete
                  </button>
                </div>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </main>
  </div>
{/if}

<!-- Toast notification -->
{#if toastVisible}
  <div class="toast" class:visible={toastVisible}>{toastMessage}</div>
{/if}

<!-- ========================================================================= -->
<!-- Styles — Dracula dark theme                                             -->
<!-- ========================================================================= -->
<style>
  /* ── CSS custom properties (Dracula palette) ──────────────────────── */
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
    margin: 0;
    padding: 0;
    background: var(--bg);
    color: var(--fg);
  }

  /* ── Auth screen ──────────────────────────────────────────────────── */
  .auth-screen {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 100vh;
    background: var(--bg);
    font-family: var(--font);
  }

  .auth-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 48px 40px;
    background: var(--bg-light);
    border: 1px solid var(--line);
    border-radius: 12px;
    max-width: 400px;
    width: 100%;
    text-align: center;
  }

  .auth-logo {
    color: var(--purple);
    margin-bottom: 20px;
  }

  .auth-title {
    font-size: 24px;
    font-weight: 700;
    margin: 0 0 8px;
    color: var(--fg);
  }

  .auth-desc {
    font-size: 14px;
    color: var(--fg-dim);
    margin: 0 0 28px;
    line-height: 1.5;
  }

  .auth-error {
    background: rgba(255, 85, 85, 0.1);
    border: 1px solid rgba(255, 85, 85, 0.3);
    border-radius: var(--radius-sm);
    padding: 10px 14px;
    font-size: 13px;
    color: var(--red);
    margin-bottom: 16px;
    width: 100%;
    box-sizing: border-box;
  }

  .google-btn-wrapper {
    margin-bottom: 16px;
  }

  .signin-btn {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    padding: 10px 24px;
    font-size: 14px;
    font-weight: 500;
    font-family: var(--font);
    border-radius: var(--radius-sm);
    border: 1px solid var(--line);
    background: var(--bg-lighter);
    color: var(--fg-dim);
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
  }
  .signin-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .spinner-sm {
    width: 16px;
    height: 16px;
    border: 2px solid var(--line);
    border-top-color: var(--purple);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    display: inline-block;
  }

  /* ── Gallery container ────────────────────────────────────────────── */
  .gallery-container {
    display: flex;
    flex-direction: column;
    min-height: 100vh;
    font-family: var(--font);
    background: var(--bg);
  }

  /* ── Header ───────────────────────────────────────────────────────── */
  .gallery-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px;
    padding: 14px 24px;
    background: var(--bg-light);
    border-bottom: 1px solid var(--line);
    position: sticky;
    top: 0;
    z-index: 10;
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .header-logo {
    color: var(--purple);
    flex-shrink: 0;
  }

  .header-title {
    font-size: 18px;
    font-weight: 600;
    margin: 0;
    color: var(--fg);
  }

  .image-count {
    font-size: 12px;
    color: var(--comment);
    font-family: var(--font-mono);
    background: var(--bg-lighter);
    padding: 3px 10px;
    border-radius: 99px;
  }

  .header-right {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .search-box {
    position: relative;
    display: flex;
    align-items: center;
  }

  .search-icon {
    position: absolute;
    left: 10px;
    color: var(--comment);
    pointer-events: none;
  }

  .search-input {
    padding: 7px 12px 7px 32px;
    font-size: 13px;
    font-family: var(--font);
    color: var(--fg);
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    outline: none;
    width: 200px;
    transition: border-color 0.15s, width 0.2s;
    box-sizing: border-box;
  }
  .search-input:focus {
    border-color: var(--purple);
    width: 260px;
  }
  .search-input::placeholder {
    color: var(--comment);
  }

  .sort-select {
    padding: 7px 10px;
    font-size: 13px;
    font-family: var(--font);
    color: var(--fg);
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    outline: none;
    cursor: pointer;
  }
  .sort-select:focus {
    border-color: var(--purple);
  }
  .sort-select option {
    background: var(--bg-light);
    color: var(--fg);
  }

  .refresh-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 14px;
    font-size: 13px;
    font-family: var(--font);
    font-weight: 500;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line);
    background: var(--bg);
    color: var(--fg-dim);
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }
  .refresh-btn:hover:not(:disabled) {
    background: var(--bg-lighter);
    color: var(--fg);
  }
  .refresh-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .spin {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  /* ── User pill ────────────────────────────────────────────────────── */
  .user-pill {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 8px 4px 4px;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: 99px;
  }

  .user-avatar {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    object-fit: cover;
  }

  .user-name {
    font-size: 13px;
    font-weight: 500;
    color: var(--fg);
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .signout-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 4px;
    background: transparent;
    border: none;
    color: var(--fg-dim);
    cursor: pointer;
    border-radius: 50%;
    transition: color 0.15s, background 0.15s;
  }
  .signout-btn:hover {
    color: var(--red);
    background: rgba(255, 85, 85, 0.1);
  }

  /* ── Error banner ─────────────────────────────────────────────────── */
  .error-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 24px;
    background: rgba(255, 85, 85, 0.08);
    border-bottom: 1px solid rgba(255, 85, 85, 0.2);
    font-size: 13px;
    color: var(--red);
  }

  .error-banner button {
    padding: 4px 12px;
    font-size: 12px;
    font-family: var(--font);
    font-weight: 500;
    background: var(--red);
    color: var(--bg);
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  /* ── Content ──────────────────────────────────────────────────────── */
  .gallery-content {
    flex: 1;
    padding: 24px;
    overflow-y: auto;
  }

  /* ── Loading ──────────────────────────────────────────────────────── */
  .loading-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    padding: 80px 0;
    color: var(--fg-dim);
    font-size: 14px;
  }

  .spinner {
    width: 32px;
    height: 32px;
    border: 3px solid var(--line);
    border-top-color: var(--purple);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  /* ── Empty state ──────────────────────────────────────────────────── */
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 80px 0;
    color: var(--fg-dim);
    text-align: center;
  }

  .empty-state h2 {
    font-size: 18px;
    font-weight: 600;
    color: var(--fg);
    margin: 0;
  }

  .empty-state p {
    font-size: 14px;
    margin: 0;
    max-width: 400px;
    line-height: 1.5;
  }

  /* ── Image grid ───────────────────────────────────────────────────── */
  .image-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 16px;
  }

  .image-card {
    display: flex;
    flex-direction: column;
    background: var(--bg-light);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    overflow: hidden;
    transition: border-color 0.15s, box-shadow 0.15s;
  }
  .image-card:hover {
    border-color: var(--purple-dim);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
  }

  .image-link {
    display: block;
    text-decoration: none;
  }

  .image-preview {
    position: relative;
    width: 100%;
    padding-top: 56.25%; /* 16:9 */
    background: var(--bg);
    overflow: hidden;
  }

  .image-preview img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    transition: transform 0.2s;
  }

  .image-card:hover .image-preview img {
    transform: scale(1.03);
  }

  .image-info {
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .image-meta {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .image-date {
    font-size: 12px;
    color: var(--fg-dim);
  }

  .image-size {
    font-size: 11px;
    color: var(--comment);
    font-family: var(--font-mono);
  }

  .image-url-row {
    overflow: hidden;
  }

  .image-url {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--cyan);
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    background: var(--bg);
    padding: 6px 8px;
    border-radius: var(--radius-sm);
  }

  .image-actions {
    display: flex;
    gap: 8px;
  }

  .action-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 6px 12px;
    font-size: 12px;
    font-family: var(--font);
    font-weight: 500;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line);
    cursor: pointer;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
  }

  .copy-btn {
    background: transparent;
    color: var(--cyan);
    border-color: rgba(139, 233, 253, 0.3);
  }
  .copy-btn:hover {
    background: rgba(139, 233, 253, 0.1);
    border-color: var(--cyan);
  }

  .delete-btn {
    background: transparent;
    color: var(--red);
    border-color: rgba(255, 85, 85, 0.3);
  }
  .delete-btn:hover {
    background: rgba(255, 85, 85, 0.1);
    border-color: var(--red);
  }

  /* ── Toast ────────────────────────────────────────────────────────── */
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
    font-family: var(--font);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
    opacity: 0;
    transition: opacity 0.2s, transform 0.25s ease-out;
    pointer-events: none;
    z-index: 1000;
  }
  .toast.visible {
    opacity: 1;
    transform: translateX(-50%) translateY(0);
  }

  /* ── Responsive ───────────────────────────────────────────────────── */
  @media (max-width: 768px) {
    .gallery-header {
      flex-direction: column;
      align-items: stretch;
    }

    .header-right {
      flex-wrap: wrap;
    }

    .search-input {
      width: 140px;
    }

    .search-input:focus {
      width: 180px;
    }

    .image-grid {
      grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
      gap: 12px;
    }

    .gallery-content {
      padding: 16px;
    }

    .auth-card {
      padding: 32px 24px;
      margin: 16px;
    }
  }
</style>
