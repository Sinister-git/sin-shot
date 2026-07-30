import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fetchGallery, deleteImage, formatBytes, formatDate } from './gallery-api';

// Mock fetch globally
const mockFetch = vi.fn();
global.fetch = mockFetch;

beforeEach(() => {
  vi.resetAllMocks();
  localStorage.clear();
});

describe('formatBytes', () => {
  it('formats bytes', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(500)).toBe('500 B');
  });

  it('formats kilobytes', () => {
    expect(formatBytes(1024)).toBe('1.0 KB');
    expect(formatBytes(1536)).toBe('1.5 KB');
  });

  it('formats megabytes', () => {
    expect(formatBytes(1048576)).toBe('1.0 MB');
    expect(formatBytes(5242880)).toBe('5.0 MB');
  });
});

describe('formatDate', () => {
  it('returns "Unknown" for "unknown" input', () => {
    expect(formatDate('unknown')).toBe('Unknown');
  });

  it('formats valid ISO date', () => {
    const result = formatDate('2024-01-15T10:30:00Z');
    // Should contain year and month
    expect(result).toContain('2024');
    expect(result).toContain('Jan');
  });

  it('returns original string for invalid date', () => {
    expect(formatDate('not-a-date')).toBe('not-a-date');
  });
});

describe('fetchGallery', () => {
  it('throws on non-ok response', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: false,
      status: 401,
      json: async () => ({ error: 'unauthorized' }),
    });

    await expect(fetchGallery()).rejects.toThrow('unauthorized');
  });

  it('returns images on success', async () => {
    const images = [
      {
        key: 'abc123',
        url: 'https://sinister.ovh/abc123',
        filename: 'abc123.png',
        mime_type: 'image/png',
        size_bytes: 100,
        uploaded_at: '2024-01-01T00:00:00Z',
      },
    ];

    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => ({ images }),
    });

    const result = await fetchGallery();
    expect(result).toEqual(images);
  });

  it('sends Authorization header when token exists', async () => {
    localStorage.setItem('gallery_id_token', 'test-token');

    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => ({ images: [] }),
    });

    await fetchGallery();

    expect(mockFetch).toHaveBeenCalledWith(
      'https://sinister.ovh/api/gallery',
      expect.objectContaining({
        headers: { Authorization: 'Bearer test-token' },
      }),
    );
  });
});

describe('deleteImage', () => {
  it('throws on non-ok response', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: false,
      status: 404,
      json: async () => ({ error: 'not found' }),
    });

    await expect(deleteImage('nonexistent')).rejects.toThrow('not found');
  });

  it('succeeds on ok response', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => ({ deleted: true }),
    });

    await expect(deleteImage('abc123')).resolves.toBeUndefined();
  });
});
