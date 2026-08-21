//! DownPilot core engine: multi-threaded chunked downloads with pause/resume,
//! SQLite persistence, yt-dlp media probing/downloading, and a task manager
//! that fans events out to any UI layer (Tauri events or the native messaging
//! socket).

pub mod db;
pub mod download;
pub mod engine;
pub mod models;
pub mod ytdlp;
