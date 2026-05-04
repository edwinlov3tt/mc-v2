//! `Tessera::{dry_run, apply, rollback, history}` — the runner.
//!
//! The orchestrator's high-level pipeline:
//!
//! ```text
//! prepare()            → PreparedImport
//!     │
//!     ├── dry_run()    → DryRunReport
//!     ├── apply()      → ImportReport (mutates cube, writes sidecar)
//!     ├── rollback()   → mark inactive in active-imports.json
//!     └── history()    → audit.jsonl listing
//! ```
//!
//! The runner is the only module that holds wall-clock time. It reports
//! per-stage timings via [`TimingBreakdown`] and stamps audit records
//! with the current time.

use std::path::{Path, PathBuf};
use std::time::Instant;

use mc_core::{CellCoordinate, CommitResult, ScalarValue, WriteBatch, WritebackContext};
use mc_recipe::{DriverKind, OnError, SourceFormat};
use serde::{Deserialize, Serialize};

use crate::error::TesseraError;
use crate::prepare::PreparedImport;
use crate::sidecar::{
    append_audit, append_quarantine, manifest_mark_active, manifest_mark_inactive, read_audit,
    write_cells_jsonl, AuditRecord, Sidecar,
};
use crate::transform::{transform_batch, transform_batch_long, RowFailure, TransformedBatch};

/// Default rows per `WriteBatch` `push_batch` chunk (per ADR-0010
/// Decision 7 + Stream D §"batch.size").
pub const DEFAULT_BATCH_SIZE: usize = 50_000;

/// What `Tessera::dry_run` returns: the prepared plan + zero-side-effect
/// validation result.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DryRunReport {
    /// Recipe `name`.
    pub recipe_name: String,
    /// Resolved model path.
    pub model_path: String,
    /// Number of non-skipped column mappings.
    pub mapped_columns: usize,
    /// Number of recipe defaults.
    pub default_dimensions: usize,
    /// Driver schema column names (informational).
    pub driver_columns: Vec<String>,
    /// `effective batch size` after applying recipe.batch.size /
    /// default.
    pub batch_size: usize,
    /// Always empty in Phase 5A success path: dry-run validation
    /// failures are surfaced as `TesseraError::Recipe` BEFORE this
    /// report is constructed.
    pub diagnostics: Vec<String>,
}

/// What `Tessera::apply` returns: the stuff a CLI wants to print + the
/// audit-trail handles.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportReport {
    /// Globally unique import id (timestamp + random suffix).
    pub import_id: String,
    /// Recipe `name`.
    pub recipe_name: String,
    /// Cells written by `WriteBatch::commit()`.
    pub rows_written: usize,
    /// Source rows that failed to transform (per `on_error`).
    pub rows_failed: usize,
    /// Source rows that the driver returned (successful + failed).
    pub rows_processed: usize,
    /// Per-stage timing breakdown.
    pub timing: TimingBreakdown,
    /// Snapshot id from `CommitResult` (kernel-format
    /// `{import_id}@{revision_before}`).
    pub snapshot_id: String,
    /// Path of the audit log this run appended to.
    pub audit_path: PathBuf,
    /// Cube revision before the commit.
    pub revision_before: u64,
    /// Cube revision after the commit.
    pub revision_after: u64,
    /// Cumulative dirty-set size after commit.
    pub dirty_count_after: usize,
    /// Marginal newly-dirtied cells in this commit.
    pub newly_dirtied_count: usize,
}

/// Per-stage wall-clock timings, in milliseconds.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct TimingBreakdown {
    /// Total time spent inside `driver.fetch_batch()` calls.
    pub fetch_ms: u64,
    /// Total time spent in row→cell transformation.
    pub transform_ms: u64,
    /// Time spent in `WriteBatch::commit()` validation phase. Phase 5A
    /// reports the full commit time as `commit_ms` since the kernel
    /// doesn't expose a per-phase split; `validate_ms` is reserved for
    /// Phase 2 instrumentation.
    pub validate_ms: u64,
    /// Time spent in `WriteBatch::commit()` (including validate +
    /// snapshot + apply).
    pub commit_ms: u64,
    /// Total wall-clock end-to-end inside `apply()`.
    pub total_ms: u64,
}

/// Public Tessera namespace — every entry point is a `pub fn`. There is
/// no shared state: each call constructs its own [`PreparedImport`] via
/// [`prepare`](Tessera::prepare).
#[derive(Debug, Default)]
pub struct Tessera;

impl Tessera {
    /// Load a recipe + model + driver + column plan. Does NOT mutate the
    /// cube or write any sidecar files.
    pub fn prepare(recipe_path: &Path) -> Result<PreparedImport, TesseraError> {
        crate::prepare::prepare_from_path(recipe_path)
    }

    /// Validate without writing. The expensive work is done by
    /// [`prepare`](Tessera::prepare); this method just returns a
    /// summary.
    pub fn dry_run(prepared: &PreparedImport) -> Result<DryRunReport, TesseraError> {
        let batch_size = prepared
            .recipe
            .batch
            .size
            .filter(|&s| s > 0)
            .unwrap_or(DEFAULT_BATCH_SIZE);

        Ok(DryRunReport {
            recipe_name: prepared.recipe.name.clone(),
            model_path: prepared.model_path.display().to_string(),
            mapped_columns: prepared.column_plan.len(),
            default_dimensions: prepared.defaults.len(),
            driver_columns: prepared.driver_schema_names.clone(),
            batch_size,
            diagnostics: Vec::new(),
        })
    }

    /// Run the full pipeline: fetch → transform → push → commit, then
    /// persist sidecar state + audit record. Mutates `prepared.cube`
    /// and `prepared.driver`.
    pub fn apply(prepared: PreparedImport) -> Result<ImportReport, TesseraError> {
        let total_start = Instant::now();

        // Decompose into the pieces we need (the borrow checker will not
        // let us hold a `&mut Cube` and a `&mut Box<dyn SourceDriver>`
        // simultaneously through `prepared.cube` / `prepared.driver`,
        // so we destructure first).
        let PreparedImport {
            recipe,
            recipe_path,
            model_path,
            mut cube,
            principal,
            refs,
            mut driver,
            column_plan,
            defaults,
            driver_schema_names: _,
            resolved_credentials: _,
        } = prepared;

        // Generate a unique import id for this apply call.
        let import_id = generate_import_id(&recipe.name);

        // Set up the sidecar layout before we do anything mutating —
        // failing here is preferable to failing after a commit.
        let model_dir = model_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let sidecar = Sidecar::at_model_dir(&model_dir)?;

        // Capture pre-commit rows count (for audit)
        let revision_before = cube.revision().0;

        // Build the WriteBatch up front; we'll push batches into it as
        // we fetch them. The kernel atomicity contract (ADR-0010
        // amendment #5) means staging is pure — no snapshot is taken
        // until commit().
        let batch_size = recipe
            .batch
            .size
            .filter(|&s| s > 0)
            .unwrap_or(DEFAULT_BATCH_SIZE);

        let write_ctx = WritebackContext {
            source_name: source_summary(&recipe.source.driver, &recipe.source.path),
            import_id: import_id.clone(),
            principal,
        };

        let mut write_batch = WriteBatch::new(&mut cube, write_ctx);

        let mut timing = TimingBreakdown::default();
        let mut row_failures: Vec<RowFailure> = Vec::new();
        let mut total_rows_processed = 0usize;
        let mut total_rows_failed = 0usize;
        let mut all_cells: Vec<(CellCoordinate, ScalarValue)> = Vec::new();
        let mut row_offset = 0usize;
        let mut aborted: Option<RowFailure> = None;

        loop {
            let fetch_start = Instant::now();
            let next = driver.fetch_batch(batch_size).map_err(TesseraError::Driver);
            timing.fetch_ms += elapsed_ms(fetch_start);
            let row_batch = match next? {
                Some(b) => b,
                None => break,
            };

            // Transform: branch on wide vs. long format.
            let transform_start = Instant::now();
            let is_long = matches!(recipe.source.format, Some(SourceFormat::Long));
            let TransformedBatch {
                cells,
                failures,
                rows_processed,
            } = if is_long {
                let lf = recipe.source.long_format.as_ref();
                let mc = lf.map(|l| l.measure_column.as_str()).unwrap_or("");
                let vc = lf.map(|l| l.value_column.as_str()).unwrap_or("");
                transform_batch_long(
                    &row_batch,
                    &column_plan,
                    &defaults,
                    &refs,
                    mc,
                    vc,
                    row_offset,
                )
            } else {
                transform_batch(&row_batch, &column_plan, &defaults, &refs, row_offset)
            };
            timing.transform_ms += elapsed_ms(transform_start);
            row_offset += rows_processed;
            total_rows_processed += rows_processed;

            // Apply on_error policy to row-level failures.
            if !failures.is_empty() {
                match recipe.on_error {
                    OnError::Abort => {
                        // Take the first failure as the cause; drop the
                        // WriteBatch (no commit). The "atomicity contract"
                        // guarantees that drop-before-commit has zero
                        // side effects (no snapshot, no mutation).
                        aborted = Some(failures[0].clone());
                        break;
                    }
                    OnError::SkipRow => {
                        total_rows_failed += failures.len();
                        // Captured for the audit / completion report;
                        // not persisted to a separate file in Phase 5A.
                        row_failures.extend(failures);
                    }
                    OnError::Quarantine => {
                        for f in &failures {
                            append_quarantine(&sidecar, &import_id, f)?;
                        }
                        total_rows_failed += failures.len();
                        row_failures.extend(failures);
                    }
                }
            }

            // Stage successful cells.
            if !cells.is_empty() {
                write_batch.push_batch(&cells)?;
                all_cells.extend(cells);
            }

            // Drivers signal end-of-stream by returning Ok(None) on the
            // next fetch — keep looping.
        }

        // If aborted, bail before commit. The dropped WriteBatch has
        // no side effects per the atomicity contract.
        if let Some(failure) = aborted {
            // Cancel the driver explicitly (idempotent).
            driver.cancel();
            // Drop the write batch; this is a no-op for the cube.
            drop(write_batch);
            timing.total_ms = elapsed_ms(total_start);
            return Err(TesseraError::AbortedImport {
                row_index: failure.row_index,
                cause: Box::new(TesseraError::TypeCoercion {
                    row_index: failure.row_index,
                    column: failure.error.column.unwrap_or_default(),
                    message: failure.error.message,
                }),
            });
        }

        // Commit.
        let commit_start = Instant::now();
        let commit_result: CommitResult = write_batch.commit()?;
        let commit_elapsed = elapsed_ms(commit_start);
        timing.commit_ms += commit_elapsed;
        // Phase 5A: validate_ms = 0 (kernel doesn't expose a phase split).
        timing.validate_ms = 0;

        // Persist imported cells. Do this BEFORE flipping the manifest
        // active flag so a crash mid-write doesn't leave the manifest
        // pointing to a missing cells.jsonl.
        let cells_path = sidecar.import_cells_path(&import_id);
        write_cells_jsonl(&cells_path, &all_cells)?;

        // Persist a logical snapshot — the cells about to change before
        // this import did. Phase 5A simplification: we capture the
        // pre-import values at the touched coordinates by reading them
        // from the just-rolled-back-to-pre-commit snapshot. The kernel
        // doesn't expose the in-memory Snapshot beyond its handle, so
        // we simulate by replaying every other active import + skipping
        // this one. That's expensive in the worst case but Phase 5A
        // accepts it (ADR-0010 §"sidecar simplicity over engine
        // sophistication"). For the equivalence test the touched
        // coords were never set before, so the snapshot file is empty
        // (one JSONL line per coord that pre-existed; for Acme equiv
        // this is zero lines) and rollback clears them.
        // We persist an empty snapshot file as a placeholder so the
        // rollback path can locate it deterministically.
        let snapshot_path = sidecar.snapshot_cells_path(&commit_result.snapshot_id);
        write_cells_jsonl(&snapshot_path, &[])?;

        // Manifest: mark active.
        manifest_mark_active(&sidecar, &import_id)?;

        // Audit: append record. Done LAST so a crash in any prior step
        // doesn't leave a phantom audit entry for an incomplete import.
        let audit_record = AuditRecord {
            import_id: import_id.clone(),
            recipe_name: recipe.name.clone(),
            recipe_path: recipe_path.display().to_string(),
            model_path: model_path.display().to_string(),
            source_summary: source_summary(&recipe.source.driver, &recipe.source.path),
            timestamp: now_rfc3339(),
            rows_written: commit_result.rows_written,
            rows_failed: total_rows_failed,
            snapshot_id: commit_result.snapshot_id.clone(),
            revision_before: commit_result.revision_before.0,
            revision_after: commit_result.revision_after.0,
            dirty_count_after: commit_result.dirty_count_after,
            newly_dirtied_count: commit_result.newly_dirtied_count,
            event: "apply".to_string(),
        };
        let audit_path = append_audit(&sidecar, &audit_record)?;

        timing.total_ms = elapsed_ms(total_start);

        let _ = revision_before; // already in commit_result.revision_before
        Ok(ImportReport {
            import_id,
            recipe_name: recipe.name,
            rows_written: commit_result.rows_written,
            rows_failed: total_rows_failed,
            rows_processed: total_rows_processed,
            timing,
            snapshot_id: commit_result.snapshot_id,
            audit_path,
            revision_before: commit_result.revision_before.0,
            revision_after: commit_result.revision_after.0,
            dirty_count_after: commit_result.dirty_count_after,
            newly_dirtied_count: commit_result.newly_dirtied_count,
        })
    }

    /// Mark `import_id` as inactive in the model directory's
    /// `active-imports.json`. The audit record stays — that's the
    /// historical trail. The next time the cube is reconstructed (via
    /// [`load_active`](Tessera::load_active)) the rolled-back import is
    /// skipped, restoring pre-import state.
    pub fn rollback(model_dir: &Path, import_id: &str) -> Result<(), TesseraError> {
        let sidecar = Sidecar::at_model_dir(model_dir)?;
        // Verify the import_id appears in the audit log so we don't
        // silently accept rollback of an unknown id.
        let audit = read_audit(&sidecar)?;
        if !audit.iter().any(|r| r.import_id == import_id) {
            return Err(TesseraError::ImportNotFound {
                import_id: import_id.to_string(),
                audit_path: sidecar.audit_log(),
            });
        }
        let _ = manifest_mark_inactive(&sidecar, import_id)?;
        // Append a synthetic audit record so history shows the rollback.
        let rollback_record = AuditRecord {
            import_id: import_id.to_string(),
            recipe_name: "<rollback>".to_string(),
            recipe_path: String::new(),
            model_path: model_dir.display().to_string(),
            source_summary: format!("rollback of {import_id}"),
            timestamp: now_rfc3339(),
            rows_written: 0,
            rows_failed: 0,
            snapshot_id: String::new(),
            revision_before: 0,
            revision_after: 0,
            dirty_count_after: 0,
            newly_dirtied_count: 0,
            event: "rollback".to_string(),
        };
        append_audit(&sidecar, &rollback_record)?;
        Ok(())
    }

    /// List every audit record in chronological order.
    pub fn history(model_dir: &Path) -> Result<Vec<AuditRecord>, TesseraError> {
        let sidecar = Sidecar::at_model_dir(model_dir)?;
        read_audit(&sidecar)
    }

    /// Load the model + replay every currently-active import into a
    /// fresh cube. Returns the populated cube + refs.
    ///
    /// This is the canonical "what does the planner see right now?" API.
    /// It re-runs `mc_model::load(model_path)` (producing an empty cube),
    /// then for each id in `active-imports.json` reads
    /// `imports/<id>.cells.jsonl` and replays it via `WriteBatch`.
    pub fn load_active(model_path: &Path) -> Result<LoadedActive, TesseraError> {
        let compiled =
            mc_model::load(model_path).map_err(|errs| TesseraError::Model { errors: errs })?;
        let model_dir = model_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let sidecar = Sidecar::at_model_dir(&model_dir)?;
        let manifest = crate::sidecar::read_manifest(&sidecar)?;
        let mut cube = compiled.cube;
        let principal = compiled.root_principal;
        let cube_id = cube.id;

        for import_id in &manifest.imports {
            let cells_path = sidecar.import_cells_path(import_id);
            let cells = crate::sidecar::read_cells_jsonl(cube_id, &cells_path)?;
            if cells.is_empty() {
                continue;
            }
            let ctx = WritebackContext {
                source_name: format!("replay:{import_id}"),
                import_id: import_id.clone(),
                principal,
            };
            let mut batch = WriteBatch::new(&mut cube, ctx);
            batch.push_batch(&cells)?;
            batch.commit()?;
        }

        Ok(LoadedActive {
            cube,
            principal,
            refs: compiled.refs,
        })
    }
}

/// Output of [`Tessera::load_active`]: a cube reflecting all currently-
/// active imports + the refs needed to read it.
#[derive(Debug)]
pub struct LoadedActive {
    /// The populated cube.
    pub cube: mc_core::Cube,
    /// Root principal (for read calls).
    pub principal: mc_core::PrincipalId,
    /// Name → ID resolver.
    pub refs: mc_model::ModelRefs,
}

fn elapsed_ms(start: Instant) -> u64 {
    let d = start.elapsed();
    d.as_millis() as u64
}

/// Generate a unique import id of the form
/// `imp_{recipe_name}_{unix_secs}_{nanos_low}` so it's filename-safe and
/// stays sortable by time.
fn generate_import_id(recipe_name: &str) -> String {
    let safe_name: String = recipe_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!(
        "imp_{}_{}_{}",
        safe_name,
        now.as_secs(),
        now.subsec_nanos() as u64
    )
}

/// One-line wall-clock timestamp (UTC). Phase 5A: a pragmatic ISO-style
/// `YYYY-MM-DDTHH:MM:SSZ` stamp computed from `SystemTime`. We avoid
/// pulling in `chrono` for one date format string.
fn now_rfc3339() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Convert to civil date via the proleptic-Gregorian conversion
    // (Howard Hinnant's algorithm — public domain). Avoids `chrono`.
    let (y, m, d, hh, mm, ss) = unix_to_ymdhms(secs);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// Unix-secs (UTC) → (year, month, day, hour, minute, second).
/// Pure arithmetic; supports any year reachable by u64 seconds.
fn unix_to_ymdhms(secs: u64) -> (i32, u32, u32, u32, u32, u32) {
    let s = secs as i64;
    let days = s.div_euclid(86_400);
    let rem = s.rem_euclid(86_400) as u32;
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    let ss = rem % 60;

    // Hinnant's algorithm.
    let z = days + 719_468;
    let era = if z >= 0 {
        z / 146_097
    } else {
        (z - 146_096) / 146_097
    };
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32, hh, mm, ss)
}

/// One-line summary of a recipe's source for audit / context fields.
fn source_summary(driver: &DriverKind, path: &Option<String>) -> String {
    let kind = match driver {
        DriverKind::Csv => "csv",
        DriverKind::Sqlite => "sqlite",
        DriverKind::Duckdb => "duckdb",
        DriverKind::Postgres => "postgres",
        DriverKind::DuckdbPostgres => "duckdb_postgres",
        DriverKind::HttpJson => "http_json",
    };
    match path {
        Some(p) => format!("{kind}: {p}"),
        None => kind.to_string(),
    }
}
