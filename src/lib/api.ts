import type { Annotation, Book, CatalogBook, Collection, Connectivity, DatabaseBackup, Diagnostics, DownloadJob, Library, PairedDevice, ReadingState, Series } from '../types';
import { isTauri } from '@tauri-apps/api/core';

export const API_BASE = isTauri() ? 'http://127.0.0.1:1822' : '';

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
  }
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const token = localStorage.getItem('stori_reader_token');
  const headers = new Headers(init.headers);
  if (token) headers.set('Authorization', `Bearer ${token}`);
  if (init.body && !(init.body instanceof FormData)) headers.set('Content-Type', 'application/json');
  const response = await fetch(`${API_BASE}${path}`, { ...init, headers });
  if (!response.ok) {
    const payload = await response.json().catch(() => ({ error: response.statusText }));
    throw new ApiError(response.status, payload.error || response.statusText);
  }
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}

export const api = {
  health: () => request<{ status: string; version: string }>('/api/health'),
  session: () => request<{ ok: boolean }>('/api/auth/session'),
  libraries: () => request<Library[]>('/api/admin/libraries'),
  addLibrary: (name: string, path: string) => request<Library>('/api/admin/libraries', { method: 'POST', body: JSON.stringify({ name, path }) }),
  scanLibrary: (id: number) => request<{ indexed: number; warnings: string[] }>(`/api/admin/libraries/${id}/scan`, { method: 'POST' }),
  books: (query = '', filters: Record<string, string> = {}) => {
    const params = new URLSearchParams({ q: query, ...filters });
    return request<Book[]>(`/api/books?${params}`);
  },
  book: (id: number) => request<Book>(`/api/books/${id}`),
  bookFileUrl: (id: number) => {
    const token = localStorage.getItem('stori_reader_token');
    const params = token ? `?token=${encodeURIComponent(token)}` : '';
    return `${API_BASE}/api/books/${id}/file${params}`;
  },
  coverUrl: (id: number) => {
    const token = localStorage.getItem('stori_reader_token');
    const params = token ? `?token=${encodeURIComponent(token)}` : '';
    return `${API_BASE}/api/books/${id}/cover${params}`;
  },
  series: () => request<Series[]>('/api/series'),
  seriesById: (id: number) => request<Series>(`/api/series/${id}`),
  collections: () => request<Collection[]>('/api/collections'),
  collection: (id: number) => request<Collection>(`/api/collections/${id}`),
  createCollection: (payload: { name: string; description?: string; book_ids: number[] }) => request<Collection>('/api/collections', { method: 'POST', body: JSON.stringify(payload) }),
  updateCollection: (id: number, payload: { name: string; description?: string; book_ids: number[] }) => request<Collection>(`/api/collections/${id}`, { method: 'PUT', body: JSON.stringify(payload) }),
  readingState: (bookId: number) => request<ReadingState | null>(`/api/books/${bookId}/progress`),
  saveProgress: (bookId: number, locator: string, progress: number) => request<ReadingState>(`/api/books/${bookId}/progress`, { method: 'PUT', body: JSON.stringify({ locator, progress }) }),
  annotations: (bookId: number) => request<Annotation[]>(`/api/books/${bookId}/annotations`),
  addAnnotation: (bookId: number, payload: Omit<Annotation, 'id' | 'book_id' | 'created_at'>) => request<Annotation>(`/api/books/${bookId}/annotations`, { method: 'POST', body: JSON.stringify(payload) }),
  connectivity: () => request<Connectivity>('/api/admin/connectivity'),
  createPairing: () => request<Connectivity>('/api/admin/pairing', { method: 'POST' }),
  pairedDevices: () => request<PairedDevice[]>('/api/admin/devices'),
  revokeDevice: (id: string) => request<{ ok: boolean }>(`/api/admin/devices/${id}/revoke`, { method: 'POST' }),
  revokeAllDevices: () => request<{ revoked: number }>('/api/admin/devices/revoke-all', { method: 'POST' }),
  diagnostics: () => request<Diagnostics>('/api/admin/diagnostics'),
  backups: () => request<DatabaseBackup[]>('/api/admin/backups'),
  createBackup: () => request<DatabaseBackup>('/api/admin/backups', { method: 'POST' }),
  rescanLibraries: () => request<{ indexed: number; warnings: string[]; libraries: number }>('/api/library/rescan', { method: 'POST' }),
  pair: (code: string) => request<{ token: string }>('/api/auth/pair', { method: 'POST', body: JSON.stringify({ code }) })
  ,catalogSearch: (query: string) => request<{ results: CatalogBook[]; warnings: string[] }>(`/api/admin/catalog/search?q=${encodeURIComponent(query)}`)
  ,downloadJobs: () => request<DownloadJob[]>('/api/admin/downloads')
  ,queueDownload: (libraryId: number, book: CatalogBook) => request<DownloadJob>('/api/admin/downloads', { method: 'POST', body: JSON.stringify({ library_id: libraryId, book }) })
  ,pauseDownload: (id: string) => request<{ ok: boolean }>(`/api/admin/downloads/${id}/pause`, { method: 'POST' })
  ,resumeDownload: (id: string) => request<{ ok: boolean }>(`/api/admin/downloads/${id}/resume`, { method: 'POST' })
  ,cancelDownload: (id: string) => request<{ ok: boolean }>(`/api/admin/downloads/${id}/cancel`, { method: 'POST' })
  ,deleteDownload: (id: string) => request<{ ok: boolean }>(`/api/admin/downloads/${id}/delete`, { method: 'POST' })
  ,clearFinishedDownloads: () => request<{ removed: number }>('/api/admin/downloads/clear-finished', { method: 'POST' })
};
