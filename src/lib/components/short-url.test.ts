/// <reference types="vitest/globals" />
/**
 * Tests for the short URL conversion and toast logic extracted from
 * Overlay.svelte's handleUpload function.
 */

// ---------------------------------------------------------------------------
// Pure logic extracted from Overlay.svelte handleUpload
// ---------------------------------------------------------------------------

/**
 * Convert a full upload URL to a short alias URL.
 * e.g. https://sinister.ovh/abc123 → https://sinister.ovh/x/abc123
 */
function toShortUrl(url: string): string {
  return url.replace(/\/([^/]+)$/, '/x/$1');
}

/**
 * Determine the toast title based on whether clipboard copy succeeded.
 */
function toastTitle(wasCopied: boolean): string {
  return wasCopied ? 'URL copied!' : 'Uploaded!';
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('toShortUrl', () => {
  it('converts a root-level key URL into /x/ alias', () => {
    expect(toShortUrl('https://sinister.ovh/abc123')).toBe('https://sinister.ovh/x/abc123');
  });

  it('handles keys with hyphens and underscores', () => {
    expect(toShortUrl('https://sinister.ovh/aBcD_1-2z')).toBe('https://sinister.ovh/x/aBcD_1-2z');
  });

  it('handles keys with mixed case characters', () => {
    expect(toShortUrl('https://sinister.ovh/a1B2c3D4e5F6g7H8')).toBe(
      'https://sinister.ovh/x/a1B2c3D4e5F6g7H8',
    );
  });

  it('replaces the last path segment (note: not idempotent on already-short URLs)', () => {
    // The regex replaces the LAST path segment. If already /x/key,
    // the last segment (key) gets replaced, producing /x/x/key.
    // This is fine in practice because the backend never returns a URL
    // that already has /x/ in it — the /x/ prefix is only added here.
    expect(toShortUrl('https://sinister.ovh/x/abc123')).toBe('https://sinister.ovh/x/x/abc123');
  });

  it('handles URLs with trailing slash (no key, degenerate)', () => {
    // Regex matches / followed by non-slash at end. With trailing slash,
    // there's no trailing non-slash segment, so no replacement occurs.
    const result = toShortUrl('https://sinister.ovh/');
    expect(result).toBe('https://sinister.ovh/');
  });

  it('handles custom domain URLs', () => {
    expect(toShortUrl('https://custom.example.com/myImage123')).toBe(
      'https://custom.example.com/x/myImage123',
    );
  });
});

describe('toastTitle', () => {
  it('shows "URL copied!" when clipboard copy succeeds', () => {
    expect(toastTitle(true)).toBe('URL copied!');
  });

  it('shows "Uploaded!" when clipboard copy fails', () => {
    expect(toastTitle(false)).toBe('Uploaded!');
  });
});
