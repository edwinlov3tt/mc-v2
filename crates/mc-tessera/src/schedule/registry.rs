//! Schedule registry — persisted to `.tessera/schedules.json`.
//!
//! The registry holds all scheduled recipe executions for a model directory.
//! It is read/written by the daemon and the CLI command handlers.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::TesseraError;

/// A single scheduled recipe execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    /// Unique schedule identifier (format: `sched_<recipe>_<unix>_<nanos>`).
    pub id: String,
    /// Path to the recipe YAML file (relative to model dir or absolute).
    pub recipe_path: String,
    /// Cron expression string.
    pub cron: String,
    /// ISO-8601 timestamp when the schedule was created.
    pub created_at: String,
    /// Status: `"active"` or `"failed"`.
    pub status: String,
    /// ISO-8601 timestamp of the last run, if any.
    pub last_run: Option<String>,
    /// Result of the last run (e.g., "ok" or error message).
    pub last_result: Option<String>,
    /// Number of consecutive failures.
    pub failure_count: u32,
    /// Unix timestamp (seconds) for next retry after failure, if applicable.
    pub next_retry: Option<u64>,
}

/// The full schedule registry file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleRegistry {
    /// Schema version (currently 1).
    pub version: u32,
    /// All registered schedules.
    pub schedules: Vec<Schedule>,
}

impl ScheduleRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            version: 1,
            schedules: Vec::new(),
        }
    }

    /// Load registry from `<model_dir>/.tessera/schedules.json`.
    /// Returns a new empty registry if the file doesn't exist.
    pub fn load(model_dir: &Path) -> Result<Self, TesseraError> {
        let path = model_dir.join(".tessera").join("schedules.json");
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(&path).map_err(|e| TesseraError::io(&path, e))?;
        let registry: ScheduleRegistry =
            serde_json::from_str(&content).map_err(|e| TesseraError::SidecarDeserialize {
                path: path.clone(),
                message: e.to_string(),
            })?;
        Ok(registry)
    }

    /// Save registry to `<model_dir>/.tessera/schedules.json`.
    /// Creates the `.tessera/` directory if it doesn't exist.
    ///
    /// Phase 6A.1 MAJ-2: write atomically via tmp+rename so a daemon
    /// crash mid-write can never leave the registry truncated. Mirrors
    /// the watermark pattern in [`crate::incremental::save_state`].
    pub fn save(&self, model_dir: &Path) -> Result<(), TesseraError> {
        let tessera_dir = model_dir.join(".tessera");
        if !tessera_dir.exists() {
            std::fs::create_dir_all(&tessera_dir).map_err(|e| TesseraError::io(&tessera_dir, e))?;
        }
        let path = tessera_dir.join("schedules.json");
        let tmp_path = tessera_dir.join("schedules.json.tmp");
        let content =
            serde_json::to_string_pretty(self).map_err(|e| TesseraError::SidecarSerialize {
                message: e.to_string(),
            })?;
        std::fs::write(&tmp_path, content).map_err(|e| TesseraError::io(&tmp_path, e))?;
        std::fs::rename(&tmp_path, &path).map_err(|e| TesseraError::io(&path, e))?;
        Ok(())
    }
}

impl Default for ScheduleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn round_trip_registry() {
        let dir = std::env::temp_dir().join("mc_tessera_schedule_test_rt");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".tessera")).unwrap();

        let mut reg = ScheduleRegistry::new();
        reg.schedules.push(Schedule {
            id: "sched_test_123_456".to_string(),
            recipe_path: "import.recipe.yaml".to_string(),
            cron: "@hourly".to_string(),
            created_at: "2026-05-05T00:00:00Z".to_string(),
            status: "active".to_string(),
            last_run: None,
            last_result: None,
            failure_count: 0,
            next_retry: None,
        });

        reg.save(&dir).unwrap();
        let loaded = ScheduleRegistry::load(&dir).unwrap();
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.schedules.len(), 1);
        assert_eq!(loaded.schedules[0].id, "sched_test_123_456");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_uses_tmp_rename_atomically() {
        // Phase 6A.1 MAJ-2 regression: confirm save() uses tmp+rename
        // so the final schedules.json is only ever the complete payload,
        // and no stale schedules.json.tmp is left around even when a
        // dangling tmp from a prior crashed save was already on disk.
        let dir = std::env::temp_dir().join("mc_tessera_schedule_test_atomic");
        let _ = fs::remove_dir_all(&dir);
        let tessera_dir = dir.join(".tessera");
        fs::create_dir_all(&tessera_dir).unwrap();
        // Plant a stale tmp file simulating a prior crashed save.
        let stale_tmp = tessera_dir.join("schedules.json.tmp");
        fs::write(&stale_tmp, b"GARBAGE FROM PRIOR CRASH").unwrap();
        assert!(stale_tmp.exists());

        let reg = ScheduleRegistry::new();
        reg.save(&dir).unwrap();

        let final_path = tessera_dir.join("schedules.json");
        assert!(final_path.exists(), "final schedules.json should exist");
        assert!(
            !stale_tmp.exists(),
            "tmp file should be renamed away, leaving no .tmp behind"
        );

        // Sanity: the final file is a parseable registry (not garbage).
        let _ = ScheduleRegistry::load(&dir).expect("load after atomic save");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_returns_empty() {
        let dir = std::env::temp_dir().join("mc_tessera_schedule_test_missing");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let reg = ScheduleRegistry::load(&dir).unwrap();
        assert_eq!(reg.schedules.len(), 0);

        let _ = fs::remove_dir_all(&dir);
    }
}
