//! `RowBatch` → `Vec<(CellCoordinate, ScalarValue)>` — the heart of the
//! Tessera orchestrator.
//!
//! For every row in a [`RowBatch`], the transformer:
//!
//! 1. Resolves dimension columns (mapped via `MappingTarget::Dimension`)
//!    by looking up `(dim_name, raw_value)` in
//!    [`mc_model::ModelRefs::elements`] to get an `ElementId`.
//! 2. Applies recipe defaults for dimensions that aren't in the source.
//! 3. Builds a per-row dim-position → ElementId vector.
//! 4. For each measure column (`MappingTarget::Measure`), extracts the
//!    F64 value (with optional `scale` factor), combines it with the
//!    dim-vector + the measure's ElementId, and emits one
//!    `(CellCoordinate, ScalarValue::F64)` pair.
//!
//! One source row can produce **N cells** — one per mapped measure
//! column. The Acme equivalence test relies on this: a row with 6 dim
//! columns + 6 measure columns produces 6 cells.
//!
//! ## `on_error` policy enforcement
//!
//! Per ADR-0010 amendment #9, three policies live here:
//!
//! - **`Abort`** — the first row error stops the batch; the caller
//!   returns [`crate::TesseraError::AbortedImport`] without committing
//!   the WriteBatch.
//! - **`SkipRow`** — failed rows are appended to a per-import "skipped"
//!   log (for the audit record) and counted toward `rows_failed`. The
//!   transformer continues to the next row.
//! - **`Quarantine`** — failed rows are appended to a per-import
//!   `quarantine/<id>.jsonl` file with the original row data + the
//!   diagnostic. Counted toward `rows_failed`. Continues.
//!
//! The transformer does NOT know about disk paths — quarantine writes
//! are the runner's job. The transformer just emits a typed
//! [`RowFailure`] for each failed row, and the runner dispatches.

use mc_core::{CellCoordinate, ElementId, ScalarValue};
use mc_drivers::{Column, ColumnData, RowBatch};
use mc_model::ModelRefs;

use crate::prepare::{MappingTarget, ResolvedColumnMapping, ResolvedDefault};

/// Output of [`transform_batch`]: per-cell write requests + per-row
/// failures.
#[derive(Debug, Default)]
pub struct TransformedBatch {
    /// Successful cells, ready to push into a `WriteBatch`.
    pub cells: Vec<(CellCoordinate, ScalarValue)>,
    /// Row-level failures captured for `on_error: skip_row` /
    /// `on_error: quarantine` handling. For `on_error: abort` the runner
    /// short-circuits on the first failure; this list will then have
    /// length 0 or 1.
    pub failures: Vec<RowFailure>,
    /// Number of rows the transformer iterated over (including failed
    /// ones). Used for `rows_processed` accounting.
    pub rows_processed: usize,
}

/// One row that failed to transform — keyed for audit / quarantine
/// emission.
#[derive(Clone, Debug)]
pub struct RowFailure {
    /// Zero-based row index within the import (cumulative across batches).
    pub row_index: usize,
    /// The error that caused the failure.
    pub error: TesseraErrorOwned,
    /// The original row's columns as `column_name → string-rendered
    /// value` pairs (so the audit / quarantine record can replay it).
    pub raw: Vec<(String, Option<String>)>,
}

/// Owned, JSON-friendly variant of [`crate::TesseraError`] for the
/// per-row audit + quarantine paths. The full error retains too many
/// non-Clone non-Serialize types for direct embedding; this type carries
/// the user-facing message + the diagnostic-style metadata.
#[derive(Clone, Debug)]
pub struct TesseraErrorOwned {
    /// Diagnostic code (MC5xxx) most-applicable to this failure, or a
    /// short symbolic tag (e.g., `"unknown_element"`).
    pub code: &'static str,
    /// Human-readable message.
    pub message: String,
    /// Optional dimension name (when the failure is element-resolution).
    pub dimension: Option<String>,
    /// Optional column name (when the failure is type-coercion).
    pub column: Option<String>,
}

/// Run the per-row transformation against a fetched batch.
///
/// `row_index_offset` is the cumulative row index across all batches so
/// the per-row counters stay coherent across multiple `fetch_batch()`
/// calls.
pub fn transform_batch(
    batch: &RowBatch,
    plan: &[ResolvedColumnMapping],
    defaults: &[ResolvedDefault],
    refs: &ModelRefs,
    row_index_offset: usize,
) -> TransformedBatch {
    let mut out = TransformedBatch {
        cells: Vec::with_capacity(batch.row_count.saturating_mul(plan.len())),
        failures: Vec::new(),
        rows_processed: batch.row_count,
    };

    // The cube's coordinate slot count = number of dimensions.
    let n_dims = refs.dimension_order.len();

    // Per-row coordinate scratch; reused across rows. Preinitialized
    // with default ElementId(0); every slot is filled by either a
    // dimension column, a default, or (for the Measure dim) a
    // per-measure-column override.
    let mut slots: Vec<Option<ElementId>> = vec![None; n_dims];

    // The Measure dim's slot index, if the cube has one. `mc_core`'s
    // canonical dim order is `[Scenario, Version, Time, Channel, Market,
    // Measure]`, but the cube model is general; if `Measure` isn't
    // present the field is None and the emit step skips the override.
    let measure_dim_position = refs.dimension_order.iter().position(|n| n == "Measure");

    // If the recipe has any measure-column mappings AND the cube has a
    // `Measure` dimension, that slot will be filled per-emitted-cell.
    // The pre-flight "every slot filled" check uses this flag to avoid
    // false-positives on the Measure slot.
    let has_measure_mapping = plan
        .iter()
        .any(|m| matches!(m.target, MappingTarget::Measure { .. }));

    // Pre-fill defaults — they don't change per row.
    for d in defaults {
        if let Some(slot) = slots.get_mut(d.dim_position) {
            *slot = Some(d.element_id);
        }
    }

    'rows: for row in 0..batch.row_count {
        let row_index = row_index_offset + row;

        // Reset per-row dim slots: re-apply defaults, clear the rest.
        for slot in slots.iter_mut() {
            *slot = None;
        }
        for d in defaults {
            if let Some(slot) = slots.get_mut(d.dim_position) {
                *slot = Some(d.element_id);
            }
        }

        // Step A: resolve dimension columns into slots.
        for entry in plan {
            if let MappingTarget::Dimension {
                dim_name,
                dim_id: _,
                dim_position,
            } = &entry.target
            {
                let raw = column_value_as_string(&batch.columns[entry.source_index], row);
                let raw = match raw {
                    Some(s) => s,
                    None => {
                        record_failure(
                            &mut out,
                            row_index,
                            TesseraErrorOwned {
                                code: "unknown_element",
                                message: format!(
                                    "row {row_index}: missing value for dimension column {:?}",
                                    entry.source
                                ),
                                dimension: Some(dim_name.clone()),
                                column: Some(entry.source.clone()),
                            },
                            row_raw(batch, row),
                        );
                        continue 'rows;
                    }
                };
                let element_id = match refs.element(dim_name, &raw) {
                    Some(id) => id,
                    None => {
                        record_failure(
                            &mut out,
                            row_index,
                            TesseraErrorOwned {
                                code: "MC_unknown_element",
                                message: format!(
                                    "row {row_index}: unknown element {raw:?} in dimension {dim_name:?}"
                                ),
                                dimension: Some(dim_name.clone()),
                                column: Some(entry.source.clone()),
                            },
                            row_raw(batch, row),
                        );
                        continue 'rows;
                    }
                };
                if let Some(slot) = slots.get_mut(*dim_position) {
                    *slot = Some(element_id);
                }
            }
        }

        // Confirm every slot is filled (defaults + dim columns must
        // collectively cover every dimension). The Measure slot is
        // exempt from this pre-flight check when the recipe has any
        // measure-column mappings — those mappings fill it
        // per-emitted-cell. We substitute the first measure mapping's
        // ElementId as a placeholder for the slot-completeness check;
        // it will be overwritten before each cell is emitted.
        let coord_elements: Vec<ElementId> = match slots
            .iter()
            .enumerate()
            .map(|(i, s)| match s {
                Some(id) => Ok(*id),
                None => {
                    if Some(i) == measure_dim_position && has_measure_mapping {
                        // Placeholder: any valid ElementId. The first
                        // measure mapping's element id is guaranteed to
                        // be valid in the Measure dim.
                        let placeholder = plan.iter().find_map(|m| match m.target {
                            MappingTarget::Measure {
                                measure_element_id, ..
                            } => Some(measure_element_id),
                            _ => None,
                        });
                        Ok(placeholder.unwrap_or(ElementId(0)))
                    } else {
                        let dim_name = refs
                            .dimension_order
                            .get(i)
                            .cloned()
                            .unwrap_or_else(|| "<?>".to_string());
                        Err(TesseraErrorOwned {
                            code: "missing_dim_value",
                            message: format!(
                                "row {row_index}: dimension {dim_name:?} has no source column or default"
                            ),
                            dimension: Some(dim_name),
                            column: None,
                        })
                    }
                }
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(v) => v,
            Err(err) => {
                record_failure(&mut out, row_index, err, row_raw(batch, row));
                continue 'rows;
            }
        };

        // Step B: emit one cell per measure column.
        // (Phase 5A note: the Measure dimension's slot is overridden by
        // the measure ElementId for each emitted cell. The dim columns
        // for `Measure` and the per-measure-column overrides interact
        // by overwriting the Measure slot — the recipe schema's "1:1
        // mapping" rule means a recipe shouldn't have BOTH a Measure
        // dim column and per-measure-column targets at the same time;
        // mc-recipe validate is the gatekeeper.)
        //
        // Find any measure-column entries.
        let mut emitted_any = false;
        for entry in plan {
            if let MappingTarget::Measure {
                measure_name,
                measure_element_id,
            } = &entry.target
            {
                let raw_f64 = match extract_f64(&batch.columns[entry.source_index], row) {
                    Ok(Some(v)) => v,
                    Ok(None) => {
                        // Null measure value: treat as a row-level error
                        // per Phase 5A's "no NULL writes" semantic. The
                        // kernel rejects NaN/Inf via WriteBatch::commit
                        // anyway, but Tessera surfaces this earlier with
                        // a typed message.
                        record_failure(
                            &mut out,
                            row_index,
                            TesseraErrorOwned {
                                code: "missing_measure",
                                message: format!(
                                    "row {row_index}: missing value for measure {:?}",
                                    measure_name
                                ),
                                dimension: None,
                                column: Some(entry.source.clone()),
                            },
                            row_raw(batch, row),
                        );
                        continue 'rows;
                    }
                    Err(msg) => {
                        record_failure(
                            &mut out,
                            row_index,
                            TesseraErrorOwned {
                                code: "type_coercion",
                                message: format!(
                                    "row {row_index}: type coercion failed for {:?}: {msg}",
                                    entry.source
                                ),
                                dimension: None,
                                column: Some(entry.source.clone()),
                            },
                            row_raw(batch, row),
                        );
                        continue 'rows;
                    }
                };
                let scaled = match entry.scale {
                    Some(f) => raw_f64 * f,
                    None => raw_f64,
                };
                if !scaled.is_finite() {
                    record_failure(
                        &mut out,
                        row_index,
                        TesseraErrorOwned {
                            code: "type_coercion",
                            message: format!(
                                "row {row_index}: non-finite value after scale for column {:?}",
                                entry.source
                            ),
                            dimension: None,
                            column: Some(entry.source.clone()),
                        },
                        row_raw(batch, row),
                    );
                    continue 'rows;
                }

                // Build a coordinate where the Measure slot is the
                // measure_element_id for THIS column.
                let mut elements = coord_elements.clone();
                if let Some(pos) = measure_dim_position {
                    if let Some(slot) = elements.get_mut(pos) {
                        *slot = *measure_element_id;
                    }
                }
                let coord = CellCoordinate::from_parts(refs.cube_id, elements);
                out.cells.push((coord, ScalarValue::F64(scaled)));
                emitted_any = true;
            }
        }

        // Step C: if no measure columns are mapped (e.g., the recipe is
        // dimension-only — anomalous in Phase 5A but possible), fall
        // through. mc-recipe validate doesn't reject this case today
        // because a "no measure column" recipe is technically valid
        // syntactically; the apply step will commit a 0-cell batch.
        let _ = emitted_any;
    }

    out
}

/// Long-format transformation: each row is one cell. Dimension columns
/// build the coordinate prefix; `measure_column` picks the measure;
/// `value_column` carries the scalar. Per ADR-0010 Amendment 2.
///
/// `row_index_offset` is the cumulative row index across all batches.
pub fn transform_batch_long(
    batch: &RowBatch,
    plan: &[ResolvedColumnMapping],
    defaults: &[ResolvedDefault],
    refs: &ModelRefs,
    measure_column_name: &str,
    value_column_name: &str,
    row_index_offset: usize,
) -> TransformedBatch {
    let mut out = TransformedBatch {
        cells: Vec::with_capacity(batch.row_count),
        failures: Vec::new(),
        rows_processed: batch.row_count,
    };

    let n_dims = refs.dimension_order.len();

    // Find the Measure dimension's slot index.
    let measure_dim_position = refs.dimension_order.iter().position(|n| n == "Measure");

    // Find the source column indices for measure_column and value_column.
    let measure_col_idx = batch
        .columns
        .iter()
        .position(|c| c.name == measure_column_name);
    let value_col_idx = batch
        .columns
        .iter()
        .position(|c| c.name == value_column_name);

    let measure_col_idx = match measure_col_idx {
        Some(i) => i,
        None => {
            // If the measure column is missing from the batch, every row
            // fails. Record one failure for the batch and bail.
            if batch.row_count > 0 {
                record_failure(
                    &mut out,
                    row_index_offset,
                    TesseraErrorOwned {
                        code: "MC5019",
                        message: format!(
                            "long_format.measure_column {measure_column_name:?} not found in source schema"
                        ),
                        dimension: None,
                        column: Some(measure_column_name.to_string()),
                    },
                    row_raw(batch, 0),
                );
            }
            return out;
        }
    };
    let value_col_idx = match value_col_idx {
        Some(i) => i,
        None => {
            if batch.row_count > 0 {
                record_failure(
                    &mut out,
                    row_index_offset,
                    TesseraErrorOwned {
                        code: "MC5020",
                        message: format!(
                            "long_format.value_column {value_column_name:?} not found in source schema"
                        ),
                        dimension: None,
                        column: Some(value_column_name.to_string()),
                    },
                    row_raw(batch, 0),
                );
            }
            return out;
        }
    };

    // Pre-fill defaults.
    let mut slots: Vec<Option<ElementId>> = vec![None; n_dims];
    for d in defaults {
        if let Some(slot) = slots.get_mut(d.dim_position) {
            *slot = Some(d.element_id);
        }
    }

    'rows: for row in 0..batch.row_count {
        let row_index = row_index_offset + row;

        // Reset slots: re-apply defaults.
        for slot in slots.iter_mut() {
            *slot = None;
        }
        for d in defaults {
            if let Some(slot) = slots.get_mut(d.dim_position) {
                *slot = Some(d.element_id);
            }
        }

        // Step A: resolve dimension columns.
        for entry in plan {
            if let MappingTarget::Dimension {
                dim_name,
                dim_id: _,
                dim_position,
            } = &entry.target
            {
                let raw = column_value_as_string(&batch.columns[entry.source_index], row);
                let raw = match raw {
                    Some(s) => s,
                    None => {
                        record_failure(
                            &mut out,
                            row_index,
                            TesseraErrorOwned {
                                code: "unknown_element",
                                message: format!(
                                    "row {row_index}: missing value for dimension column {:?}",
                                    entry.source
                                ),
                                dimension: Some(dim_name.clone()),
                                column: Some(entry.source.clone()),
                            },
                            row_raw(batch, row),
                        );
                        continue 'rows;
                    }
                };
                let element_id = match refs.element(dim_name, &raw) {
                    Some(id) => id,
                    None => {
                        record_failure(
                            &mut out,
                            row_index,
                            TesseraErrorOwned {
                                code: "MC_unknown_element",
                                message: format!(
                                    "row {row_index}: unknown element {raw:?} in dimension {dim_name:?}"
                                ),
                                dimension: Some(dim_name.clone()),
                                column: Some(entry.source.clone()),
                            },
                            row_raw(batch, row),
                        );
                        continue 'rows;
                    }
                };
                if let Some(slot) = slots.get_mut(*dim_position) {
                    *slot = Some(element_id);
                }
            }
        }

        // Step B: resolve measure from the measure_column.
        let measure_name = match column_value_as_string(&batch.columns[measure_col_idx], row) {
            Some(s) => s,
            None => {
                record_failure(
                    &mut out,
                    row_index,
                    TesseraErrorOwned {
                        code: "missing_measure",
                        message: format!(
                            "row {row_index}: missing value in measure column {measure_column_name:?}"
                        ),
                        dimension: None,
                        column: Some(measure_column_name.to_string()),
                    },
                    row_raw(batch, row),
                );
                continue 'rows;
            }
        };

        // Resolve the measure name to an ElementId in the Measure dim.
        let measure_element_id = match refs.element("Measure", &measure_name) {
            Some(id) => id,
            None => {
                record_failure(
                    &mut out,
                    row_index,
                    TesseraErrorOwned {
                        code: "MC5022",
                        message: format!(
                            "row {row_index}: measure column value {measure_name:?} is not a declared measure in the model"
                        ),
                        dimension: Some("Measure".to_string()),
                        column: Some(measure_column_name.to_string()),
                    },
                    row_raw(batch, row),
                );
                continue 'rows;
            }
        };

        // Fill the Measure dim slot.
        if let Some(pos) = measure_dim_position {
            if let Some(slot) = slots.get_mut(pos) {
                *slot = Some(measure_element_id);
            }
        }

        // Confirm every slot is filled.
        let coord_elements: Vec<ElementId> = match slots
            .iter()
            .enumerate()
            .map(|(i, s)| match s {
                Some(id) => Ok(*id),
                None => {
                    let dim_name = refs
                        .dimension_order
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| "<?>".to_string());
                    Err(TesseraErrorOwned {
                        code: "missing_dim_value",
                        message: format!(
                            "row {row_index}: dimension {dim_name:?} has no source column or default"
                        ),
                        dimension: Some(dim_name),
                        column: None,
                    })
                }
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(v) => v,
            Err(err) => {
                record_failure(&mut out, row_index, err, row_raw(batch, row));
                continue 'rows;
            }
        };

        // Step C: extract the value from the value_column.
        let raw_f64 = match extract_f64(&batch.columns[value_col_idx], row) {
            Ok(Some(v)) => v,
            Ok(None) => {
                record_failure(
                    &mut out,
                    row_index,
                    TesseraErrorOwned {
                        code: "missing_measure",
                        message: format!(
                            "row {row_index}: missing value in value column {value_column_name:?}"
                        ),
                        dimension: None,
                        column: Some(value_column_name.to_string()),
                    },
                    row_raw(batch, row),
                );
                continue 'rows;
            }
            Err(msg) => {
                record_failure(
                    &mut out,
                    row_index,
                    TesseraErrorOwned {
                        code: "type_coercion",
                        message: format!(
                            "row {row_index}: type coercion failed for value column {value_column_name:?}: {msg}"
                        ),
                        dimension: None,
                        column: Some(value_column_name.to_string()),
                    },
                    row_raw(batch, row),
                );
                continue 'rows;
            }
        };

        if !raw_f64.is_finite() {
            record_failure(
                &mut out,
                row_index,
                TesseraErrorOwned {
                    code: "type_coercion",
                    message: format!(
                        "row {row_index}: non-finite value in value column {value_column_name:?}"
                    ),
                    dimension: None,
                    column: Some(value_column_name.to_string()),
                },
                row_raw(batch, row),
            );
            continue 'rows;
        }

        let coord = CellCoordinate::from_parts(refs.cube_id, coord_elements);
        out.cells.push((coord, ScalarValue::F64(raw_f64)));
    }

    out
}

fn record_failure(
    out: &mut TransformedBatch,
    row_index: usize,
    err: TesseraErrorOwned,
    raw: Vec<(String, Option<String>)>,
) {
    out.failures.push(RowFailure {
        row_index,
        error: err,
        raw,
    });
}

/// Extract column[row] as an `f64`, returning `Ok(None)` for SQL NULL
/// and `Err(msg)` for type-incompatible values.
fn extract_f64(col: &Column, row: usize) -> Result<Option<f64>, String> {
    match &col.data {
        ColumnData::F64(v) => Ok(v.get(row).and_then(|x| *x)),
        ColumnData::I64(v) => Ok(v.get(row).and_then(|x| x.map(|n| n as f64))),
        ColumnData::Bool(v) => Ok(v
            .get(row)
            .and_then(|x| x.map(|b| if b { 1.0 } else { 0.0 }))),
        ColumnData::Str(v) => match v.get(row).and_then(|x| x.as_ref()) {
            None => Ok(None),
            Some(s) => match s.parse::<f64>() {
                Ok(n) => Ok(Some(n)),
                Err(e) => Err(format!("could not parse {s:?} as f64: {e}")),
            },
        },
        // `ColumnData` is `#[non_exhaustive]` (Stream C). Future variants
        // (e.g., decimals) require a Stream D update; until then we
        // surface a typed error instead of silently coercing.
        _ => Err("unsupported column data type for f64 extraction".to_string()),
    }
}

/// Render `column[row]` as a `String` (for dimension-element lookup) or
/// `None` for SQL NULL / empty cells.
fn column_value_as_string(col: &Column, row: usize) -> Option<String> {
    match &col.data {
        ColumnData::Str(v) => v.get(row).and_then(|x| x.clone()),
        ColumnData::F64(v) => v.get(row).and_then(|x| x.map(|n| n.to_string())),
        ColumnData::I64(v) => v.get(row).and_then(|x| x.map(|n| n.to_string())),
        ColumnData::Bool(v) => v.get(row).and_then(|x| x.map(|b| b.to_string())),
        // `ColumnData` is `#[non_exhaustive]`; treat unknown variants as
        // SQL NULL for stringification purposes.
        _ => None,
    }
}

/// Build a row-as-key/value list (for audit + quarantine emission).
fn row_raw(batch: &RowBatch, row: usize) -> Vec<(String, Option<String>)> {
    batch
        .columns
        .iter()
        .map(|c| (c.name.clone(), column_value_as_string(c, row)))
        .collect()
}
