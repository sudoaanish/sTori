export type BookFormat = 'epub' | 'pdf' | 'mobi';

export interface Library {
  id: number;
  name: string;
  path: string;
  book_count: number;
  last_scanned_at?: string;
  managed: boolean;
}

export interface Book {
  id: number;
  library_id: number;
  title: string;
  subtitle?: string;
  authors: string[];
  description?: string;
  published?: string;
  tags: string[];
  identifier?: string;
  format: BookFormat;
  file_name: string;
  series_id?: number;
  series_name?: string;
  series_index?: number;
  progress: number;
  updated_at: string;
  has_cover: boolean;
}

export interface Series {
  id: number;
  name: string;
  description?: string;
  books: Book[];
}

export interface Collection {
  id: number;
  name: string;
  description?: string;
  books: Book[];
  updated_at: string;
}

export interface Connectivity {
  server_name: string;
  version: string;
  port: number;
  urls: Array<{ url: string; label: string; recommended: boolean }>;
  pairing_code?: string;
}

export interface PairedDevice {
  id: string;
  name: string;
  created_at: string;
  last_seen_at: string;
  last_ip?: string;
  user_agent?: string;
}

export interface DatabaseBackup {
  file_name: string;
  path: string;
  size_bytes: number;
  created_at: string;
}

export interface Diagnostics {
  database_status: string;
  schema_version: number;
  database_path: string;
  backup_directory: string;
  managed_library_free_bytes?: number;
  firewall_rule_detected?: boolean;
  firewall_guidance: string;
  libraries: Array<{ id: number; name: string; path: string; available: boolean; book_count: number }>;
}

export interface ReadingState {
  book_id: number;
  locator: string;
  progress: number;
  updated_at: string;
}

export interface Annotation {
  id: number;
  book_id: number;
  kind: 'bookmark' | 'highlight' | 'note';
  locator: string;
  text?: string;
  note?: string;
  created_at: string;
}

export interface CatalogBook {
  provider: string;
  provider_id: string;
  title: string;
  authors: string[];
  description?: string;
  language?: string;
  published?: string;
  subjects: string[];
  cover_url?: string;
  download_url: string;
  source_url?: string;
  license_label: string;
}

export interface DownloadJob {
  id: string;
  library_id: number;
  provider: string;
  provider_id: string;
  title: string;
  authors: string[];
  status: 'queued' | 'downloading' | 'paused' | 'verifying' | 'importing' | 'indexing' | 'completed' | 'failed' | 'cancelled';
  message: string;
  progress: number;
  bytes_downloaded: number;
  total_bytes?: number;
  local_path?: string;
  error?: string;
  content_sha256?: string;
  created_at: string;
  updated_at: string;
}
