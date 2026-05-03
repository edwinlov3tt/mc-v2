//! Phase 3C resolve-inputs stage.
//!
//! Sits between Stage 2 (`validate`) and Stage 3 (`compile`). Produces
//! [`ResolvedInputs`] from a [`ValidatedModel`] + an optional model-file
//! directory. Reads CSV files; canonicalizes paths; rejects path-escape;
//! type-checks rows against measure declarations.
//!
//! Per the Phase 3C ADR-0006 acceptance amendments + the project owner's
//! architectural clarification on top of "Option A":
//!
//! - `validate()` stays filesystem-free.
//! - Resolve-inputs is a **named stage** between validate and compile.
//! - Diagnostics emit `ValidationError` variants with MC2xxx codes
//!   (MC2012–MC2025) so the JSON envelope shape is unchanged.
//! - `mc_model::load(path)` runs all four stages (parse → validate →
//!   resolve_inputs → compile) but does NOT apply inputs to the cube;
//!   the returned cube is empty of input data.
//! - `mc model test` is the only caller that *applies* inputs via
//!   [`apply_canonical_inputs`] / [`apply_fixture`] using existing
//!   `Cube::write`.
//!
//! The output is **name-keyed**, not ID-keyed — the resolved data is
//! independent of `compile`, so `mc model test` can compile the cube
//! after resolve_inputs and then apply rows by resolving names through
//! `ModelRefs::coord_from_names`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use mc_core::{
    CellCoordinate, Cube, EngineError, PrincipalId, ScalarValue, WriteIntent, WritebackRequest,
};

use crate::compile::ModelRefs;
use crate::csv::parse_strict;
use crate::error::ValidationError;
use crate::schema::{ParsedFixture, ParsedInputSet, ParsedRowCell, ValidatedModel};

/// Reserved column name for the cell value column. Per ADR-0006
/// amendment #19's chosen literal — the implementer was given leeway to
/// pick `__value` / `_value` if `value` conflicted, but Acme has no
/// measure named `value` so the natural literal stands.
pub const VALUE_COLUMN: &str = "value";

/// Output of [`resolve_inputs`]. Carries no kernel IDs; the apply
/// helpers resolve names → IDs via [`ModelRefs`] at apply time.
#[derive(Clone, Debug, Default)]
pub struct ResolvedInputs {
    /// `canonical_inputs:` block, resolved. `None` if the model didn't
    /// declare one.
    pub canonical: Option<ResolvedInputSet>,
    /// `test_fixtures:` blocks, resolved. Empty if no fixtures.
    pub fixtures: Vec<ResolvedFixture>,
}

impl ResolvedInputs {
    /// Look up a fixture by name.
    pub fn fixture(&self, name: &str) -> Option<&ResolvedFixture> {
        self.fixtures.iter().find(|f| f.name == name)
    }

    /// Total resolved row count across canonical_inputs + every fixture.
    /// Used by `inspect`.
    pub fn total_row_count(&self) -> usize {
        self.canonical.as_ref().map_or(0, |c| c.rows.len())
            + self.fixtures.iter().map(|f| f.rows.len()).sum::<usize>()
    }
}

/// One resolved input set (canonical_inputs or one fixture's data).
#[derive(Clone, Debug)]
pub struct ResolvedInputSet {
    /// Human-readable origin (the source path, or `"(inline)"`). Used
    /// by `mc model inspect` and the completion report.
    pub source_label: String,
    pub rows: Vec<ResolvedRow>,
}

/// One resolved fixture; same shape as [`ResolvedInputSet`] plus a name.
#[derive(Clone, Debug)]
pub struct ResolvedFixture {
    pub name: String,
    pub source_label: String,
    pub rows: Vec<ResolvedRow>,
}

/// One resolved input row. Names (not IDs) so the type is independent
/// of `compile`. The `coord` map is dim_name → element_name, including
/// the Measure dim. The `value` is typed against the row's measure
/// declaration.
#[derive(Clone, Debug)]
pub struct ResolvedRow {
    pub coord: BTreeMap<String, String>,
    pub value: ScalarValue,
}

/// Phase 3C resolve-inputs stage.
///
/// Reads CSV files (when `source:` is declared), canonicalizes their
/// paths against `model_dir`, and type-checks rows against the model's
/// measure declarations. Returns every accumulated `ValidationError` so
/// the user sees all problems in one pass.
pub fn resolve_inputs(
    validated: &ValidatedModel,
    model_dir: Option<&Path>,
) -> Result<ResolvedInputs, Vec<ValidationError>> {
    let mut diags: Vec<ValidationError> = Vec::new();
    let mut out = ResolvedInputs::default();

    // Fixture-level checks first (filesystem-free).
    check_fixture_uniqueness(validated, &mut diags);
    check_golden_fixture_refs(validated, &mut diags);

    // Resolve canonical_inputs.
    if let Some(decl) = &validated.parsed.canonical_inputs {
        match resolve_input_set(validated, decl, "canonical_inputs", model_dir) {
            Ok(rs) => out.canonical = Some(rs),
            Err(es) => diags.extend(es),
        }
    }

    // Resolve every test_fixtures entry.
    for f in &validated.parsed.test_fixtures {
        let label = format!("test_fixtures.{}", f.name);
        let decl = fixture_to_input_set(f);
        match resolve_input_set(validated, &decl, &label, model_dir) {
            Ok(rs) => out.fixtures.push(ResolvedFixture {
                name: f.name.clone(),
                source_label: rs.source_label,
                rows: rs.rows,
            }),
            Err(es) => diags.extend(es),
        }
    }

    if !diags.is_empty() {
        return Err(diags);
    }
    Ok(out)
}

/// Apply `canonical_inputs` rows to the cube via `Cube::write`. No-op
/// when the model didn't declare canonical_inputs. Returns the count of
/// cells written.
///
/// Per ADR-0006 acceptance amendment #15 + the project owner's
/// architectural clarification: this is the only place outside
/// `mc model test` that mutates the cube with input data. `load()` does
/// not call this; `mc demo --model` does not call this.
pub fn apply_canonical_inputs(
    cube: &mut Cube,
    refs: &ModelRefs,
    principal: PrincipalId,
    inputs: &ResolvedInputs,
) -> Result<usize, EngineError> {
    let Some(canonical) = &inputs.canonical else {
        return Ok(0);
    };
    apply_rows(cube, refs, principal, &canonical.rows)
}

/// Apply a single named fixture's rows to the cube. Used by
/// `mc model test` for goldens that declare `fixture: <name>`.
pub fn apply_fixture(
    cube: &mut Cube,
    refs: &ModelRefs,
    principal: PrincipalId,
    fixture: &ResolvedFixture,
) -> Result<usize, EngineError> {
    apply_rows(cube, refs, principal, &fixture.rows)
}

// ---------------------------------------------------------------------------
// Internal: per-input-set resolution
// ---------------------------------------------------------------------------

fn fixture_to_input_set(f: &ParsedFixture) -> ParsedInputSet {
    ParsedInputSet {
        columns: f.columns.clone(),
        source: f.source.clone(),
        inline: f.inline.clone(),
    }
}

fn resolve_input_set(
    validated: &ValidatedModel,
    decl: &ParsedInputSet,
    label: &str,
    model_dir: Option<&Path>,
) -> Result<ResolvedInputSet, Vec<ValidationError>> {
    let mut diags: Vec<ValidationError> = Vec::new();

    // Source XOR inline.
    let (source_label, raw_rows): (String, Vec<Vec<String>>) = match (&decl.source, &decl.inline) {
        (Some(_), Some(_)) => {
            return Err(vec![ValidationError::Schema {
                message: format!(
                    "input set {label:?}: both `source:` and `inline:` declared (must declare exactly one)"
                ),
            }]);
        }
        (None, None) => {
            return Err(vec![ValidationError::Schema {
                message: format!(
                    "input set {label:?}: neither `source:` nor `inline:` declared (must declare exactly one)"
                ),
            }]);
        }
        (Some(src), None) => match resolve_csv_path(model_dir, src, label) {
            Ok(path) => match std::fs::read_to_string(path) {
                Ok(content) => match parse_strict(&content, &decl.columns, label) {
                    Ok(rows) => (src.clone(), rows),
                    Err(es) => return Err(es),
                },
                Err(e) => {
                    return Err(vec![ValidationError::FixtureSourceUnreadable {
                        input_set: label.to_string(),
                        path: src.clone(),
                        reason: format!("read error: {e}"),
                    }]);
                }
            },
            Err(e) => return Err(vec![e]),
        },
        (None, Some(inline)) => {
            // Convert inline rows to string-based rows so the rest of
            // the resolver is uniform between CSV and inline paths.
            // Type-checking happens against the measure declaration in
            // the per-row loop below; ParsedScalar's discriminant is
            // discarded here intentionally.
            let mut bad = false;
            let mut rows: Vec<Vec<String>> = Vec::new();
            for (i, row) in inline.rows.iter().enumerate() {
                if row.len() != decl.columns.len() {
                    diags.push(ValidationError::FixtureCsvRowColumnCountMismatch {
                        input_set: label.to_string(),
                        // Inline rows have no "line number"; use the
                        // 1-based row index as the proxy.
                        line: i + 1,
                        expected: decl.columns.len(),
                        actual: row.len(),
                    });
                    bad = true;
                    continue;
                }
                rows.push(row.iter().map(row_cell_to_string).collect());
            }
            if bad {
                return Err(diags);
            }
            ("(inline)".to_string(), rows)
        }
    };

    // Validate columns.
    if decl.columns.is_empty() {
        return Err(vec![ValidationError::Schema {
            message: format!("input set {label:?}: columns is empty"),
        }]);
    }
    let last = decl
        .columns
        .last()
        .ok_or_else(|| vec![internal_schema(label, "columns vec drained unexpectedly")])?;
    if last != VALUE_COLUMN {
        return Err(vec![ValidationError::Schema {
            message: format!(
                "input set {label:?}: last column must be {VALUE_COLUMN:?} (got {last:?})"
            ),
        }]);
    }
    let dim_columns: &[String] = &decl.columns[..decl.columns.len() - 1];

    // MC2012: each dim column must name a declared dimension.
    let mut bad_columns = false;
    for col in dim_columns {
        if !validated.dim_index_by_name.contains_key(col) {
            diags.push(ValidationError::FixtureUnknownDimensionKey {
                input_set: label.to_string(),
                column: col.clone(),
            });
            bad_columns = true;
        }
    }

    // MC2019: every model dim must be present in dim_columns.
    let dim_set: BTreeSet<&str> = dim_columns.iter().map(String::as_str).collect();
    let mut missing: Vec<String> = Vec::new();
    for d in &validated.parsed.dimensions {
        if !dim_set.contains(d.name.as_str()) {
            missing.push(d.name.clone());
        }
    }
    if !missing.is_empty() {
        diags.push(ValidationError::FixtureMissingDimension {
            input_set: label.to_string(),
            columns: dim_columns.to_vec(),
            missing,
        });
    }

    // If the column declaration is broken, per-row resolution would
    // produce noise. Stop here.
    if bad_columns || !diags.is_empty() {
        return Err(diags);
    }

    let measure_dim_name = validated
        .parsed
        .dimensions
        .get(validated.measure_dim_index)
        .map(|d| d.name.as_str())
        .ok_or_else(|| {
            vec![internal_schema(
                label,
                "Measure dim missing from validated model",
            )]
        })?;
    let measure_col_idx = dim_columns
        .iter()
        .position(|c| c == measure_dim_name)
        .ok_or_else(|| {
            vec![internal_schema(
                label,
                "Measure dim column not in dim_columns after MC2019 cleared",
            )]
        })?;
    let value_col_idx = dim_columns.len();

    let consolidated_by_dim = compute_consolidated_per_dim(validated);

    // Per-row resolution.
    let mut resolved_rows: Vec<ResolvedRow> = Vec::new();
    let mut seen_coords: BTreeMap<Vec<(String, String)>, usize> = BTreeMap::new();
    for (row_idx, row) in raw_rows.iter().enumerate() {
        if row.len() != decl.columns.len() {
            // CSV path already filtered these via MC2023; inline rows
            // already filtered above. Defensive guard only.
            continue;
        }
        let mut coord: BTreeMap<String, String> = BTreeMap::new();
        let mut row_ok = true;
        for (col_i, col_name) in dim_columns.iter().enumerate() {
            let val = &row[col_i];
            let dim_i = match validated.dim_index_by_name.get(col_name) {
                Some(&i) => i,
                None => continue, // already MC2012'd above
            };
            let dim = &validated.parsed.dimensions[dim_i];
            let element_known = if dim.kind == "Measure" {
                validated.measure_index_by_name.contains_key(val.as_str())
            } else {
                validated.element_index_by_name[dim_i].contains_key(val.as_str())
            };
            if !element_known {
                if dim.kind == "Measure" {
                    diags.push(ValidationError::FixtureUnknownMeasure {
                        input_set: label.to_string(),
                        row_index: row_idx,
                        measure: val.clone(),
                    });
                } else {
                    diags.push(ValidationError::FixtureUnknownElementValue {
                        input_set: label.to_string(),
                        row_index: row_idx,
                        dim: col_name.clone(),
                        value: val.clone(),
                    });
                }
                row_ok = false;
                continue;
            }
            // MC2020: consolidated-element rejection.
            if let Some(consolidated) = consolidated_by_dim.get(col_name.as_str()) {
                if consolidated.contains(val.as_str()) {
                    diags.push(ValidationError::FixtureWritesConsolidatedCell {
                        input_set: label.to_string(),
                        row_index: row_idx,
                        dim: col_name.clone(),
                        element: val.clone(),
                    });
                    row_ok = false;
                }
            }
            coord.insert(col_name.clone(), val.clone());
        }
        if !row_ok {
            continue;
        }

        // Resolve the row's measure for value typing.
        let measure_name = &row[measure_col_idx];
        let measure = match validated.measure_index_by_name.get(measure_name) {
            Some(&idx) => &validated.parsed.measures[idx],
            None => continue, // already MC2014'd above
        };

        // MC2015: derived measures are not writable.
        if measure.role == "Derived" {
            diags.push(ValidationError::FixtureWritesDerivedMeasure {
                input_set: label.to_string(),
                row_index: row_idx,
                measure: measure_name.clone(),
            });
            continue;
        }

        // Parse value per measure data_type (MC2018 / MC2021).
        let value_str = &row[value_col_idx];
        let value = match parse_value(value_str, measure.data_type.as_str()) {
            Ok(v) => v,
            Err(reason) => {
                if let ParseValueError::Nan = reason {
                    diags.push(ValidationError::FixtureValueIsNaN {
                        input_set: label.to_string(),
                        row_index: row_idx,
                    });
                } else {
                    diags.push(ValidationError::FixtureValueTypeMismatch {
                        input_set: label.to_string(),
                        row_index: row_idx,
                        measure: measure_name.clone(),
                        data_type: measure.data_type.clone(),
                        value: value_str.clone(),
                    });
                }
                continue;
            }
        };

        // MC2025: duplicate coordinate within the same input set.
        let coord_key: Vec<(String, String)> =
            coord.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        if let Some(&first_idx) = seen_coords.get(&coord_key) {
            diags.push(ValidationError::FixtureDuplicateCoordinate {
                input_set: label.to_string(),
                first_row: first_idx,
                second_row: row_idx,
            });
            continue;
        }
        seen_coords.insert(coord_key, row_idx);

        resolved_rows.push(ResolvedRow { coord, value });
    }

    if !diags.is_empty() {
        return Err(diags);
    }
    Ok(ResolvedInputSet {
        source_label,
        rows: resolved_rows,
    })
}

// ---------------------------------------------------------------------------
// Internal: path resolution + helpers
// ---------------------------------------------------------------------------

fn resolve_csv_path(
    model_dir: Option<&Path>,
    source: &str,
    label: &str,
) -> Result<PathBuf, ValidationError> {
    let model_dir = match model_dir {
        Some(d) => d,
        None => {
            return Err(ValidationError::FixtureSourceUnreadable {
                input_set: label.to_string(),
                path: source.to_string(),
                reason: "no file context: source paths are only valid when loading from a file (not from an in-memory string)".into(),
            });
        }
    };
    let raw = Path::new(source);
    if raw.is_absolute() {
        return Err(ValidationError::FixtureSourceUnreadable {
            input_set: label.to_string(),
            path: source.to_string(),
            reason:
                "path-escape: absolute paths are rejected; sources must be relative to the YAML model file's directory"
                    .into(),
        });
    }
    if raw
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(ValidationError::FixtureSourceUnreadable {
            input_set: label.to_string(),
            path: source.to_string(),
            reason:
                "path-escape: `..` segments are rejected; sources must stay within the YAML model file's directory tree"
                    .into(),
        });
    }
    let candidate = model_dir.join(source);
    let canonical_candidate = match candidate.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return Err(ValidationError::FixtureSourceUnreadable {
                input_set: label.to_string(),
                path: source.to_string(),
                reason: format!("not found: {e}"),
            });
        }
    };
    let canonical_dir = match model_dir.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return Err(ValidationError::FixtureSourceUnreadable {
                input_set: label.to_string(),
                path: source.to_string(),
                reason: format!("model directory unreadable: {e}"),
            });
        }
    };
    if !canonical_candidate.starts_with(canonical_dir) {
        return Err(ValidationError::FixtureSourceUnreadable {
            input_set: label.to_string(),
            path: source.to_string(),
            reason: "path-escape: resolved path is outside the YAML model file's directory tree"
                .into(),
        });
    }
    Ok(canonical_candidate)
}

/// Compute, per dimension, the set of element names that appear as a
/// hierarchy parent in any hierarchy on that dim (i.e., consolidated
/// elements). Used by MC2020.
fn compute_consolidated_per_dim(validated: &ValidatedModel) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for h in &validated.parsed.hierarchies {
        let entry = out.entry(h.dimension.clone()).or_default();
        for edge in &h.edges {
            entry.insert(edge.parent.clone());
        }
    }
    out
}

fn check_fixture_uniqueness(validated: &ValidatedModel, diags: &mut Vec<ValidationError>) {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for f in &validated.parsed.test_fixtures {
        *seen.entry(f.name.clone()).or_insert(0) += 1;
    }
    for (name, count) in seen {
        if count > 1 {
            diags.push(ValidationError::DuplicateFixtureName { name });
        }
    }
}

fn check_golden_fixture_refs(validated: &ValidatedModel, diags: &mut Vec<ValidationError>) {
    let known: BTreeSet<&str> = validated
        .parsed
        .test_fixtures
        .iter()
        .map(|f| f.name.as_str())
        .collect();
    for g in &validated.parsed.golden_tests {
        if let Some(fname) = &g.fixture {
            if !known.contains(fname.as_str()) {
                diags.push(ValidationError::GoldenReferencesUnknownFixture {
                    golden_name: g.name.clone(),
                    fixture_name: fname.clone(),
                });
            }
        }
    }
}

enum ParseValueError {
    Nan,
    TypeMismatch,
}

fn parse_value(s: &str, data_type: &str) -> Result<ScalarValue, ParseValueError> {
    match data_type {
        "F64" => match s.parse::<f64>() {
            Ok(f) if f.is_nan() => Err(ParseValueError::Nan),
            Ok(f) => Ok(ScalarValue::F64(f)),
            Err(_) => Err(ParseValueError::TypeMismatch),
        },
        "I64" => match s.parse::<i64>() {
            Ok(i) => Ok(ScalarValue::I64(i)),
            Err(_) => Err(ParseValueError::TypeMismatch),
        },
        "Bool" => match s.parse::<bool>() {
            Ok(b) => Ok(ScalarValue::Bool(b)),
            Err(_) => Err(ParseValueError::TypeMismatch),
        },
        // Category not implemented in Phase 3C input loading — none of
        // Acme's measures use it, and the schema for Category-typed
        // input rows would also need category-domain validation that's
        // out of scope.
        _ => Err(ParseValueError::TypeMismatch),
    }
}

fn row_cell_to_string(s: &ParsedRowCell) -> String {
    match s {
        ParsedRowCell::Float(f) => format!("{f}"),
        ParsedRowCell::Int(i) => format!("{i}"),
        ParsedRowCell::Bool(b) => format!("{b}"),
        ParsedRowCell::Str(s) => s.clone(),
    }
}

fn internal_schema(label: &str, msg: &str) -> ValidationError {
    ValidationError::Schema {
        message: format!("input set {label:?}: internal: {msg}"),
    }
}

fn apply_rows(
    cube: &mut Cube,
    refs: &ModelRefs,
    principal: PrincipalId,
    rows: &[ResolvedRow],
) -> Result<usize, EngineError> {
    let mut count = 0;
    for row in rows {
        let coord: CellCoordinate =
            refs.coord_from_names(&row.coord)
                .ok_or(EngineError::Internal(
                    "apply_rows: resolve_inputs left an unresolvable coord (validator gap)",
                ))?;
        cube.write(WritebackRequest {
            coord,
            new_value: row.value.clone(),
            principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        })?;
        count += 1;
    }
    Ok(count)
}
