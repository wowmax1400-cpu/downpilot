use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use crate::models::{
    AppSettings, Category, DownloadQueue, DownloadTask, NewDownload, Schedule, SiteRule, Status,
};

pub struct Db {
    conn: Mutex<Connection>,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

impl Db {
    /// Open (or create) the SQLite database, ensuring the parent directory
    /// exists first. Runs migrations so the schema can grow across releases
    /// without wiping the user's data.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS downloads (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                filename TEXT NOT NULL,
                save_dir TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'queued',
                total_size INTEGER NOT NULL DEFAULT 0,
                downloaded INTEGER NOT NULL DEFAULT 0,
                transferred INTEGER NOT NULL DEFAULT 0,
                threads INTEGER NOT NULL DEFAULT 8,
                format_id TEXT,
                webpage_url TEXT,
                referer TEXT,
                user_agent TEXT,
                cookies TEXT,
                category TEXT NOT NULL DEFAULT 'other',
                expected_sha256 TEXT,
                error TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS queues (
                name TEXT PRIMARY KEY,
                label TEXT NOT NULL,
                max_simultaneous INTEGER NOT NULL DEFAULT 3,
                speed_limit_kbps INTEGER NOT NULL DEFAULT 0,
                enabled INTEGER NOT NULL DEFAULT 1,
                priority INTEGER NOT NULL DEFAULT 0,
                schedule_enabled INTEGER NOT NULL DEFAULT 0,
                schedule_start TEXT NOT NULL DEFAULT '00:00',
                schedule_end TEXT NOT NULL DEFAULT '23:59',
                on_complete TEXT NOT NULL DEFAULT 'none',
                on_complete_command TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS sites (
                domain TEXT PRIMARY KEY,
                queue TEXT NOT NULL DEFAULT 'general',
                save_dir TEXT NOT NULL DEFAULT '',
                auth_ref TEXT NOT NULL DEFAULT '',
                proxy_url TEXT NOT NULL DEFAULT '',
                auto_capture INTEGER NOT NULL DEFAULT 1,
                max_connections INTEGER NOT NULL DEFAULT 8,
                bandwidth_kbps INTEGER NOT NULL DEFAULT 0,
                notes TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS schedules (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                queue TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                kind TEXT NOT NULL DEFAULT 'daily',
                start_time TEXT NOT NULL DEFAULT '00:00',
                end_time TEXT NOT NULL DEFAULT '23:59',
                days INTEGER NOT NULL DEFAULT 127,
                action TEXT NOT NULL DEFAULT 'start',
                bandwidth_kbps INTEGER NOT NULL DEFAULT 0,
                on_complete TEXT NOT NULL DEFAULT 'none',
                on_complete_command TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS categories (
                name TEXT PRIMARY KEY,
                label TEXT NOT NULL,
                extensions TEXT NOT NULL DEFAULT '',
                save_dir TEXT NOT NULL DEFAULT '',
                queue TEXT NOT NULL DEFAULT 'general',
                color TEXT NOT NULL DEFAULT '#64748b',
                builtin INTEGER NOT NULL DEFAULT 0
            );",
        )?;
        // Extended task columns (added post-1.0; safe to re-run).
        let migrations: [&str; 40] = [
            "ALTER TABLE downloads ADD COLUMN queue TEXT NOT NULL DEFAULT 'general'",
            "ALTER TABLE downloads ADD COLUMN description TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE downloads ADD COLUMN started_at INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE downloads ADD COLUMN completed_at INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE downloads ADD COLUMN last_resumed_at INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE downloads ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE downloads ADD COLUMN max_retries INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE downloads ADD COLUMN retry_delay_ms INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE downloads ADD COLUMN conn_timeout_ms INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE downloads ADD COLUMN read_timeout_ms INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE downloads ADD COLUMN peak_speed_bps REAL NOT NULL DEFAULT 0",
            "ALTER TABLE downloads ADD COLUMN avg_speed_bps REAL NOT NULL DEFAULT 0",
            "ALTER TABLE downloads ADD COLUMN segments INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE downloads ADD COLUMN resume_support INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE downloads ADD COLUMN http_status INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE downloads ADD COLUMN content_type TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE downloads ADD COLUMN checksum_algorithm TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE downloads ADD COLUMN checksum_value TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE downloads ADD COLUMN checksum_verified INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE downloads ADD COLUMN site_id TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE downloads ADD COLUMN error_code INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE downloads ADD COLUMN error_category TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE downloads ADD COLUMN on_complete TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE downloads ADD COLUMN transferred INTEGER NOT NULL DEFAULT 0",
            // Scheduler: per-profile priority + per-queue overrides JSON.
            "ALTER TABLE schedules ADD COLUMN priority INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE schedules ADD COLUMN overrides_json TEXT NOT NULL DEFAULT \'{}\'",
            // Per-queue authentication overlay.
            "ALTER TABLE queues ADD COLUMN proxy_url TEXT NOT NULL DEFAULT \'\'",
            "ALTER TABLE queues ADD COLUMN cookies TEXT NOT NULL DEFAULT \'\'",
            "ALTER TABLE queues ADD COLUMN user_agent TEXT NOT NULL DEFAULT \'\'",
            "ALTER TABLE queues ADD COLUMN auth_ref TEXT NOT NULL DEFAULT \'\'",
            "ALTER TABLE queues ADD COLUMN auth_user TEXT NOT NULL DEFAULT \'\'",
            "ALTER TABLE queues ADD COLUMN has_password INTEGER NOT NULL DEFAULT 0",
            // Browser dedup: stop a Chrome retry from creating a second
            // DownPilot task for the same chrome.downloads.onCreated event.
            "ALTER TABLE downloads ADD COLUMN browser_dedup_id TEXT NOT NULL DEFAULT ''",
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_browser_dedup ON downloads(browser_dedup_id) WHERE browser_dedup_id != ''",
            // Phase 3B: download-level priority (High=10, Normal=0, Low=-10)
            // Sits *inside* the queue-priority sort, so the precedence
            // chain is: queue priority → task priority → creation time.
            "ALTER TABLE downloads ADD COLUMN priority INTEGER NOT NULL DEFAULT 0",
            // Phase 3B: one-shot "Force Started" flag. Lights up the
            // UI badge without changing the task's permanent priority.
            "ALTER TABLE downloads ADD COLUMN force_started INTEGER NOT NULL DEFAULT 0",
            // Phase 3B: queue chaining — when this queue empties, start
            // `next_queue` automatically. Empty string = no chain.
            "ALTER TABLE queues ADD COLUMN next_queue TEXT NOT NULL DEFAULT ''",
            // Per-download proxy: NULL/inherit, empty string/direct, URL/custom.
            "ALTER TABLE downloads ADD COLUMN proxy_url TEXT",
            // Site Rules: category + user_agent fields. The existing
            // sites table predates these; the ALTER statements are
            // idempotent because each column has a DEFAULT.
            "ALTER TABLE sites ADD COLUMN category TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE sites ADD COLUMN user_agent TEXT NOT NULL DEFAULT ''",
        ];
        // settings migrations
        let _ = conn.execute("ALTER TABLE settings ADD COLUMN _migrated INTEGER NOT NULL DEFAULT 0", []);
        for m in migrations {
            let _ = conn.execute(m, []);
        }
        // Indexes for fast filtering/sorting.
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_dl_status ON downloads(status);
             CREATE INDEX IF NOT EXISTS idx_dl_category ON downloads(category);
             CREATE INDEX IF NOT EXISTS idx_dl_queue ON downloads(queue);
             CREATE INDEX IF NOT EXISTS idx_dl_created ON downloads(created_at);",
        )?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.seed_defaults();
        Ok(db)
    }

    /// Insert built-in categories and the general queue on first run.
    fn seed_defaults(&self) {
        let conn = self.conn.lock().unwrap();
        // General queue.
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM queues WHERE name = 'general'",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if !exists {
            let _ = conn.execute(
                "INSERT INTO queues (name, label, max_simultaneous, priority) VALUES ('general', 'General', 3, 0)",
                [],
            );
        }
        // Built-in categories.
        for (name, label, ext, color) in crate::models::BUILTIN_CATEGORIES {
            let exists: bool = conn
                .query_row(
                    "SELECT 1 FROM categories WHERE name = ?1",
                    [name],
                    |_| Ok(true),
                )
                .unwrap_or(false);
            if !exists {
                let _ = conn.execute(
                    "INSERT INTO categories (name, label, extensions, color, builtin) VALUES (?1, ?2, ?3, ?4, 1)",
                    params![name, label, ext, color],
                );
            }
        }
    }

    // -------------------------------------------------------------------
    // Downloads
    // -------------------------------------------------------------------

    pub fn insert(&self, nd: &NewDownload, save_dir: &str) -> rusqlite::Result<i64> {
        let filename = nd
            .filename
            .clone()
            .unwrap_or_else(|| guess_filename(&nd.url));
        let category = nd.category.clone().unwrap_or_else(|| guess_category(&filename));
        let queue = nd.queue.clone().unwrap_or_else(|| "general".to_string());
        let t = now();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO downloads
             (url, filename, save_dir, status, threads, format_id, webpage_url, referer,
              user_agent, cookies, category, expected_sha256, created_at, updated_at,
              queue, description, max_retries, retry_delay_ms, conn_timeout_ms, read_timeout_ms,
              checksum_algorithm, on_complete, browser_dedup_id, priority, proxy_url)
             VALUES (?1, ?2, ?3, 'queued', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12,
                     ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
            params![
                nd.url,
                filename,
                save_dir,
                nd.threads.unwrap_or(8),
                nd.format_id,
                nd.webpage_url,
                nd.referer,
                nd.user_agent,
                nd.cookies,
                category,
                nd.expected_sha256,
                t,
                queue,
                nd.description.clone().unwrap_or_default(),
                nd.max_retries.unwrap_or(0),
                nd.retry_delay_ms.unwrap_or(0),
                nd.conn_timeout_ms.unwrap_or(0),
                nd.read_timeout_ms.unwrap_or(0),
                nd.checksum_algorithm.clone().unwrap_or_default(),
                nd.on_complete.clone().unwrap_or_default(),
                nd.browser_dedup_id.clone(),
                nd.priority.unwrap_or(0),
                nd.proxy_url.clone(),
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Phase 3B: set a task's download-level priority (high/normal/low).
    /// Persisted so the precedence chain survives an app restart.
    pub fn set_priority(&self, id: i64, priority: i32) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE downloads SET priority = ?1, updated_at = ?2 WHERE id = ?3",
            params![priority, now(), id],
        )?;
        Ok(())
    }

    pub fn set_proxy_url(&self, id: i64, proxy_url: Option<&str>) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE downloads SET proxy_url = ?1, updated_at = ?2 WHERE id = ?3",
            params![proxy_url, now(), id],
        )?;
        Ok(())
    }

    /// Phase 3B: toggle the one-shot "Force Started" badge. The flag
    /// does not change the task's persistent priority — it only tells
    /// `govern_concurrency` to consider this task as a transient
    /// bypass candidate. The engine clears it whenever the task
    /// leaves Downloading.
    pub fn set_force_started(&self, id: i64, value: bool) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE downloads SET force_started = ?1, updated_at = ?2 WHERE id = ?3",
            params![value as i64, now(), id],
        )?;
        Ok(())
    }

/// Look up a task by the browser-side dedup handle. Returns `None` if the
    /// handle is empty or no task matches. Used by the engine to suppress
    /// duplicate inserts when a Chrome retry fires the same download event.
    pub fn find_by_browser_dedup(&self, dedup: &str) -> rusqlite::Result<Option<i64>> {
        if dedup.is_empty() {
            return Ok(None);
        }
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id FROM downloads WHERE browser_dedup_id = ?1 LIMIT 1")?;
        let mut rows = stmt.query_map([dedup], |r| r.get::<_, i64>(0))?;
        Ok(rows.next().transpose()?.map(|id| id))
    }

    /// Look up a task by URL. Returns the most recently inserted matching
    /// id, or `None` if no task matches. Used by the Batch dialog and the
    /// Grabber dialog to pre-warn the user about a URL they're about to
    /// add that already exists in the table.
    ///
    /// Note: this is a *helper*, not an enforcement point. The engine
    /// never refuses to add a URL just because it already exists — users
    /// legitimately re-download files (different file version, expired
    /// link, etag miss, etc.). The Batch dialog uses this to highlight
    /// duplicate rows in its preview so the user can decide.
    pub fn find_by_url(&self, url: &str) -> rusqlite::Result<Option<i64>> {
        if url.is_empty() {
            return Ok(None);
        }
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id FROM downloads WHERE url = ?1 ORDER BY id DESC LIMIT 1",
        )?;
        let mut rows = stmt.query_map([url], |r| r.get::<_, i64>(0))?;
        Ok(rows.next().transpose()?.map(|id| id))
    }

    /// Returns the count of rows whose `url` matches `url` exactly. Used
    /// by the Batch dialog to flag "this URL is already in the table N
    /// times" — even if the most recent one is in a terminal state.
    pub fn count_by_url(&self, url: &str) -> rusqlite::Result<i64> {
        if url.is_empty() {
            return Ok(0);
        }
        let conn = self.conn.lock().unwrap();
        let conn = &conn;
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM downloads WHERE url = ?1",
                [url],
                |r| r.get(0),
            )
            .unwrap_or(0);
        Ok(n)
    }

    pub fn get(&self, id: i64) -> rusqlite::Result<Option<DownloadTask>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, url, filename, save_dir, status, total_size, downloaded, transferred,
                    threads, format_id, webpage_url, referer, user_agent, cookies, category,
                    expected_sha256, error, created_at, updated_at, queue, description,
                    started_at, completed_at, last_resumed_at, retry_count, max_retries,
                    retry_delay_ms, conn_timeout_ms, read_timeout_ms, peak_speed_bps,
                    avg_speed_bps, segments, resume_support, http_status, content_type,
                    checksum_algorithm, checksum_value, checksum_verified, site_id, error_code,
                    error_category, on_complete, priority, force_started, proxy_url
             FROM downloads WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map([id], row_to_task)?;
        rows.next().transpose()
    }

    pub fn list(&self) -> rusqlite::Result<Vec<DownloadTask>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, url, filename, save_dir, status, total_size, downloaded, transferred,
                    threads, format_id, webpage_url, referer, user_agent, cookies, category,
                    expected_sha256, error, created_at, updated_at, queue, description,
                    started_at, completed_at, last_resumed_at, retry_count, max_retries,
                    retry_delay_ms, conn_timeout_ms, read_timeout_ms, peak_speed_bps,
                    avg_speed_bps, segments, resume_support, http_status, content_type,
                    checksum_algorithm, checksum_value, checksum_verified, site_id, error_code,
                    error_category, on_complete, priority, force_started, proxy_url
             FROM downloads ORDER BY id DESC",
        )?;
        let tasks = stmt.query_map([], row_to_task)?.collect::<Result<Vec<_>, _>>()?;
        Ok(tasks)
    }

    pub fn list_by_queue(&self, queue: &str) -> rusqlite::Result<Vec<DownloadTask>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, url, filename, save_dir, status, total_size, downloaded, transferred,
                    threads, format_id, webpage_url, referer, user_agent, cookies, category,
                    expected_sha256, error, created_at, updated_at, queue, description,
                    started_at, completed_at, last_resumed_at, retry_count, max_retries,
                    retry_delay_ms, conn_timeout_ms, read_timeout_ms, peak_speed_bps,
                    avg_speed_bps, segments, resume_support, http_status, content_type,
                    checksum_algorithm, checksum_value, checksum_verified, site_id, error_code,
                    error_category, on_complete, priority, force_started, proxy_url
             FROM downloads WHERE queue = ?1 ORDER BY id DESC",
        )?;
        let tasks = stmt
            .query_map([queue], row_to_task)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tasks)
    }

    pub fn update_progress(
        &self,
        id: i64,
        downloaded: i64,
        transferred: i64,
        total_size: i64,
        speed_bps: f64,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let peak = conn
            .query_row(
                "SELECT peak_speed_bps FROM downloads WHERE id = ?1",
                [id],
                |r| r.get::<_, f64>(0),
            )
            .unwrap_or(0.0);
        let new_peak = if speed_bps > peak { speed_bps } else { peak };
        conn.execute(
            "UPDATE downloads SET downloaded = ?1, transferred = ?2, total_size = ?3,
             peak_speed_bps = ?4, updated_at = ?5 WHERE id = ?6",
            params![downloaded, transferred, total_size, new_peak, now(), id],
        )?;
        Ok(())
    }

    pub fn set_status(&self, id: i64, status: Status, error: Option<&str>) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let err_cat = error.map(error_to_category).unwrap_or_default();
        conn.execute(
            "UPDATE downloads SET status = ?1, error = ?2, error_category = ?3, updated_at = ?4 WHERE id = ?5",
            params![status.as_str(), error, err_cat, now(), id],
        )?;
        if status == Status::Completed {
            // Compute average speed if we have timestamps.
            let _ = conn.execute(
                "UPDATE downloads SET completed_at = ?1 WHERE id = ?2",
                params![now(), id],
            );
        }
        Ok(())
    }

    /// Transaction-bound: persist final progress AND status in one atomic
    /// write. Avoids a torn state where `downloaded` is the canonical
    /// final value but `status` is still `Downloading` (visible to the UI
    /// between the two UPDATEs and after a crash). Returns the row's
    /// `peak_speed_bps` after the write so the caller can compute
    /// `avg_speed_bps` from timestamps.
    pub fn finish_complete(
        &self,
        id: i64,
        downloaded: i64,
        total_size: i64,
    ) -> rusqlite::Result<f64> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        // Read peak first so we can preserve it.
        let peak: f64 = tx
            .query_row(
                "SELECT peak_speed_bps FROM downloads WHERE id = ?1",
                [id],
                |r| r.get::<_, f64>(0),
            )
            .unwrap_or(0.0);
        let now_ts = now();
        tx.execute(
            "UPDATE downloads SET downloaded = ?1, transferred = ?1, total_size = ?2,
             status = 'completed', completed_at = ?3, updated_at = ?3
             WHERE id = ?4",
            params![downloaded, total_size, now_ts, id],
        )?;
        tx.commit()?;
        Ok(peak)
    }

    /// Transaction-bound: set status to a terminal state (Failed / Paused /
    /// Stopped / Cancelled) and record the error. One atomic write so the
    /// UI never sees a half-updated error/status pair.
    pub fn finish_terminal(
        &self,
        id: i64,
        status: Status,
        error: Option<&str>,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let err_cat = error.map(error_to_category).unwrap_or_default();
        conn.execute(
            "UPDATE downloads SET status = ?1, error = ?2, error_category = ?3, updated_at = ?4 WHERE id = ?5",
            params![status.as_str(), error, err_cat, now(), id],
        )?;
        Ok(())
    }

    /// Set just the content-type from the probe (called by `add()` before
    /// the downloader starts, so the UI sees the real MIME type
    /// immediately). No-op if `ct` is None.
    pub fn set_content_type(&self, id: i64, ct: Option<&str>) -> rusqlite::Result<()> {
        if let Some(ct) = ct {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "UPDATE downloads SET content_type = ?1, updated_at = ?2 WHERE id = ?3",
                params![ct, now(), id],
            )?;
        }
        Ok(())
    }

    /// Persist a rich set of metadata discovered during the download.
    pub fn set_meta(&self, id: i64, meta: &DownloadMeta) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE downloads SET total_size = ?1, threads = ?2, segments = ?3,
             resume_support = ?4, http_status = ?5, content_type = ?6,
             avg_speed_bps = ?7, started_at = ?8, last_resumed_at = ?9,
             checksum_algorithm = ?10, checksum_value = ?11, checksum_verified = ?12,
             error_code = ?13, error_category = ?14 WHERE id = ?15",
            params![
                meta.total_size,
                meta.threads,
                meta.segments,
                meta.resume_support as i64,
                meta.http_status,
                meta.content_type,
                meta.avg_speed_bps,
                meta.started_at,
                meta.last_resumed_at,
                meta.checksum_algorithm,
                meta.checksum_value,
                meta.checksum_verified as i64,
                meta.error_code,
                meta.error_category,
                id
            ],
        )?;
        Ok(())
    }

    pub fn set_category(&self, id: i64, category: &str) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE downloads SET category = ?1 WHERE id = ?2",
            params![category, id],
        )?;
        Ok(())
    }

    pub fn set_queue(&self, id: i64, queue: &str) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE downloads SET queue = ?1 WHERE id = ?2",
            params![queue, id],
        )?;
        Ok(())
    }

    pub fn set_filename(&self, id: i64, filename: &str) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE downloads SET filename = ?1 WHERE id = ?2",
            params![filename, id],
        )?;
        Ok(())
    }

    pub fn set_total_size(&self, id: i64, total_size: i64) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE downloads SET total_size = ?1, updated_at = ?2 WHERE id = ?3",
            params![total_size, now(), id],
        )?;
        Ok(())
    }

    /// Stamp `last_resumed_at` to the current epoch second. Cheap —
    /// called every time a user resumes a download.
    pub fn set_last_resumed(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE downloads SET last_resumed_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now(), id],
        )?;
        Ok(())
    }

    pub fn set_save_dir(&self, id: i64, save_dir: &str) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE downloads SET save_dir = ?1 WHERE id = ?2",
            params![save_dir, id],
        )?;
        Ok(())
    }

    pub fn set_description(&self, id: i64, description: &str) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE downloads SET description = ?1 WHERE id = ?2",
            params![description, id],
        )?;
        Ok(())
    }

    /// Set (or clear, if `value` is `None`) the user-supplied
    /// expected SHA-256 digest. Pass an empty string to clear.
    pub fn set_expected_sha256(&self, id: i64, value: Option<&str>) -> rusqlite::Result<()> {
        let normalized = value
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase());
        self.conn.lock().unwrap().execute(
            "UPDATE downloads SET expected_sha256 = ?1 WHERE id = ?2",
            params![normalized, id],
        )?;
        Ok(())
    }

    /// Update the task's User-Agent header. Used by Site Rules to apply
    /// a per-domain UA override at insert time.
    pub fn set_user_agent(&self, id: i64, user_agent: &str) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE downloads SET user_agent = ?1 WHERE id = ?2",
            params![user_agent, id],
        )?;
        Ok(())
    }

    pub fn inc_retry(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE downloads SET retry_count = retry_count + 1 WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: i64) -> rusqlite::Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM downloads WHERE id = ?1", [id])?;
        Ok(())
    }

    // -------------------------------------------------------------------
    // Queues / Sites / Schedules / Categories / Settings
    // -------------------------------------------------------------------

    pub fn list_queues(&self) -> rusqlite::Result<Vec<DownloadQueue>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT name, label, max_simultaneous, speed_limit_kbps, enabled, priority,
                    schedule_enabled, schedule_start, schedule_end, on_complete, on_complete_command,
                    next_queue, proxy_url, cookies, user_agent, auth_ref, auth_user, has_password
             FROM queues ORDER BY priority ASC, name ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(DownloadQueue {
                name: r.get(0)?,
                label: r.get(1)?,
                max_simultaneous: r.get(2)?,
                speed_limit_kbps: r.get(3)?,
                enabled: r.get::<_, i64>(4)? != 0,
                priority: r.get(5)?,
                schedule_enabled: r.get::<_, i64>(6)? != 0,
                schedule_start: r.get(7)?,
                schedule_end: r.get(8)?,
                on_complete: r.get(9)?,
                on_complete_command: r.get(10)?,
                next_queue: r.get(11)?,
                proxy_url: r.get(12)?,
                cookies: r.get(13)?,
                user_agent: r.get(14)?,
                auth_ref: r.get(15)?,
                auth_user: r.get(16)?,
                has_password: r.get::<_, i64>(17)? != 0,
            })
        })?;
        rows.collect()
    }

    pub fn get_queue(&self, name: &str) -> rusqlite::Result<Option<DownloadQueue>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT name, label, max_simultaneous, speed_limit_kbps, enabled, priority,
                    schedule_enabled, schedule_start, schedule_end, on_complete, on_complete_command,
                    next_queue, proxy_url, cookies, user_agent, auth_ref, auth_user, has_password
             FROM queues WHERE name = ?1",
        )?;
        let mut rows = stmt.query_map([name], |r| {
            Ok(DownloadQueue {
                name: r.get(0)?,
                label: r.get(1)?,
                max_simultaneous: r.get(2)?,
                speed_limit_kbps: r.get(3)?,
                enabled: r.get::<_, i64>(4)? != 0,
                priority: r.get(5)?,
                schedule_enabled: r.get::<_, i64>(6)? != 0,
                schedule_start: r.get(7)?,
                schedule_end: r.get(8)?,
                on_complete: r.get(9)?,
                on_complete_command: r.get(10)?,
                next_queue: r.get(11)?,
                proxy_url: r.get(12)?,
                cookies: r.get(13)?,
                user_agent: r.get(14)?,
                auth_ref: r.get(15)?,
                auth_user: r.get(16)?,
                has_password: r.get::<_, i64>(17)? != 0,
            })
        })?;
        rows.next().transpose()
    }

    pub fn upsert_queue(&self, q: &DownloadQueue) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO queues (name, label, max_simultaneous, speed_limit_kbps, enabled, priority,
                                schedule_enabled, schedule_start, schedule_end, on_complete, on_complete_command,
                                next_queue, proxy_url, cookies, user_agent, auth_ref, auth_user, has_password)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
             ON CONFLICT(name) DO UPDATE SET
                label = excluded.label,
                max_simultaneous = excluded.max_simultaneous,
                speed_limit_kbps = excluded.speed_limit_kbps,
                enabled = excluded.enabled,
                priority = excluded.priority,
                schedule_enabled = excluded.schedule_enabled,
                schedule_start = excluded.schedule_start,
                schedule_end = excluded.schedule_end,
                on_complete = excluded.on_complete,
                on_complete_command = excluded.on_complete_command,
                next_queue = excluded.next_queue,
                proxy_url = excluded.proxy_url,
                cookies = excluded.cookies,
                user_agent = excluded.user_agent,
                auth_ref = excluded.auth_ref,
                auth_user = excluded.auth_user,
                has_password = excluded.has_password",
            params![
                q.name,
                q.label,
                q.max_simultaneous,
                q.speed_limit_kbps,
                q.enabled as i64,
                q.priority,
                q.schedule_enabled as i64,
                q.schedule_start,
                q.schedule_end,
                q.on_complete,
                q.on_complete_command,
                q.next_queue,
                q.proxy_url,
                q.cookies,
                q.user_agent,
                q.auth_ref,
                q.auth_user,
                q.has_password as i64
            ],
        )?;
        Ok(())
    }

    pub fn delete_queue(&self, name: &str) -> rusqlite::Result<()> {
        // Reassign owned downloads back to the general queue.
        self.conn.lock().unwrap().execute(
            "UPDATE downloads SET queue = 'general' WHERE queue = ?1",
            [name],
        )?;
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM queues WHERE name = ?1", [name])?;
        Ok(())
    }

    pub fn list_sites(&self) -> rusqlite::Result<Vec<SiteRule>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT domain, queue, save_dir, auth_ref, proxy_url, auto_capture,
                    max_connections, bandwidth_kbps, category, user_agent, notes
             FROM sites ORDER BY domain ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(SiteRule {
                domain: r.get(0)?,
                queue: r.get(1)?,
                save_dir: r.get(2)?,
                auth_ref: r.get(3)?,
                proxy_url: r.get(4)?,
                auto_capture: r.get::<_, i64>(5)? != 0,
                max_connections: r.get(6)?,
                bandwidth_kbps: r.get(7)?,
                category: r.get(8)?,
                user_agent: r.get(9)?,
                notes: r.get(10)?,
            })
        })?;
        rows.collect()
    }

    pub fn upsert_site(&self, s: &SiteRule) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sites (domain, queue, save_dir, auth_ref, proxy_url, auto_capture,
                               max_connections, bandwidth_kbps, category, user_agent, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(domain) DO UPDATE SET
                queue = excluded.queue,
                save_dir = excluded.save_dir,
                auth_ref = excluded.auth_ref,
                proxy_url = excluded.proxy_url,
                auto_capture = excluded.auto_capture,
                max_connections = excluded.max_connections,
                bandwidth_kbps = excluded.bandwidth_kbps,
                category = excluded.category,
                user_agent = excluded.user_agent,
                notes = excluded.notes",
            params![
                s.domain,
                s.queue,
                s.save_dir,
                s.auth_ref,
                s.proxy_url,
                s.auto_capture as i64,
                s.max_connections,
                s.bandwidth_kbps,
                s.category,
                s.user_agent,
                s.notes,
            ],
        )?;
        Ok(())
    }

    pub fn delete_site(&self, domain: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM sites WHERE domain = ?1", params![domain])?;
        Ok(())
    }

    pub fn list_schedules(&self) -> rusqlite::Result<Vec<Schedule>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, queue, enabled, kind, start_time, end_time, days, action,
                    bandwidth_kbps, on_complete, on_complete_command, priority, overrides_json
             FROM schedules ORDER BY priority DESC, id ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Schedule {
                id: r.get(0)?,
                name: r.get(1)?,
                queue: r.get(2)?,
                enabled: r.get::<_, i64>(3)? != 0,
                kind: r.get(4)?,
                start_time: r.get(5)?,
                end_time: r.get(6)?,
                days: r.get(7)?,
                action: r.get(8)?,
                bandwidth_kbps: r.get(9)?,
                on_complete: r.get(10)?,
                on_complete_command: r.get(11)?,
                priority: r.get(12)?,
                overrides_json: r.get(13)?,
            })
        })?;
        rows.collect()
    }

    pub fn upsert_schedule(&self, s: &Schedule) -> rusqlite::Result<i64> {
        let conn = self.conn.lock().unwrap();
        if s.id > 0 {
            conn.execute(
                "UPDATE schedules SET name=?1, queue=?2, enabled=?3, kind=?4, start_time=?5,
                 end_time=?6, days=?7, action=?8, bandwidth_kbps=?9, on_complete=?10,
                 on_complete_command=?11, priority=?12, overrides_json=?13 WHERE id=?14",
                params![
                    s.name, s.queue, s.enabled as i64, s.kind, s.start_time, s.end_time, s.days,
                    s.action, s.bandwidth_kbps, s.on_complete, s.on_complete_command,
                    s.priority, s.overrides_json, s.id
                ],
            )?;
            Ok(s.id)
        } else {
            conn.execute(
                "INSERT INTO schedules (name, queue, enabled, kind, start_time, end_time, days,
                                       action, bandwidth_kbps, on_complete, on_complete_command,
                                       priority, overrides_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    s.name, s.queue, s.enabled as i64, s.kind, s.start_time, s.end_time, s.days,
                    s.action, s.bandwidth_kbps, s.on_complete, s.on_complete_command,
                    s.priority, s.overrides_json
                ],
            )?;
            Ok(conn.last_insert_rowid())
        }
    }

    pub fn delete_schedule(&self, id: i64) -> rusqlite::Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM schedules WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn list_categories(&self) -> rusqlite::Result<Vec<Category>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT name, label, extensions, save_dir, queue, color, builtin
             FROM categories ORDER BY builtin DESC, name ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Category {
                name: r.get(0)?,
                label: r.get(1)?,
                extensions: r.get(2)?,
                save_dir: r.get(3)?,
                queue: r.get(4)?,
                color: r.get(5)?,
                builtin: r.get::<_, i64>(6)? != 0,
            })
        })?;
        rows.collect()
    }

    pub fn upsert_category(&self, c: &Category) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO categories (name, label, extensions, save_dir, queue, color, builtin)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(name) DO UPDATE SET
                label = excluded.label,
                extensions = excluded.extensions,
                save_dir = excluded.save_dir,
                queue = excluded.queue,
                color = excluded.color,
                builtin = excluded.builtin",
            params![
                c.name,
                c.label,
                c.extensions,
                c.save_dir,
                c.queue,
                c.color,
                c.builtin as i64
            ],
        )?;
        Ok(())
    }

    pub fn delete_category(&self, name: &str) -> rusqlite::Result<()> {
        // Reassign owned downloads to 'other'.
        self.conn.lock().unwrap().execute(
            "UPDATE downloads SET category = 'other' WHERE category = ?1",
            [name],
        )?;
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM categories WHERE name = ?1 AND builtin = 0", [name])?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query_map([key], |r| r.get::<_, String>(0))?;
        rows.next().transpose()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
        Ok(())
    }

    pub fn all_settings(&self) -> rusqlite::Result<AppSettings> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut s = AppSettings::default();
        for row in rows {
            let (k, v) = row?;
            match k.as_str() {
                "download_dir" => s.download_dir = Some(v),
                "default_threads" => s.default_threads = v.parse().ok(),
                "max_threads" => s.max_threads = v.parse().ok(),
                "max_simultaneous" => s.max_simultaneous = v.parse().ok(),
                "speed_limit_kbps" => s.speed_limit_kbps = v.parse().ok(),
                "theme" => s.theme = Some(v),
                "accent" => s.accent = Some(v),
                "auto_intercept" => s.auto_intercept = Some(v == "true"),
                "use_ytdlp_for_media" => s.use_ytdlp_for_media = Some(v == "true"),
                "clipboard_monitor" => s.clipboard_monitor = Some(v == "true"),
                "proxy_url" => s.proxy_url = Some(v),
                "schedule_enabled" => s.schedule_enabled = Some(v == "true"),
                "schedule_start" => s.schedule_start = Some(v),
                "schedule_end" => s.schedule_end = Some(v),
                "on_complete" => s.on_complete = Some(v),
                "max_retries" => s.max_retries = v.parse().ok(),
                "retry_delay_ms" => s.retry_delay_ms = v.parse().ok(),
                "conn_timeout_ms" => s.conn_timeout_ms = v.parse().ok(),
                "read_timeout_ms" => s.read_timeout_ms = v.parse().ok(),
                "exclude_extensions" => s.exclude_extensions = Some(v),
                "exclude_domains" => s.exclude_domains = Some(v),
                "allowed_extensions" => s.allowed_extensions = Some(v),
                "always_allow_extensions" => s.always_allow_extensions = Some(v),
                "capture_mode" => s.capture_mode = Some(v),
                _ => {}
            }
        }
        Ok(s)
    }
}

/// Rich metadata stored mid-download / on completion.
#[derive(Debug, Default, Clone)]
pub struct DownloadMeta {
    pub total_size: i64,
    pub threads: u32,
    pub segments: i32,
    pub resume_support: bool,
    pub http_status: i32,
    pub content_type: String,
    pub avg_speed_bps: f64,
    pub started_at: i64,
    pub last_resumed_at: i64,
    pub checksum_algorithm: String,
    pub checksum_value: String,
    pub checksum_verified: bool,
    pub error_code: i32,
    pub error_category: String,
}

/// Snapshot a task's meta fields so callers can apply a partial delta
/// without wiping the other meta columns (e.g. `set_meta(task, DownloadMeta {
/// checksum_verified: true, ..DownloadMeta::from(task) })`).
impl From<&crate::models::DownloadTask> for DownloadMeta {
    fn from(t: &crate::models::DownloadTask) -> Self {
        Self {
            total_size: t.total_size,
            threads: t.threads,
            segments: t.segments,
            resume_support: t.resume_support,
            http_status: t.http_status,
            content_type: t.content_type.clone().unwrap_or_default(),
            avg_speed_bps: t.avg_speed_bps,
            started_at: t.started_at.unwrap_or(0),
            last_resumed_at: t.last_resumed_at.unwrap_or(0),
            checksum_algorithm: t.checksum_algorithm.clone(),
            checksum_value: t.checksum_value.clone(),
            checksum_verified: t.checksum_verified,
            error_code: t.error_code,
            error_category: t.error_category.clone(),
        }
    }
}

fn error_to_category(err: &str) -> String {
    let e = err.to_lowercase();
    if e.contains("dns") {
        "dns".into()
    } else if e.contains("tls") || e.contains("ssl") || e.contains("certificate") {
        "tls".into()
    } else if e.contains("timed out") || e.contains("timeout") {
        "timeout".into()
    } else if e.contains("404") {
        "404".into()
    } else if e.contains("403") || e.contains("401") || e.contains("407") {
        "403".into()
    } else if e.contains("429") {
        "429".into()
    } else if e.contains("5") && e.contains("0") && e.contains("server") {
        "5xx".into()
    } else if e.contains("disk") || e.contains("space") || e.contains("no space") {
        "disk_full".into()
    } else if e.contains("permission") {
        "permission".into()
    } else if e.contains("checksum") {
        "checksum".into()
    } else if e.contains("connection") || e.contains("unreachable") || e.contains("reset") {
        "connection".into()
    } else if e.contains("cancelled") {
        "cancelled".into()
    } else {
        "unknown".into()
    }
}

fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<DownloadTask> {
    let status_str: String = row.get(4)?;
    let status = match status_str.as_str() {
        "downloading" => Status::Downloading,
        "paused" => Status::Paused,
        "stopped" => Status::Stopped,
        "completed" => Status::Completed,
        "failed" => Status::Failed,
        "cancelled" => Status::Cancelled,
        "scheduled" => Status::Scheduled,
        _ => Status::Queued,
    };
    Ok(DownloadTask {
        id: row.get(0)?,
        url: row.get(1)?,
        filename: row.get(2)?,
        save_dir: row.get(3)?,
        status,
        total_size: row.get(5)?,
        downloaded: row.get(6)?,
        transferred: row.get::<_, i64>(7)?,
        speed_bps: 0.0,
        threads: row.get(8)?,
        format_id: row.get(9)?,
        webpage_url: row.get(10)?,
        referer: row.get(11)?,
        user_agent: row.get(12)?,
        cookies: row.get(13)?,
        category: row.get(14)?,
        expected_sha256: row.get(15)?,
        error: row.get(16)?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
        queue: row.get(19)?,
        description: row.get(20)?,
        started_at: row.get(21)?,
        completed_at: row.get(22)?,
        last_resumed_at: row.get(23)?,
        retry_count: row.get(24)?,
        max_retries: row.get(25)?,
        retry_delay_ms: row.get(26)?,
        conn_timeout_ms: row.get(27)?,
        read_timeout_ms: row.get(28)?,
        peak_speed_bps: row.get(29)?,
        avg_speed_bps: row.get(30)?,
        segments: row.get(31)?,
        resume_support: row.get::<_, i64>(32)? != 0,
        http_status: row.get(33)?,
        content_type: row.get(34)?,
        checksum_algorithm: row.get(35)?,
        checksum_value: row.get(36)?,
        checksum_verified: row.get::<_, i64>(37)? != 0,
        site_id: row.get(38)?,
        error_code: row.get(39)?,
        error_category: row.get(40)?,
        on_complete: row.get(41)?,
        priority: row.get(42)?,
        force_started: row.get::<_, i64>(43)? != 0,
        proxy_url: row.get(44)?,
    })
}

pub fn guess_filename(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let name = path.rsplit('/').next().unwrap_or("download");
    if name.is_empty() {
        "download".to_string()
    } else {
        name.to_string()
    }
}

pub fn guess_category(filename: &str) -> String {
    let lower = filename.to_ascii_lowercase();
    let ext = lower.rsplit('.').next().unwrap_or("");
    match ext {
        "zip" | "rar" | "7z" | "tar" | "gz" | "xz" | "bz2" | "zst" | "iso" => "compressed".into(),
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "md" | "odt" | "epub" | "csv" => "documents".into(),
        "mp3" | "flac" | "wav" | "ogg" | "m4a" | "aac" | "opus" => "music".into(),
        "mp4" | "mkv" | "webm" | "avi" | "mov" | "wmv" | "flv" | "ts" | "m4v" => "video".into(),
        "exe" | "msi" | "deb" | "rpm" | "AppImage" | "flatpak" | "dmg" | "apk" | "run" | "sh" => "programs".into(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "heic" | "ico" => "images".into(),
        _ => "other".into(),
    }
}
