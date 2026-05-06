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
/// Behavior:
/// - **Placeholder path** (preferred for non-trivial queries): if the
///   query contains a `{{watermark}}` placeholder, it is replaced
///   with the escaped `last_value`. The query author keeps full
///   control over clause ordering and quoting.
/// - **WHERE-injection path** (fallback): otherwise the function
///   detects whether the query already has a `WHERE` clause and
///   builds the new predicate as either ` WHERE col > '...'` (no
///   existing WHERE) or ` AND col > '...'` (existing WHERE). The
///   predicate is inserted **before** any `ORDER BY` / `LIMIT` /
///   `GROUP BY` / `HAVING` clauses so the resulting SQL stays valid.
/// - If there is no prior state (first run), the query is returned
///   unchanged.
/// - For HTTP sources using `param_name`, injection is handled at the
///   orchestrator level, not here.
///
/// **Limitations of the WHERE-injection path** (Phase 6A.2 item 1.6
/// Decision Matrix W1, W2, W6): the case-insensitive keyword scan
/// doesn't understand quoted-string literals, CTEs, or subqueries.
/// If your query uses any of those AND requires watermark injection,
/// embed `{{watermark}}` explicitly — the placeholder path is the
/// safe escape valve. The injection path is correct for the common
/// cases (`SELECT ... FROM t [WHERE ...] [ORDER BY ...] [LIMIT ...]`).
///
/// Single quotes in `last_value` are escaped via SQL doubling
/// (`'` → `''`).
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
    let escaped = escape_sql_quotes(last_value);

    // Phase 6A.2 item 1.6 W3: keep `{{watermark}}` as the canonical
    // placeholder syntax (matches the existing tests + handoff
    // recommendation that complex queries route through the
    // placeholder path).
    if query.contains("{{watermark}}") {
        return query.replace("{{watermark}}", &escaped);
    }

    let predicate_body = format!("{} > '{escaped}'", config.column);
    let where_keyword = if has_existing_where(query) {
        "AND"
    } else {
        "WHERE"
    };
    let injected = format!(" {where_keyword} {predicate_body}");
    insert_before_clause_keywords(query, &injected)
}

/// Replace each `'` in `s` with `''` (SQL string-escaping). Defensive
/// for `last_value`s that contain a single quote.
fn escape_sql_quotes(s: &str) -> String {
    s.replace('\'', "''")
}

/// Case-insensitive ASCII-only scan that returns true iff `query`
/// contains a `WHERE` keyword at a word boundary.
fn has_existing_where(query: &str) -> bool {
    find_keyword_ci(query, "WHERE").is_some()
}

/// Insert `clause` immediately before the first occurrence (case-
/// insensitive, word-boundary) of any of `ORDER BY`, `LIMIT`,
/// `GROUP BY`, `HAVING`. If none are present, append at the end
/// (after trimming trailing whitespace / `;`).
///
/// Phase 6A.2 item 1.6 W5.
fn insert_before_clause_keywords(query: &str, clause: &str) -> String {
    let positions: [Option<usize>; 4] = [
        find_keyword_ci(query, "ORDER BY"),
        find_keyword_ci(query, "GROUP BY"),
        find_keyword_ci(query, "HAVING"),
        find_keyword_ci(query, "LIMIT"),
    ];
    let earliest = positions.iter().filter_map(|p| *p).min();
    match earliest {
        Some(idx) => {
            let mut left = &query[..idx];
            // Avoid a double space if `query` already had whitespace
            // before the trailing clause.
            while left.ends_with(' ') {
                left = &left[..left.len() - 1];
            }
            format!("{left}{clause} {}", &query[idx..])
        }
        None => {
            let mut trimmed = query.trim_end();
            // Strip a trailing ';' so the new clause doesn't land
            // after the statement terminator.
            if trimmed.ends_with(';') {
                trimmed = &trimmed[..trimmed.len() - 1];
                trimmed = trimmed.trim_end();
            }
            // Re-attach the ';' if the original had one.
            let suffix = if query.trim_end_matches(char::is_whitespace).ends_with(';') {
                ";"
            } else {
                ""
            };
            format!("{trimmed}{clause}{suffix}")
        }
    }
}

/// Find a case-insensitive ASCII keyword in `haystack` at a word
/// boundary. Treats SQL keywords as ASCII; matches `WHERE`,
/// `where`, `Where` equivalently. Multi-word keywords like
/// `ORDER BY` accept any run of whitespace between the words.
fn find_keyword_ci(haystack: &str, keyword: &str) -> Option<usize> {
    let bytes = haystack.as_bytes();
    let parts: Vec<&str> = keyword.split_whitespace().collect();
    let first = parts.first()?.as_bytes();
    let mut i = 0;
    while i + first.len() <= bytes.len() {
        if matches_word(bytes, i, first) {
            // Try to match remaining parts after whitespace.
            let mut j = i + first.len();
            let mut ok = true;
            for part in parts.iter().skip(1) {
                let pb = part.as_bytes();
                // Require at least one whitespace.
                let mut ws = j;
                while ws < bytes.len() && is_ascii_ws(bytes[ws]) {
                    ws += 1;
                }
                if ws == j {
                    ok = false;
                    break;
                }
                if ws + pb.len() > bytes.len() {
                    ok = false;
                    break;
                }
                if !matches_word(bytes, ws, pb) {
                    ok = false;
                    break;
                }
                j = ws + pb.len();
            }
            if ok {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn matches_word(haystack: &[u8], start: usize, needle: &[u8]) -> bool {
    if start + needle.len() > haystack.len() {
        return false;
    }
    let prev_ok = start == 0 || !is_word_byte(haystack[start - 1]);
    let next_ok =
        start + needle.len() == haystack.len() || !is_word_byte(haystack[start + needle.len()]);
    if !(prev_ok && next_ok) {
        return false;
    }
    haystack[start..start + needle.len()].eq_ignore_ascii_case(needle)
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn is_ascii_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
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

    // -----------------------------------------------------------------
    // Phase 6A.2 item 1.6 regression tests for WHERE-injection
    // -----------------------------------------------------------------

    #[test]
    fn t_inject_watermark_no_existing_where() {
        // Already covered by `t_inject_watermark_appends_where` above —
        // pinned here under the 6A.2 test naming convention so the
        // handoff regression matrix can find it.
        let config = make_config();
        let state = Some(make_state(Some("2026-05-01")));
        let result = inject_watermark("SELECT * FROM events", &config, &state);
        assert_eq!(
            result,
            "SELECT * FROM events WHERE updated_at > '2026-05-01'"
        );
    }

    #[test]
    fn t_inject_watermark_with_existing_where_uses_and() {
        let config = make_config();
        let state = Some(make_state(Some("2026-05-01")));
        let result = inject_watermark("SELECT * FROM events WHERE tenant_id = 7", &config, &state);
        // Phase 6A.2 item 1.6: append AND, not a second WHERE.
        assert_eq!(
            result,
            "SELECT * FROM events WHERE tenant_id = 7 AND updated_at > '2026-05-01'"
        );
        // Sanity: only one WHERE keyword in the result.
        let where_count = result
            .split_whitespace()
            .filter(|w| w.eq_ignore_ascii_case("WHERE"))
            .count();
        assert_eq!(where_count, 1, "must not produce two WHERE clauses");
    }

    #[test]
    fn t_inject_watermark_with_order_by_inserts_before() {
        let config = make_config();
        let state = Some(make_state(Some("2026-05-01")));
        let result = inject_watermark("SELECT * FROM events ORDER BY id DESC", &config, &state);
        assert_eq!(
            result, "SELECT * FROM events WHERE updated_at > '2026-05-01' ORDER BY id DESC",
            "watermark predicate must land before ORDER BY (item 1.6 W5)"
        );
        // With LIMIT after the ORDER BY too:
        let result = inject_watermark(
            "SELECT * FROM events WHERE tenant_id = 7 ORDER BY id LIMIT 100",
            &config,
            &state,
        );
        assert_eq!(
            result,
            "SELECT * FROM events WHERE tenant_id = 7 AND updated_at > '2026-05-01' ORDER BY id LIMIT 100"
        );
    }

    #[test]
    fn t_inject_watermark_placeholder_used_when_present() {
        let config = make_config();
        let state = Some(make_state(Some("2026-05-01")));
        // Placeholder path bypasses the keyword scan entirely (safe
        // escape valve for queries the scanner can't parse — CTEs,
        // subqueries, quoted-string WHEREs).
        let q = "WITH t AS (SELECT id, value FROM raw WHERE 'WHERE' != 'noise') \
                 SELECT * FROM t WHERE updated_at > '{{watermark}}' ORDER BY id";
        let result = inject_watermark(q, &config, &state);
        assert!(
            result.contains("'2026-05-01'"),
            "placeholder must be substituted: {result}"
        );
        assert!(
            !result.contains("{{watermark}}"),
            "no placeholder should remain"
        );
    }

    #[test]
    fn t_inject_watermark_escapes_single_quote_in_value() {
        let config = make_config();
        let state = Some(make_state(Some("O'Brien")));
        let result = inject_watermark("SELECT * FROM events", &config, &state);
        assert_eq!(
            result, "SELECT * FROM events WHERE updated_at > 'O''Brien'",
            "single quote in last_value must be escaped via SQL doubling (item 1.6 W7)"
        );
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
