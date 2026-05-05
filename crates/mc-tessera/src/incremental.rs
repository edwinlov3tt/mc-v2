//! Incremental load state management for Tessera.
//!
//! Tracks watermarks/cursors between runs so subsequent imports can fetch
//! only new/changed rows from the source. State is persisted as JSON under
//! `<model_dir>/.tessera/incremental/<recipe_name>.state.json`.
//!
//! Key invariant: state is written AFTER successful commit only. A failed
//! import never advances the watermark.

use std::fs;
use std::path::{Path, PathBuf};

use mc_core::{CellCoordinate, ScalarValue};
use mc_drivers::{ColumnData, RowBatch};
use mc_recipe::IncrementalConfig;
use serde::{Deserialize, Serialize};

use crate::TesseraError;

// ---------------------------------------------------------------------------
// State shape
// ---------------------------------------------------------------------------

/// Persisted incremental-load state for a single recipe.
///
/// Written to `<model_dir>/.tessera/incremental/<recipe_name>.state.json`
/// after a successful import commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncrementalState {
    /// Name of the recipe this state belongs to.
    pub recipe_name: String,
    /// Strategy identifier: `"watermark"` or `"cursor"`.
    pub strategy: String,
    /// The source column being tracked.
    pub column: String,
    /// The last-seen maximum value of the tracked column (stringified).
    pub last_value: Option<String>,
    /// ISO 8601 timestamp of the last successful run.
    pub last_run: Option<String>,
    /// Cumulative rows imported since the last full load.
    pub rows_since_full_load: u64,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build the path to the state file for a given recipe.
fn state_path(model_dir: &Path, recipe_name: &str) -> PathBuf {
    model_dir
        .join(".tessera")
        .join("incremental")
        .join(format!("{recipe_name}.state.json"))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Load incremental state for `recipe_name` from disk.
///
/// Returns `Ok(None)` if no state file exists (first run = full load) or if
/// the file contains invalid JSON (treat as no state, per handoff decision).
pub fn load_state(
    model_dir: &Path,
    recipe_name: &str,
) -> Result<Option<IncrementalState>, TesseraError> {
    let path = state_path(model_dir, recipe_name);
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path).map_err(|e| TesseraError::io(&path, e))?;
    match serde_json::from_str::<IncrementalState>(&contents) {
        Ok(state) => Ok(Some(state)),
        Err(_) => {
            // Invalid JSON — treat as no state (full load). Per handoff
            // SPEC QUESTION #4 resolution: log a warning, return None.
            #[cfg(feature = "tracing")]
            tracing::warn!(
                "invalid incremental state at {}: treating as first run",
                path.display()
            );
            Ok(None)
        }
    }
}

/// Persist incremental state to disk atomically (write to `.tmp`, then
/// rename).
///
/// Creates the `.tessera/incremental/` directory if it does not exist.
pub fn save_state(model_dir: &Path, state: &IncrementalState) -> Result<(), TesseraError> {
    let path = state_path(model_dir, &state.recipe_name);
    let dir = path.parent().ok_or_else(|| TesseraError::Io {
        path: path.clone(),
        message: "could not determine parent directory".to_string(),
    })?;
    fs::create_dir_all(dir).map_err(|e| TesseraError::io(dir, e))?;

    let tmp_path = path.with_extension("state.json.tmp");
    let json = serde_json::to_string_pretty(state).map_err(|e| TesseraError::SidecarSerialize {
        message: e.to_string(),
    })?;
    fs::write(&tmp_path, json.as_bytes()).map_err(|e| TesseraError::io(&tmp_path, e))?;
    fs::rename(&tmp_path, &path).map_err(|e| TesseraError::io(&path, e))?;
    Ok(())
}

/// Delete the state file for `recipe_name`. The next run will perform a
/// full load.
pub fn reset_state(model_dir: &Path, recipe_name: &str) -> Result<(), TesseraError> {
    let path = state_path(model_dir, recipe_name);
    if path.exists() {
        fs::remove_file(&path).map_err(|e| TesseraError::io(&path, e))?;
    }
    Ok(())
}

/// Inject a watermark filter into a SQL query based on incremental state.
///
/// - If the query contains a `{{watermark}}` placeholder, it is replaced
///   with the `last_value`.
/// - Otherwise, ` WHERE {column} > '{last_value}'` is appended.
/// - If there is no prior state (first run), the query is returned unchanged.
/// - For HTTP sources using `param_name`, injection is handled at the
///   orchestrator level, not here.
pub fn inject_watermark(
    query: &str,
    config: &IncrementalConfig,
    state: &Option<IncrementalState>,
) -> String {
    let last_value = match state {
        Some(s) => match &s.last_value {
            Some(v) => v,
            None => return query.to_string(),
        },
        None => return query.to_string(),
    };

    // If the query has a {{watermark}} placeholder, substitute it.
    if query.contains("{{watermark}}") {
        return query.replace("{{watermark}}", last_value);
    }

    // Otherwise, append a WHERE clause.
    format!("{query} WHERE {} > '{last_value}'", config.column)
}

/// Scan a [`RowBatch`] and compute the new high-water mark by finding the
/// MAX value of the tracked column (as a string).
///
/// Returns `None` if the batch has no rows or the column is not found.
#[allow(unused_variables)]
pub fn compute_new_watermark(
    cells: &[(CellCoordinate, ScalarValue)],
    column_name: &str,
    batch: &RowBatch,
    config: &IncrementalConfig,
) -> Option<String> {
    if batch.row_count == 0 {
        return None;
    }

    // Find the column in the batch that matches the config's tracked column.
    let col = batch.columns.iter().find(|c| c.name == config.column)?;

    // Extract the max value from the column data.
    match &col.data {
        ColumnData::Str(values) => values.iter().filter_map(|v| v.as_ref()).max().cloned(),
        ColumnData::I64(values) => values
            .iter()
            .filter_map(|v| v.as_ref())
            .max()
            .map(|v| v.to_string()),
        ColumnData::F64(values) => values
            .iter()
            .filter_map(|v| v.as_ref())
            .copied()
            .filter(|v| !v.is_nan())
            .fold(None, |acc: Option<f64>, v| {
                Some(match acc {
                    Some(a) if a >= v => a,
                    _ => v,
                })
            })
            .map(|v| v.to_string()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use mc_recipe::IncrementalStrategy;
    use std::fs;

    fn make_config() -> IncrementalConfig {
        IncrementalConfig {
            strategy: IncrementalStrategy::Watermark,
            column: "updated_at".to_string(),
            format: None,
            initial_value: None,
            param_name: None,
        }
    }

    fn make_state(last_value: Option<&str>) -> IncrementalState {
        IncrementalState {
            recipe_name: "test_recipe".to_string(),
            strategy: "watermark".to_string(),
            column: "updated_at".to_string(),
            last_value: last_value.map(|s| s.to_string()),
            last_run: Some("2026-05-05T00:00:00Z".to_string()),
            rows_since_full_load: 100,
        }
    }

    #[test]
    fn t_load_state_returns_none_when_no_file() {
        let tmp = tempfile::tempdir().unwrap();
        let result = load_state(tmp.path(), "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn t_save_and_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let state = make_state(Some("2026-05-04T12:00:00Z"));
        save_state(tmp.path(), &state).unwrap();
        let loaded = load_state(tmp.path(), "test_recipe").unwrap().unwrap();
        assert_eq!(loaded.recipe_name, "test_recipe");
        assert_eq!(loaded.last_value.as_deref(), Some("2026-05-04T12:00:00Z"));
        assert_eq!(loaded.rows_since_full_load, 100);
    }

    #[test]
    fn t_load_state_returns_none_for_invalid_json() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(".tessera").join("incremental");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("bad.state.json"), "not valid json {{{").unwrap();
        let result = load_state(tmp.path(), "bad").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn t_reset_state_deletes_file() {
        let tmp = tempfile::tempdir().unwrap();
        let state = make_state(Some("v1"));
        save_state(tmp.path(), &state).unwrap();
        assert!(state_path(tmp.path(), "test_recipe").exists());
        reset_state(tmp.path(), "test_recipe").unwrap();
        assert!(!state_path(tmp.path(), "test_recipe").exists());
    }

    #[test]
    fn t_reset_state_noop_when_no_file() {
        let tmp = tempfile::tempdir().unwrap();
        // Should not error.
        reset_state(tmp.path(), "nonexistent").unwrap();
    }

    #[test]
    fn t_inject_watermark_no_state() {
        let config = make_config();
        let query = "SELECT * FROM events";
        let result = inject_watermark(query, &config, &None);
        assert_eq!(result, query);
    }

    #[test]
    fn t_inject_watermark_appends_where() {
        let config = make_config();
        let state = Some(make_state(Some("2026-05-01")));
        let result = inject_watermark("SELECT * FROM events", &config, &state);
        assert_eq!(
            result,
            "SELECT * FROM events WHERE updated_at > '2026-05-01'"
        );
    }

    #[test]
    fn t_inject_watermark_placeholder() {
        let config = make_config();
        let state = Some(make_state(Some("2026-05-01")));
        let query = "SELECT * FROM events WHERE updated_at > '{{watermark}}'";
        let result = inject_watermark(query, &config, &state);
        assert_eq!(
            result,
            "SELECT * FROM events WHERE updated_at > '2026-05-01'"
        );
    }

    #[test]
    fn t_inject_watermark_none_last_value() {
        let config = make_config();
        let state = Some(make_state(None));
        let result = inject_watermark("SELECT * FROM events", &config, &state);
        assert_eq!(result, "SELECT * FROM events");
    }

    #[test]
    fn t_compute_new_watermark_str_column() {
        let config = make_config();
        let batch = RowBatch {
            columns: vec![mc_drivers::Column {
                name: "updated_at".to_string(),
                data: ColumnData::Str(vec![
                    Some("2026-05-01".to_string()),
                    Some("2026-05-03".to_string()),
                    Some("2026-05-02".to_string()),
                    None,
                ]),
            }],
            row_count: 4,
        };
        let result = compute_new_watermark(&[], "updated_at", &batch, &config);
        assert_eq!(result.as_deref(), Some("2026-05-03"));
    }

    #[test]
    fn t_compute_new_watermark_i64_column() {
        let mut config = make_config();
        config.column = "id".to_string();
        let batch = RowBatch {
            columns: vec![mc_drivers::Column {
                name: "id".to_string(),
                data: ColumnData::I64(vec![Some(10), Some(50), Some(30), None]),
            }],
            row_count: 4,
        };
        let result = compute_new_watermark(&[], "id", &batch, &config);
        assert_eq!(result.as_deref(), Some("50"));
    }

    #[test]
    fn t_compute_new_watermark_empty_batch() {
        let config = make_config();
        let batch = RowBatch {
            columns: vec![mc_drivers::Column {
                name: "updated_at".to_string(),
                data: ColumnData::Str(vec![]),
            }],
            row_count: 0,
        };
        let result = compute_new_watermark(&[], "updated_at", &batch, &config);
        assert!(result.is_none());
    }

    #[test]
    fn t_compute_new_watermark_missing_column() {
        let config = make_config();
        let batch = RowBatch {
            columns: vec![mc_drivers::Column {
                name: "other_col".to_string(),
                data: ColumnData::Str(vec![Some("x".to_string())]),
            }],
            row_count: 1,
        };
        let result = compute_new_watermark(&[], "updated_at", &batch, &config);
        assert!(result.is_none());
    }
}
