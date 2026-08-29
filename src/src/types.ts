// Shared types mirroring the Rust engine (downpilot-core/src/models.rs).
// Keep this in sync with src-tauri/crates/downpilot-core/src/models.rs.

export type Status =
  | "queued"
  | "scheduled"
  | "downloading"
  | "paused"
  | "stopped"
  | "completed"
  | "failed"
  | "cancelled";

export type ErrorClass =
  | "none"
  | "cancelled"
  | "connection"
  | "timeout"
  | "dns"
  | "tls"
  | "5xx"
  | "permission"
  | "bad_destination"
  | "disk_full"
  | "checksum"
  | "404"
  | "403"
  | "unknown";

export interface DownloadTask {
  id: number;
  url: string;
  filename: string;
  save_dir: string;
  status: Status;
  total_size: number;
  downloaded: number;
  transferred: number;
  speed_bps: number;
  threads: number;
  format_id: string | null;
  webpage_url: string | null;
  referer: string | null;
  user_agent: string | null;
  cookies: string | null;
  category: string;
  expected_sha256: string | null;
  error: string | null;
  created_at: number;
  updated_at: number;
  // ---- IDM-class extensions ----
  queue: string;
  description: string | null;
  started_at: number | null;
  completed_at: number | null;
  last_resumed_at: number | null;
  retry_count: number;
  max_retries: number;
  retry_delay_ms: number;
  conn_timeout_ms: number;
  read_timeout_ms: number;
  peak_speed_bps: number;
  avg_speed_bps: number;
  segments: number;
  resume_support: boolean;
  http_status: number;
  content_type: string | null;
  checksum_algorithm: string;
  checksum_value: string;
  checksum_verified: boolean;
  site_id: string;
  error_code: number;
  error_category: string;
  on_complete: string;
}

export interface NewDownload {
  url: string;
  filename?: string | null;
  save_dir?: string | null;
  threads?: number | null;
  max_connections?: number | null;
  format_id?: string | null;
  webpage_url?: string | null;
  referer?: string | null;
  user_agent?: string | null;
  cookies?: string | null;
  expected_sha256?: string | null;
  category?: string | null;
  queue?: string | null;
  description?: string | null;
  proxy_url?: string | null;
  speed_limit_kbps?: number | null;
  max_retries?: number | null;
  retry_delay_ms?: number | null;
  conn_timeout_ms?: number | null;
  read_timeout_ms?: number | null;
  checksum_algorithm?: string | null;
  on_complete?: string | null;
  start_immediately?: boolean;
}

export interface DownloadQueue {
  name: string;
  label: string;
  max_simultaneous: number;
  speed_limit_kbps: number;
  enabled: boolean;
  priority: number;
  schedule_enabled: boolean;
  schedule_start: string;
  schedule_end: string;
  on_complete: string;
  on_complete_command: string;
  // Per-queue authentication overlay. Empty values mean "inherit
  // global settings"; the secret material lives in the OS credential
  // store, never in SQLite — `has_password` is just a UI flag.
  proxy_url?: string;
  cookies?: string;
  user_agent?: string;
  auth_ref?: string;
  auth_user?: string;
  has_password?: boolean;
}

export interface SiteRule {
  domain: string;
  queue: string;
  save_dir: string;
  auth_ref: string;
  proxy_url: string;
  auto_capture: boolean;
  max_connections: number;
  bandwidth_kbps: number;
  notes: string;
  /** Optional category override applied at insert time. */
  category?: string;
  /** Optional per-site User-Agent override. */
  user_agent?: string;
}

export interface GrabRules {
  include_extensions: string[];
  exclude_extensions: string[];
  include_mime: string[];
  exclude_mime: string[];
  exclude_domains: string[];
  min_size: number;
  max_size: number;
}

export type ScheduleKind = "once" | "daily" | "weekly";

export interface Schedule {
  id: number;
  name: string;
  queue: string;
  enabled: boolean;
  kind: string; // "daily" | "weekly" | "once"
  start_time: string; // "HH:MM"
  end_time: string;   // "HH:MM"
  days: number;       // bitmask 0b1111111 (Sun..Sat)
  action: string;     // "start" | "stop" | "bandwidth"
  bandwidth_kbps: number;
  on_complete: string;
  on_complete_command: string;
  /** Higher = wins on overlap. Ties break by older id. */
  priority?: number;
  /** JSON object: { "<queueName>": kbps }. When `kind === "bandwidth"`,
   *  this is the authoritative map; the legacy single-queue
   *  `bandwidth_kbps` field is used as a fallback when the queue is
   *  missing from the map. */
  overrides_json?: string;
}

export interface Category {
  name: string;
  label: string;
  extensions: string;
  save_dir: string;
  queue: string;
  color: string | null;
  builtin: boolean;
}

export interface MediaFormat {
  format_id: string;
  ext: string;
  resolution: string | null;
  fps: number | null;
  vcodec: string | null;
  acodec: string | null;
  filesize: number | null;
  note: string | null;
  url: string | null;
  protocol: string | null;
}

export interface MediaInfo {
  title: string;
  webpage_url: string;
  thumbnail: string | null;
  duration: number | null;
  uploader: string | null;
  formats: MediaFormat[];
  best_formats: MediaFormat[];
  default_filename: string;
}

export type EngineEvent =
  | { kind: "added"; task: DownloadTask }
  | { kind: "progress"; id: number; downloaded: number; total: number; speed_bps: number }
  | { kind: "status_changed"; id: number; status: Status; error: string | null }
  | { kind: "removed"; id: number }
  | { kind: "finished"; task: DownloadTask }
  | { kind: "notify"; level: string; title: string; body: string }
  | { kind: "queue_changed"; queue: DownloadQueue };

export interface AppSettings {
  download_dir: string | null;
  default_threads: number | null;
  max_threads: number | null;
  max_simultaneous: number | null;
  speed_limit_kbps: number | null;
  theme: string | null;
  accent: string | null;
  auto_intercept: boolean | null;
  use_ytdlp_for_media: boolean | null;
  clipboard_monitor: boolean | null;
  proxy_url: string | null;
  schedule_enabled: boolean | null;
  schedule_start: string | null;
  schedule_end: string | null;
  // IDM-class settings
  on_complete: string | null;
  max_retries: number | null;
  retry_delay_ms: number | null;
  conn_timeout_ms: number | null;
  read_timeout_ms: number | null;
  // Browser-capture settings (read by the Chrome extension via NMH `config`).
  exclude_extensions: string | null;
  exclude_domains: string | null;
  allowed_extensions: string | null;
  always_allow_extensions: string | null;
  capture_mode: string | null;
}

export interface GrabbedLink {
  url: string;
  filename: string;
  ext: string;
  content_type?: string | null;
  size: number;
  category?: string | null;
  /** "include:ext" | "default" | similar — what rule kept the link. */
  matched: string;
}

// Sidebar tree node selectors.
export type SidebarKey =
  | { kind: "all" }
  | { kind: "status"; value: "unfinished" | "finished" | "failed" | "paused" | "scheduled" }
  | { kind: "category"; value: string }
  | { kind: "queue"; value: string }
  | { kind: "site"; value: string }
  | { kind: "schedule"; value: number };

export type SortField =
  | "name"
  | "status"
  | "size"
  | "downloaded"
  | "progress"
  | "speed"
  | "avg_speed"
  | "time_left"
  | "connections"
  | "category"
  | "queue"
  | "added_at"
  | "completed_at"
  | "save_dir";

export type SortDir = "asc" | "desc";

export interface Sort {
  field: SortField;
  dir: SortDir;
}

export type GroupBy = "none" | "status" | "category" | "queue" | "date";

/**
 * Per-queue bandwidth override inside a bandwidth profile.
 * `queue` is the queue name; `kbps` is the configured cap (0 = unlimited).
 */
export interface BandwidthOverride {
  queue: string;
  kbps: number;
}

/**
 * Snapshot of which schedule is currently overriding any queue.
 * Returned by the engine's `active_profiles` command so the UI can show
 * "Night Profile active" without polling every queue.
 */
export interface ActiveProfile {
  schedule_id: number;
  name: string;
  priority: number;
  affected_queues: string[];
}

/** Snapshot returned by the backend telling the UI which schedules
 *  are currently active. Powers the "Night Profile active" badge
 *  in the status bar / queue panel. */
export interface ActiveProfile {
  schedule_id: number;
  name: string;
  priority: number;
  affected_queues: string[];
}
