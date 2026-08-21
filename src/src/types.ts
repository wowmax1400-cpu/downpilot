// Shared types mirroring the Rust engine (downpilot-core/src/models.rs)

export type Status =
  | "queued"
  | "downloading"
  | "paused"
  | "completed"
  | "failed"
  | "cancelled";

export interface DownloadTask {
  id: number;
  url: string;
  filename: string | null;
  save_dir: string;
  status: Status;
  total_size: number;
  downloaded: number;
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
}

export interface NewDownload {
  url: string;
  filename?: string | null;
  save_dir?: string | null;
  threads?: number | null;
  format_id?: string | null;
  webpage_url?: string | null;
  referer?: string | null;
  user_agent?: string | null;
  cookies?: string | null;
  expected_sha256?: string | null;
  start_immediately?: boolean | null;
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

export interface EngineEvent {
  type: "progress" | "status_changed" | "added" | "removed";
  id?: number;
  downloaded?: number;
  total?: number;
  speed_bps?: number;
  status?: Status;
  error?: string | null;
  task?: DownloadTask;
}

export interface AppSettings {
  save_dir: string;
  threads: number;
  speed_limit_kbps: number; // 0 = unlimited
  theme: string;
  accent: string;
  auto_intercept: boolean;
  proxy_url?: string | null;
  schedule_enabled?: boolean;
  schedule_start?: string | null; // "HH:MM" 24h
  schedule_end?: string | null;
}

export interface GrabbedLink {
  url: string;
  filename: string;
  ext: string;
}
