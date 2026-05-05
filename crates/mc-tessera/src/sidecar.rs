//! `.tessera/` sidecar state model.
//!
//! Per ADR-0010 Decision 2.5 + the Stream D handoff:
//!
//! ```text
//! <model_dir>/.tessera/
//! ├── audit.jsonl                  append-only; one JSON record per import
//! ├── imports/
//! │   └── <import_id>.cells.jsonl  cells written by this import
//! ├── snapshots/
//! │   └── <snapshot_id>.cells.jsonl pre-commit snapshot (for rollback)
//! ├── quarantine/
//! │   └── <import_id>.jsonl        quarantined rows (on_error: quarantine)
//! └── active-imports.json          manifest of currently-active imports
//! ```
//!
//! All formats are JSON Lines (`.jsonl`) or JSON (`active-imports.json`).
//! No binary serialization. The directory is `.gitignore`-able and
//! safe to delete (re-running `mc tessera apply` regenerates it).

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use mc_core::{CellCoordinate, ElementId, ScalarValue};
use serde::{Deserialize, Serialize};

use crate::error::TesseraError;
use crate::transform::RowFailure;

/// Top-level sidecar layout for a given model directory.
#[derive(Clone, Debug)]
pub struct Sidecar {
    /// Root directory: `<model_dir>/.tessera/`.
    pub root: PathBuf,
}

impl Sidecar {
    /// Build the sidecar layout for `model_dir`. Creates the directory
    /// tree (idempotent) so callers can immediately read/write.
    pub fn at_model_dir(model_dir: &Path) -> Result<Self, TesseraError> {
        let root = model_dir.join(".tessera");
        fs::create_dir_all(&root).map_err(|e| TesseraError::io(&root, e))?;
        fs::create_dir_all(root.join("imports"))
            .map_err(|e| TesseraError::io(root.join("imports"), e))?;
        fs::create_dir_all(root.join("snapshots"))
            .map_err(|e| TesseraError::io(root.join("snapshots"), e))?;
        fs::create_dir_all(root.join("quarantine"))
            .map_err(|e| TesseraError::io(root.join("quarantine"), e))?;
        Ok(Sidecar { root })
    }

    /// Path to `audit.jsonl`.
    pub fn audit_log(&self) -> PathBuf {
        self.root.join("audit.jsonl")
    }

    /// Path to `active-imports.json`.
    pub fn manifest_path(&self) -> PathBuf {
        self.root.join("active-imports.json")
    }

    /// Path to `imports/<import_id>.cells.jsonl`.
    pub fn import_cells_path(&self, import_id: &str) -> PathBuf {
        self.root
            .join("imports")
            .join(format!("{import_id}.cells.jsonl"))
    }

    /// Path to `snapshots/<snapshot_id>.cells.jsonl`.
    pub fn snapshot_cells_path(&self, snapshot_id: &str) -> PathBuf {
        self.root
            .join("snapshots")
            .join(format!("{}.cells.jsonl", sanitize(snapshot_id)))
    }

    /// Path to `quarantine/<import_id>.jsonl`.
    pub fn quarantine_path(&self, import_id: &str) -> PathBuf {
        self.root
            .join("quarantine")
            .join(format!("{import_id}.jsonl"))
    }
}

/// Replace characters that are awkward in filenames with `_`. Used for
/// snapshot-id filenames (snapshot ids are `{import_id}@{revision}` and
/// the `@` is a fine filename char on Unix but reserved on some Windows
/// share configurations).
fn sanitize(id: &str) -> String {
    id.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            other => other,
        })
        .collect()
}

// ============================================================================
// Audit log + active-imports manifest.
// ============================================================================

/// One audit record per import — one JSON line in `audit.jsonl`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditRecord {
    /// Globally unique import id.
    pub import_id: String,
    /// Recipe `name` field at the time of the import.
    pub recipe_name: String,
    /// Recipe path on disk at the time of the import (best-effort —
    /// recipes can be moved post-import; the audit record preserves the
    /// original location).
    pub recipe_path: String,
    /// Resolved model path at the time of the import.
    pub model_path: String,
    /// Source-of-truth one-line summary of the source (e.g.,
    /// `"csv: ./data/q3.csv"`).
    pub source_summary: String,
    /// Timestamp of the import (RFC3339-style; in Phase 5A this is the
    /// process wall clock at write time).
    pub timestamp: String,
    /// Cells written by `WriteBatch::commit()`.
    pub rows_written: usize,
    /// Rows that failed to transform (per `on_error`).
    pub rows_failed: usize,
    /// Pre-commit snapshot identifier (kernel-format
    /// `{import_id}@{revision_before}`).
    pub snapshot_id: String,
    /// Cube revision before this import committed.
    pub revision_before: u64,
    /// Cube revision after this import committed.
    pub revision_after: u64,
    /// `dirty_count_after` (cumulative dirty-set size).
    pub dirty_count_after: usize,
    /// `newly_dirtied_count` (marginal: clean → dirty during this commit).
    pub newly_dirtied_count: usize,
    /// `apply` (the only Phase 5A event kind). `dry_run` does NOT
    /// produce audit records.
    pub event: String,
}

/// Append `record` to `audit.jsonl` (creating it if absent).
pub fn append_audit(sidecar: &Sidecar, record: &AuditRecord) -> Result<PathBuf, TesseraError> {
    let path = sidecar.audit_log();
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| TesseraError::io(&path, e))?;
    let json = serde_json::to_string(record).map_err(|e| TesseraError::SidecarSerialize {
        message: e.to_string(),
    })?;
    writeln!(file, "{json}").map_err(|e| TesseraError::io(&path, e))?;
    Ok(path)
}

/// Read every record from `audit.jsonl`. Returns an empty Vec when the
/// file doesn't exist.
pub fn read_audit(sidecar: &Sidecar) -> Result<Vec<AuditRecord>, TesseraError> {
    let path = sidecar.audit_log();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = std::fs::File::open(&path).map_err(|e| TesseraError::io(&path, e))?;
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for (line_idx, line_result) in reader.lines().enumerate() {
        let line = line_result.map_err(|e| TesseraError::io(&path, e))?;
        if line.trim().is_empty() {
            continue;
        }
        let rec: AuditRecord =
            serde_json::from_str(&line).map_err(|e| TesseraError::SidecarDeserialize {
                path: path.clone(),
                message: format!("audit.jsonl line {}: {e}", line_idx + 1),
            })?;
        out.push(rec);
    }
    Ok(out)
}

/// Manifest of currently-active imports, stored as `active-imports.json`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ActiveImports {
    /// Import ids in order of `apply`. Imports that were rolled back are
    /// REMOVED from this list (the audit trail in `audit.jsonl` is the
    /// historical record).
    #[serde(default)]
    pub imports: Vec<String>,
}

/// Read the active-imports manifest. Returns an empty manifest when the
/// file doesn't exist.
pub fn read_manifest(sidecar: &Sidecar) -> Result<ActiveImports, TesseraError> {
    let path = sidecar.manifest_path();
    if !path.exists() {
        return Ok(ActiveImports::default());
    }
    let body = fs::read_to_string(&path).map_err(|e| TesseraError::io(&path, e))?;
    if body.trim().is_empty() {
        return Ok(ActiveImports::default());
    }
    serde_json::from_str(&body).map_err(|e| TesseraError::SidecarDeserialize {
        path: path.clone(),
        message: e.to_string(),
    })
}

/// Write the active-imports manifest (overwrites).
pub fn write_manifest(sidecar: &Sidecar, manifest: &ActiveImports) -> Result<(), TesseraError> {
    let path = sidecar.manifest_path();
    let body =
        serde_json::to_string_pretty(manifest).map_err(|e| TesseraError::SidecarSerialize {
            message: e.to_string(),
        })?;
    fs::write(&path, body).map_err(|e| TesseraError::io(&path, e))
}

/// Mark `import_id` as active in the manifest (idempotent — appending
/// the same id twice is a no-op).
pub fn manifest_mark_active(sidecar: &Sidecar, import_id: &str) -> Result<(), TesseraError> {
    let mut manifest = read_manifest(sidecar)?;
    if !manifest.imports.iter().any(|s| s == import_id) {
        manifest.imports.push(import_id.to_string());
    }
    write_manifest(sidecar, &manifest)
}

/// Mark `import_id` as inactive (removing from the manifest). Returns
/// `Ok(true)` if the id was present, `Ok(false)` if not.
pub fn manifest_mark_inactive(sidecar: &Sidecar, import_id: &str) -> Result<bool, TesseraError> {
    let mut manifest = read_manifest(sidecar)?;
    let before = manifest.imports.len();
    manifest.imports.retain(|s| s != import_id);
    let removed = manifest.imports.len() != before;
    write_manifest(sidecar, &manifest)?;
    Ok(removed)
}

// ============================================================================
// Cells JSONL — used both for imports/<id> and snapshots/<id>.
// ============================================================================

/// One JSON line per cell, in the per-import / per-snapshot files.
///
/// `coord` is the raw element-id slot vector (in cube dimension order)
/// — replay re-resolves these against a freshly compiled cube via
/// [`crate::Tessera::apply`]. `value` is rendered as `Some(f64)` for
/// `ScalarValue::F64`, `null` for `ScalarValue::Null`, or a tagged
/// object for `I64` / `Bool` / `Category`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CellRecord {
    /// Element ids in canonical cube dimension order.
    pub coord: Vec<u64>,
    /// Cell value.
    pub value: CellValueJson,
}

/// JSON-serializable cell value (mirror of [`mc_core::ScalarValue`]
/// without depending on serde for that type).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CellValueJson {
    /// 64-bit float.
    F64 {
        /// Bits-preserving float representation (so byte-equality holds
        /// across roundtrip).
        bits: u64,
    },
    /// 64-bit integer.
    I64 {
        /// Value.
        value: i64,
    },
    /// Boolean.
    Bool {
        /// Value.
        value: bool,
    },
    /// Category index.
    Category {
        /// Index into the measure's category domain.
        index: usize,
    },
    /// SQL NULL.
    Null,
}

impl CellValueJson {
    /// Encode an [`mc_core::ScalarValue`] as JSON.
    pub fn from_scalar(v: &ScalarValue) -> Self {
        match v {
            ScalarValue::F64(x) => CellValueJson::F64 { bits: x.to_bits() },
            ScalarValue::I64(x) => CellValueJson::I64 { value: *x },
            ScalarValue::Bool(x) => CellValueJson::Bool { value: *x },
            ScalarValue::Category(x) => CellValueJson::Category { index: *x },
            ScalarValue::Str(_) => CellValueJson::Null, // Str is transient; never stored
            ScalarValue::Null => CellValueJson::Null,
        }
    }

    /// Decode back to a [`ScalarValue`].
    pub fn to_scalar(&self) -> ScalarValue {
        match self {
            CellValueJson::F64 { bits } => ScalarValue::F64(f64::from_bits(*bits)),
            CellValueJson::I64 { value } => ScalarValue::I64(*value),
            CellValueJson::Bool { value } => ScalarValue::Bool(*value),
            CellValueJson::Category { index } => ScalarValue::Category(*index),
            CellValueJson::Null => ScalarValue::Null,
        }
    }
}

/// Append every `(coord, value)` pair from `cells` to `path` as JSONL.
/// Truncates the file if it already exists (one import_id, one cells
/// file).
pub fn write_cells_jsonl(
    path: &Path,
    cells: &[(CellCoordinate, ScalarValue)],
) -> Result<(), TesseraError> {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|e| TesseraError::io(path, e))?;
    let mut writer = BufWriter::new(file);
    for (coord, value) in cells {
        let rec = CellRecord {
            coord: coord.elements().iter().map(|e| e.0).collect(),
            value: CellValueJson::from_scalar(value),
        };
        let json = serde_json::to_string(&rec).map_err(|e| TesseraError::SidecarSerialize {
            message: e.to_string(),
        })?;
        writeln!(writer, "{json}").map_err(|e| TesseraError::io(path, e))?;
    }
    writer.flush().map_err(|e| TesseraError::io(path, e))
}

/// Read every cell record from a JSONL file. Returns an empty Vec when
/// the file doesn't exist.
pub fn read_cells_jsonl(
    cube_id: mc_core::CubeId,
    path: &Path,
) -> Result<Vec<(CellCoordinate, ScalarValue)>, TesseraError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = std::fs::File::open(path).map_err(|e| TesseraError::io(path, e))?;
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for (line_idx, line_result) in reader.lines().enumerate() {
        let line = line_result.map_err(|e| TesseraError::io(path, e))?;
        if line.trim().is_empty() {
            continue;
        }
        let rec: CellRecord =
            serde_json::from_str(&line).map_err(|e| TesseraError::SidecarDeserialize {
                path: path.to_path_buf(),
                message: format!("line {}: {e}", line_idx + 1),
            })?;
        let elements = rec.coord.into_iter().map(ElementId);
        let coord = CellCoordinate::from_parts(cube_id, elements);
        out.push((coord, rec.value.to_scalar()));
    }
    Ok(out)
}

// ============================================================================
// Quarantine.
// ============================================================================

/// One quarantined row.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuarantineRecord {
    /// Zero-based row index within the import.
    pub row_index: usize,
    /// Diagnostic code (MC5xxx or short tag).
    pub code: String,
    /// Human-readable message.
    pub message: String,
    /// Optional dimension hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimension: Option<String>,
    /// Optional column hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,
    /// Original row data: column-name → value (or `null` for SQL NULL).
    pub raw: Vec<QuarantineCell>,
}

/// One cell of a quarantined row's raw data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuarantineCell {
    /// Column name.
    pub column: String,
    /// Source value (or `null`).
    pub value: Option<String>,
}

/// Append a quarantine record to `quarantine/<import_id>.jsonl`.
pub fn append_quarantine(
    sidecar: &Sidecar,
    import_id: &str,
    failure: &RowFailure,
) -> Result<(), TesseraError> {
    let path = sidecar.quarantine_path(import_id);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| TesseraError::io(&path, e))?;
    let rec = QuarantineRecord {
        row_index: failure.row_index,
        code: failure.error.code.to_string(),
        message: failure.error.message.clone(),
        dimension: failure.error.dimension.clone(),
        column: failure.error.column.clone(),
        raw: failure
            .raw
            .iter()
            .map(|(c, v)| QuarantineCell {
                column: c.clone(),
                value: v.clone(),
            })
            .collect(),
    };
    let json = serde_json::to_string(&rec).map_err(|e| TesseraError::SidecarSerialize {
        message: e.to_string(),
    })?;
    writeln!(file, "{json}").map_err(|e| TesseraError::io(&path, e))?;
    Ok(())
}
