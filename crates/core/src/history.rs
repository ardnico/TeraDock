use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: DateTime<Utc>,
    pub profile_id: String,
    pub profile_name: String,
    pub success: bool,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub forced: bool,
}

impl HistoryEntry {
    pub fn new(
        profile_id: String,
        profile_name: String,
        success: bool,
        message: Option<String>,
        forced: bool,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            profile_id,
            profile_name,
            success,
            message,
            forced,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HistoryStore {
    path: std::path::PathBuf,
}

impl HistoryStore {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn append(&self, entry: &HistoryEntry) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let line = serde_json::to_string(entry)?;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    pub fn load(&self, limit: Option<usize>) -> Result<Vec<HistoryEntry>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = File::open(&self.path)?;
        let reader = BufReader::new(file);
        let mut entries: Vec<HistoryEntry> = reader
            .lines()
            .filter_map(|line| line.ok())
            .filter_map(|line| serde_json::from_str(&line).ok())
            .collect();
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        if let Some(limit) = limit {
            entries.truncate(limit);
        }
        Ok(entries)
    }
}

pub fn format_history_entry(entry: &HistoryEntry) -> String {
    format!(
        "{} | {} | {} | {}{}",
        entry.timestamp.to_rfc3339(),
        if entry.success { "SUCCESS" } else { "FAIL" },
        entry.profile_name,
        entry.profile_id,
        entry
            .message
            .as_ref()
            .map(|m| format!(" | {}", m))
            .unwrap_or_default()
    )
}
