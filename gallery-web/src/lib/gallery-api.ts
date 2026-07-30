/**
 * Gallery API client — talks to the Sin Shot upload server.
 *
 * Configuration:
 *   VITE_GALLERY_API_URL — base URL of the upload server (default: https://sinister.ovh)
 *   VITE_GOOGLE_CLIENT_ID — Google OAuth client ID for SSO
 */

const API_BASE = (import.meta.env.VITE_GALLERY_API_URL as string) || 'https://sinister.ovh';
const GOOGLE_CLIENT_ID = (import.meta.env.VITE_GOOGLE_CLIENT_ID as string) || '';

export interface GalleryImage {
  key: string;
  url: string;
  filename: string;
  mime_type: string;
  size_bytes: number;
  uploaded_at: string;
}

export interface GalleryResponse {
  images: GalleryImage[];
}

function getToken(): string | null {
  if (typeof localStorage === 'undefined') return null;
  return localStorage.getItem('gallery_id_token');
}

export function saveToken(token: string): void {
  if (typeof localStorage === 'undefined') return;
  localStorage.setItem('gallery_id_token', token);
}

export function clearToken(): void {
  if (typeof localStorage === 'undefined') return;
  localStorage.removeItem('gallery_id_token');
}

export function getGoogleClientId(): string {
  return GOOGLE_CLIENT_ID;
}

async function authHeaders(): Promise<Record<string, string>> {
  const token = getToken();
  if (!token) return {};
  return { Authorization: `Bearer ${token}` };
}

export async function fetchGallery(): Promise<GalleryImage[]> {
  const headers = await authHeaders();
  const resp = await fetch(`${API_BASE}/api/gallery`, { headers });
  if (!resp.ok) {
    const err = await resp.json().catch(() => ({ error: resp.statusText }));
    throw new Error(err.error || `HTTP ${resp.status}`);
  }
  const data: GalleryResponse = await resp.json();
  return data.images;
}

export async function deleteImage(key: string): Promise<void> {
  const headers = await authHeaders();
  const resp = await fetch(`${API_BASE}/api/image/${encodeURIComponent(key)}`, {
    method: 'DELETE',
    headers,
  });
  if (!resp.ok) {
    const err = await resp.json().catch(() => ({ error: resp.statusText }));
    throw new Error(err.error || `HTTP ${resp.status}`);
  }
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function formatDate(isoString: string): string {
  if (isoString === 'unknown') return 'Unknown';
  const d = new Date(isoString);
  if (isNaN(d.getTime())) return isoString;
  return d.toLocaleString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}
