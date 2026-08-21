use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use crate::models::{AppSettings, DownloadTask, NewDownload, Status};

pub struct Db {
    conn: Mutex<Connection>,
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64
}

impl Db {
    /// Open (or create) the SQLite database, ensuring the parent directory
    /// exists first.
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
            );",
        )?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn insert(&self, nd: &NewDownload, save_dir: &str) -> rusqlite::Result<i64> {
        let filename = nd
            .filename
            .clone()
            .unwrap_or_else(|| guess_filename(&nd.url));
        let category = guess_category(&filename);
        let t = now();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO downloads
             (url, filename, save_dir, status, threads, format_id, webpage_url, referer,
              user_agent, cookies, category, expected_sha256, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'queued', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
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
                t
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get(&self, id: i64) -> rusqlite::Result<Option<DownloadTask>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, url, filename, save_dir, status, total_size, downloaded, threads,
                             format_id, webpage_url, referer, user_agent, cookies, category,
                             expected_sha256, error, created_at, updated_at
                      FROM downloads WHERE id = ?1")?;
        let mut rows = stmt.query_map([id], row_to_task)?;
        rows.next().transpose()
    }

    pub fn list(&self) -> rusqlite::Result<Vec<DownloadTask>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, url, filename, save_dir, status, total_size, downloaded, threads,
                             format_id, webpage_url, referer, user_agent, cookies, category,
                             expected_sha256, error, created_at, updated_at
                      FROM downloads ORDER BY id DESC")?;
        let tasks = stmt.query_map([], row_to_task)?.collect::<Result<Vec<_>, _>>()?;
        Ok(tasks)
    }

    pub fn update_progress(&self, id: i64, downloaded: i64, total_size: i64) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE downloads SET downloaded = ?1, total_size = ?2, updated_at = ?3 WHERE id = ?4",
            params![downloaded, total_size, now(), id],
        )?;
        Ok(())
    }

    pub fn set_status(&self, id: i64, status: Status, error: Option<&str>) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
            "UPDATE downloads SET status = ?1, error = ?2, updated_at = ?3 WHERE id = ?4",
            params![status.as_str(), error, now(), id],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM downloads WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> rusqlite::Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query_map([key], |r| r.get::<_, String>(0))?;
        rows.next().transpose()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn.lock().unwrap().execute(
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
                "speed_limit_kbps" => s.speed_limit_kbps = v.parse().ok(),
                "theme" => s.theme = Some(v),
                "accent" => s.accent = Some(v),
                "auto_intercept" => s.auto_intercept = Some(v == "true"),
                "use_ytdlp_for_media" => s.use_ytdlp_for_media = Some(v == "true"),
                "proxy_url" => s.proxy_url = Some(v),
                "schedule_enabled" => s.schedule_enabled = Some(v == "true"),
                "schedule_start" => s.schedule_start = Some(v),
                "schedule_end" => s.schedule_end = Some(v),
                _ => {}
            }
        }
        Ok(s)
    }
}

fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<DownloadTask> {
    let status_str: String = row.get(4)?;
    let status = match status_str.as_str() {
        "downloading" => Status::Downloading,
        "paused" => Status::Paused,
        "completed" => Status::Completed,
        "failed" => Status::Failed,
        "cancelled" => Status::Cancelled,
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
        speed_bps: 0.0,
        threads: row.get(7)?,
        format_id: row.get(8)?,
        webpage_url: row.get(9)?,
        referer: row.get(10)?,
        user_agent: row.get(11)?,
        cookies: row.get(12)?,
        category: row.get(13)?,
        expected_sha256: row.get(14)?,
        error: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
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
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "md" | "odt" | "epub" => "documents".into(),
        "mp3" | "flac" | "wav" | "ogg" | "m4a" | "aac" | "opus" => "music".into(),
        "mp4" | "mkv" | "webm" | "avi" | "mov" | "wmv" | "flv" | "ts" | "m4v" => "video".into(),
        "exe" | "msi" | "deb" | "rpm" | "AppImage" | "flatpak" | "dmg" | "apk" => "programs".into(),
        _ => "other".into(),
    }
}
