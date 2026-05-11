//! Write-ahead journal — crash recovery for in-flight writes.
//!
//! Per ADR-0029 Decision 8: every write is journaled BEFORE cube mutation.
//! The journal is crash recovery only — NOT long-term persistence. After a
//! write is applied to the cube, it is ALSO appended to `.tessera/writes.jsonl`
//! (the four-source model's source #4). The client acknowledgment happens only
//! after both the journal "committed" entry AND the `.tessera/writes.jsonl`
//! append succeed.
//!
//! Journal format: `.mosaic/write-journal.jsonl`
//!
//! ```json
//! {"seq":1,"ts":"...","workspace":"./","cube":"name","coord":[...],"value":N,"status":"pending"}
//! {"seq":1,"ts":"...","workspace":"./","cube":"name","coord":[...],"value":N,"status":"committed"}
//! ```
//!
//! On crash restart: replay entries with "pending" but no "committed."
//! If two pending entries target the same cell, higher `seq` wins.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};

/// Maximum journal size before rotation (10MB per ADR-0029 Decision 8).
const ROTATION_THRESHOLD: u64 = 10 * 1024 * 1024;

/// A write-ahead journal for crash recovery.
pub struct WriteJournal {
    path: PathBuf,
    mosaic_dir: PathBuf,
    next_seq: u64,
}

/// A single journal entry (serialized as one JSONL line).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub seq: u64,
    pub ts: String,
    pub workspace: String,
    pub cube: String,
    pub coord: Vec<String>,
    pub value: f64,
    pub status: JournalStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JournalStatus {
    Pending,
    Committed,
}

impl WriteJournal {
    /// Open or create a journal at the workspace's `.mosaic/` directory.
    ///
    /// Per the Claude Desktop review: on restart, scan for highest existing
    /// seq number and continue from there.
    pub fn open(workspace_path: &Path) -> std::io::Result<Self> {
        let mosaic_dir = workspace_path.join(".mosaic");
        std::fs::create_dir_all(&mosaic_dir)?;
        let path = mosaic_dir.join("write-journal.jsonl");

        // Determine next_seq by scanning existing entries
        let next_seq = if path.exists() {
            Self::scan_max_seq(&path) + 1
        } else {
            1
        };

        Ok(Self {
            path,
            mosaic_dir,
            next_seq,
        })
    }

    /// Write a "pending" entry. Returns the assigned sequence number.
    pub fn write_pending(
        &mut self,
        workspace: &str,
        cube: &str,
        coord: &[String],
        value: f64,
    ) -> std::io::Result<u64> {
        let seq = self.next_seq;
        self.next_seq += 1;

        let entry = JournalEntry {
            seq,
            ts: iso_now(),
            workspace: workspace.to_string(),
            cube: cube.to_string(),
            coord: coord.to_vec(),
            value,
            status: JournalStatus::Pending,
        };
        self.append(&entry)?;
        Ok(seq)
    }

    /// Write a "committed" entry for the given sequence number.
    pub fn write_committed(
        &mut self,
        seq: u64,
        workspace: &str,
        cube: &str,
        coord: &[String],
        value: f64,
    ) -> std::io::Result<()> {
        let entry = JournalEntry {
            seq,
            ts: iso_now(),
            workspace: workspace.to_string(),
            cube: cube.to_string(),
            coord: coord.to_vec(),
            value,
            status: JournalStatus::Committed,
        };
        self.append(&entry)?;
        self.maybe_rotate()?;
        Ok(())
    }

    /// Replay uncommitted entries from the journal.
    ///
    /// Per ADR-0029 Decision 8: entries with "pending" but no corresponding
    /// "committed" are returned for replay. If two pending entries target
    /// the same cell, the higher sequence number wins. Truncated last line
    /// is ignored (write was never acknowledged to client).
    pub fn replay_uncommitted(&self) -> Vec<JournalEntry> {
        if !self.path.exists() {
            return Vec::new();
        }
        let content = match std::fs::read_to_string(&self.path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let mut pending: HashMap<u64, JournalEntry> = HashMap::new();
        let mut committed: std::collections::HashSet<u64> = std::collections::HashSet::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            // Per ADR-0029 Amendment 3: if the last line is truncated/malformed,
            // ignore it — that write was never acknowledged to the client.
            let entry: JournalEntry = match serde_json::from_str(trimmed) {
                Ok(e) => e,
                Err(_) => {
                    tracing::warn!("Ignoring truncated journal entry (crash during write)");
                    continue;
                }
            };
            match entry.status {
                JournalStatus::Pending => {
                    pending.insert(entry.seq, entry);
                }
                JournalStatus::Committed => {
                    committed.insert(entry.seq);
                }
            }
        }

        // Remove committed entries from pending
        for seq in &committed {
            pending.remove(seq);
        }

        // Sort by seq for deterministic replay order
        let mut uncommitted: Vec<JournalEntry> = pending.into_values().collect();
        uncommitted.sort_by_key(|e| e.seq);

        // Deduplicate: if two pending entries target the same cell, higher seq wins
        let mut deduped: HashMap<(String, String, Vec<String>), JournalEntry> = HashMap::new();
        for entry in uncommitted {
            let key = (
                entry.workspace.clone(),
                entry.cube.clone(),
                entry.coord.clone(),
            );
            let should_insert = deduped
                .get(&key)
                .map(|existing| entry.seq > existing.seq)
                .unwrap_or(true);
            if should_insert {
                deduped.insert(key, entry);
            }
        }

        let mut result: Vec<JournalEntry> = deduped.into_values().collect();
        result.sort_by_key(|e| e.seq);
        result
    }

    /// Truncate the journal (called on graceful shutdown after all writes
    /// are confirmed durable in `.tessera/writes.jsonl`).
    pub fn truncate(&self) -> std::io::Result<()> {
        if self.path.exists() {
            std::fs::write(&self.path, "")?;
        }
        Ok(())
    }

    fn append(&self, entry: &JournalEntry) -> std::io::Result<()> {
        let line = serde_json::to_string(entry)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{line}")?;
        file.sync_data()?;
        Ok(())
    }

    /// Rotate if journal exceeds 10MB threshold.
    fn maybe_rotate(&self) -> std::io::Result<()> {
        let metadata = match std::fs::metadata(&self.path) {
            Ok(m) => m,
            Err(_) => return Ok(()),
        };
        if metadata.len() > ROTATION_THRESHOLD {
            let timestamp = iso_now().replace(':', "-");
            let rotated = self
                .mosaic_dir
                .join(format!("write-journal-{timestamp}.jsonl"));
            std::fs::rename(&self.path, &rotated)?;
            tracing::info!("Journal rotated to {}", rotated.display());
        }
        Ok(())
    }

    /// Scan journal file for the highest sequence number.
    fn scan_max_seq(path: &Path) -> u64 {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        let mut max_seq = 0u64;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<JournalEntry>(trimmed) {
                if entry.seq > max_seq {
                    max_seq = entry.seq;
                }
            }
        }
        max_seq
    }
}

/// Append a write to `.tessera/writes.jsonl` — the durable four-source
/// persistence layer. This is NOT the journal; this is the file that gets
/// replayed on every cube cold-load.
///
/// Per ADR-0029 Decision 8 (durability handoff): every successful write
/// must reach this file before the client gets acknowledged.
pub fn append_tessera_write(
    model_dir: &Path,
    coord_string: &str,
    value: f64,
) -> std::io::Result<u64> {
    let tessera_dir = model_dir.join(".tessera");
    std::fs::create_dir_all(&tessera_dir)?;
    let log_path = tessera_dir.join("writes.jsonl");

    // Compute write_id by line count (1-indexed, same as mc-cli write.rs)
    let write_id = match std::fs::read_to_string(&log_path) {
        Ok(s) => s.lines().count() as u64 + 1,
        Err(_) => 1,
    };

    let timestamp = iso_now();
    let coord_json =
        serde_json::to_string(coord_string).unwrap_or_else(|_| format!("\"{coord_string}\""));
    let log_entry = format!(
        "{{\"write_id\":{write_id},\"timestamp\":\"{timestamp}\",\"coord\":{coord_json},\"value\":{value},\"source\":\"mc daemon write\"}}\n"
    );

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    file.write_all(log_entry.as_bytes())?;
    file.sync_data()?;

    Ok(write_id)
}

/// ISO 8601 timestamp without external chrono dependency.
/// Reuses the same approach as mc-cli/src/write.rs.
fn iso_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

fn days_to_ymd(days_since_epoch: u64) -> (u64, u64, u64) {
    let mut y = 1970u64;
    let mut remaining = days_since_epoch;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let months: [u64; 12] = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 1u64;
    for &days_in_month in &months {
        if remaining < days_in_month {
            break;
        }
        remaining -= days_in_month;
        m += 1;
    }
    (y, m, remaining + 1)
}

fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use std::sync::atomic::{AtomicU64, Ordering};
    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("mc-daemon-test-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_journal_write_and_commit() {
        let dir = temp_dir();
        let mut journal = WriteJournal::open(&dir).unwrap();
        let seq = journal
            .write_pending("./", "test-cube", &["A".into(), "B".into()], 42.0)
            .unwrap();
        assert_eq!(seq, 1);

        journal
            .write_committed(seq, "./", "test-cube", &["A".into(), "B".into()], 42.0)
            .unwrap();

        let uncommitted = journal.replay_uncommitted();
        assert!(uncommitted.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_journal_replay_uncommitted() {
        let dir = temp_dir();
        let mut journal = WriteJournal::open(&dir).unwrap();

        // Write pending but don't commit (simulates crash)
        journal
            .write_pending("./", "test-cube", &["X".into()], 100.0)
            .unwrap();

        let uncommitted = journal.replay_uncommitted();
        assert_eq!(uncommitted.len(), 1);
        assert_eq!(uncommitted[0].value, 100.0);
        assert_eq!(uncommitted[0].seq, 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_journal_dedup_same_cell() {
        let dir = temp_dir();
        let mut journal = WriteJournal::open(&dir).unwrap();

        // Two writes to same cell — higher seq wins
        journal
            .write_pending("./", "cube", &["C".into()], 10.0)
            .unwrap();
        journal
            .write_pending("./", "cube", &["C".into()], 20.0)
            .unwrap();

        let uncommitted = journal.replay_uncommitted();
        assert_eq!(uncommitted.len(), 1);
        assert_eq!(uncommitted[0].value, 20.0);
        assert_eq!(uncommitted[0].seq, 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_journal_seq_persists_across_reopen() {
        let dir = temp_dir();
        {
            let mut journal = WriteJournal::open(&dir).unwrap();
            journal
                .write_pending("./", "cube", &["A".into()], 1.0)
                .unwrap();
            journal
                .write_committed(1, "./", "cube", &["A".into()], 1.0)
                .unwrap();
        }
        // Reopen — seq should continue from 2
        let mut journal = WriteJournal::open(&dir).unwrap();
        let seq = journal
            .write_pending("./", "cube", &["B".into()], 2.0)
            .unwrap();
        assert_eq!(seq, 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_journal_truncated_line_ignored() {
        let dir = temp_dir();
        let mosaic_dir = dir.join(".mosaic");
        fs::create_dir_all(&mosaic_dir).unwrap();
        let path = mosaic_dir.join("write-journal.jsonl");

        // Write a valid pending entry followed by a truncated line
        let valid = r#"{"seq":1,"ts":"2026-05-10T00:00:00Z","workspace":"./","cube":"c","coord":["X"],"value":5.0,"status":"pending"}"#;
        fs::write(path, format!("{valid}\n{{\"seq\":2,\"ts\":\"trun")).unwrap();

        let journal = WriteJournal::open(&dir).unwrap();
        let uncommitted = journal.replay_uncommitted();
        assert_eq!(uncommitted.len(), 1);
        assert_eq!(uncommitted[0].seq, 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_append_tessera_write() {
        let dir = temp_dir();
        let write_id = append_tessera_write(&dir, "Measure=Spend,Time=Q1", 500.0).unwrap();
        assert_eq!(write_id, 1);

        let write_id2 = append_tessera_write(&dir, "Measure=Spend,Time=Q2", 600.0).unwrap();
        assert_eq!(write_id2, 2);

        let content = fs::read_to_string(dir.join(".tessera").join("writes.jsonl")).unwrap();
        assert_eq!(content.lines().count(), 2);
        let _ = fs::remove_dir_all(&dir);
    }
}
