//! Centralized model loading with explicit replay policy.
//!
//! Per `docs/process-notes.md` Rule 9 ("Four-source cube state model"),
//! cube state is the composition of up to four sources:
//!
//! ```text
//! Cube state = compile(YAML)
//!            + apply(canonical_inputs from YAML)
//!            + apply(Tessera imports from .tessera/audit.jsonl active manifest)
//!            + apply(post-hoc writes from .tessera/writes.jsonl)
//! ```
//!
//! Verbs split into two categories by which sources they replay:
//!
//! - **`LoadPolicy::CurrentReality`** — replay all four. Used by
//!   `query`, `whatif`, `trace`, `diff`, and `write` (so subsequent
//!   writes layer correctly on existing patches). This is the default
//!   policy: agents asking "what is true right now" should see
//!   operational reality including the writes-jsonl patch log.
//! - **`LoadPolicy::Reproducible`** — replay only the first three.
//!   Used by `mc model test` and `mc model sweep`. Tests verify the
//!   version-controlled model; post-hoc writes are not part of the
//!   reproducible model definition. Sweep runs in a pristine state to
//!   experiment with hypotheticals.
//!
//! `mc tessera apply` always operates on the canonical model state and
//! persists into the audit log; its in-process state is `Reproducible`.
//!
//! Item 1.1 of Phase 6A.2: this module replaces the prior `query::load_model`
//! which silently dropped post-hoc writes (the write-then-read coherence
//! bug). The previous public surface (`load_model`, `LoadModelError`,
//! `LoadedModel`) is preserved via re-exports in `query.rs`.

use mc_core::{Cube, PrincipalId, ScalarValue, WriteIntent, WritebackContext, WritebackRequest};
use mc_model::ModelRefs;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// Which sources of cube state should `load_model_with_policy` replay?
///
/// See module docs for the four-source state model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadPolicy {
    /// Replay YAML compile + canonical_inputs + Tessera active imports +
    /// `.tessera/writes.jsonl` post-hoc writes. The default for
    /// agent-facing read/write verbs (query, whatif, trace, diff, write).
    CurrentReality,
    /// Replay YAML compile + canonical_inputs + Tessera active imports
    /// only. Skips the post-hoc writes log. Used by `mc model test` and
    /// `mc model sweep` to operate on the version-controlled model
    /// without operational patches.
    Reproducible,
}

/// Reasons a [`load_model_with_policy`] call may fail.
///
/// Phase 6A.1 CRIT-3: I/O failures map to exit code 3, model failures to
/// exit code 1. Phase 6A.2 item 1.1 adds three writes-log replay variants;
/// per the handoff Decision Matrix all map to exit code 3 (they're operational
/// reality / file-system class problems, not model definition errors).
#[derive(Debug)]
pub enum LoadModelError {
    /// File system / I/O error reading the model file.
    Io(String),
    /// Parse, validate, resolve_inputs, or compile error.
    Model(String),
    /// A `.tessera/writes.jsonl` line failed to deserialize as a valid
    /// write-log entry. Per handoff matrix W3 (item 1.1): error at the
    /// first bad line; do not roll back lines 1..N.
    WriteLogCorrupt { line_number: usize, message: String },
    /// A `.tessera/writes.jsonl` entry references a coordinate whose
    /// dimension or element no longer exists in the YAML (the user
    /// edited the model after the write was logged). Per handoff matrix
    /// W1 (item 1.1): error rather than silently dropping — the user
    /// explicitly wrote that cell and silent loss is a correctness
    /// violation.
    WriteLogStaleCoord {
        line_number: usize,
        coord_string: String,
        missing_element: String,
    },
    /// A `.tessera/writes.jsonl` replay failed because the kernel
    /// rejected the write (e.g. now-derived measure). Per handoff
    /// matrix W2 (item 1.1): wrap the kernel error to preserve
    /// provenance for debugging.
    WriteLogReplayFailed { line_number: usize, inner: String },
}

impl LoadModelError {
    /// Map this error to the canonical CLI exit code (3 for I/O class,
    /// 1 for model class).
    pub fn exit_code(&self) -> i32 {
        match self {
            LoadModelError::Io(_) => 3,
            LoadModelError::Model(_) => 1,
            LoadModelError::WriteLogCorrupt { .. }
            | LoadModelError::WriteLogStaleCoord { .. }
            | LoadModelError::WriteLogReplayFailed { .. } => 3,
        }
    }

    /// Human-readable error message.
    pub fn message(&self) -> String {
        match self {
            LoadModelError::Io(m) | LoadModelError::Model(m) => m.clone(),
            LoadModelError::WriteLogCorrupt {
                line_number,
                message,
            } => format!(".tessera/writes.jsonl line {line_number}: {message}"),
            LoadModelError::WriteLogStaleCoord {
                line_number,
                coord_string,
                missing_element,
            } => format!(
                ".tessera/writes.jsonl line {line_number}: coordinate {coord_string:?} \
                 references unknown {missing_element}; the YAML may have been edited after \
                 the write was logged"
            ),
            LoadModelError::WriteLogReplayFailed { line_number, inner } => {
                format!(".tessera/writes.jsonl line {line_number}: replay failed: {inner}")
            }
        }
    }
}

impl std::fmt::Display for LoadModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

/// A populated cube + the metadata the CLI verbs need to read from it.
///
/// Carries the [`LoadPolicy`] used so verbs can introspect (e.g.,
/// regression-test that `model test` skipped the writes log).
pub struct LoadedModel {
    pub cube: Cube,
    pub root_principal: PrincipalId,
    pub refs: ModelRefs,
    /// The policy this model was loaded with.
    pub policy: LoadPolicy,
    /// Phase 6A.2 item 1.2: rule-name → rendered formula string,
    /// computed once at load time via `mc_model::inspect::summarize`.
    /// Trace consults this map to emit human/LLM-readable formula
    /// strings (e.g. `"Spend / CPC"`) instead of debug-formatted AST
    /// node names (e.g. `"Div"`).
    pub formulas: HashMap<String, String>,
    /// Phase 6A.3 item 5: highest `write_id` replayed from
    /// `.tessera/writes.jsonl`. Equal to the line count in that file at
    /// load time. `None` when the file does not exist OR when the
    /// `Reproducible` policy was used (write log was not replayed).
    /// Agents use this to chain queries to a specific revision via
    /// the `as_of_write_id` envelope field.
    pub as_of_write_id: Option<u64>,
    /// Phase 4D: measure-name → description string, extracted from the
    /// model's `measures[].description` field at load time. Only measures
    /// that have a `description:` key appear in this map. Used by
    /// `--verbose` mode in CLI verbs to enrich text output with prose.
    pub measure_descriptions: HashMap<String, String>,
}

/// Load a YAML model with the given replay policy. See [`LoadPolicy`]
/// and the module docs.
pub fn load_model_with_policy(
    path: &str,
    policy: LoadPolicy,
) -> Result<LoadedModel, LoadModelError> {
    // 1. YAML compile path (parse + validate + resolve_inputs + compile).
    let yaml = std::fs::read_to_string(path)
        .map_err(|e| LoadModelError::Io(format!("could not read model file {path:?}: {e}")))?;
    let parsed = mc_model::parse(&yaml, Some(path.to_string()))
        .map_err(|e| LoadModelError::Model(format!("parse error: {e}")))?;
    let mut validated = mc_model::validate(parsed).map_err(|errs| {
        LoadModelError::Model(
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    let model_dir = Path::new(path).parent();
    // Phase 3K (ADR-0030): auto-populate empty Standard/Time dimensions
    // from canonical_inputs columns before downstream resolve/compile.
    // MC2026 case-mismatch errors are surfaced via LoadModelError::Model.
    // Info/Warning diagnostics (MC1015/16/17) are silently discarded here
    // — agent-surface loaders don't print them; see `load_validated` in
    // mc-cli/main.rs for the user-facing surface.
    let _ = mc_model::auto_populate_dimensions(&mut validated, model_dir).map_err(|errs| {
        LoadModelError::Model(
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    let inputs = mc_model::resolve_inputs(&validated, model_dir).map_err(|errs| {
        LoadModelError::Model(
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    // Phase 6A.2 item 1.2: build the rule-name → formula map BEFORE
    // we move `validated` into `compile`. `summarize` runs in sub-ms
    // for Acme-scale models (5 rules); see decision matrix W4.
    let summary = mc_model::inspect::summarize(&validated, &[], None);
    let formulas: HashMap<String, String> = summary
        .rules
        .items
        .into_iter()
        .map(|r| (r.name, r.body_shape))
        .collect();

    // Phase 4D: extract measure descriptions before compile consumes
    // the validated model. Only measures with a description are included.
    let measure_descriptions: HashMap<String, String> = validated
        .parsed
        .measures
        .iter()
        .filter_map(|m| m.description.as_ref().map(|d| (m.name.clone(), d.clone())))
        .collect();

    let compiled = mc_model::compile(validated.clone())
        .map_err(|e| LoadModelError::Model(format!("compile error: {e}")))?;
    let mut cube = compiled.cube;
    let principal = compiled.root_principal;

    // 2. canonical_inputs (no-op when the model has none).
    if let Err(e) = mc_model::apply_canonical_inputs(&mut cube, &compiled.refs, principal, &inputs)
    {
        return Err(LoadModelError::Model(format!(
            "apply_canonical_inputs failed: {e}"
        )));
    }

    // 3 + 4: Tessera audit replay + post-hoc writes — only for CurrentReality.
    let mut as_of_write_id: Option<u64> = None;
    if policy == LoadPolicy::CurrentReality {
        let dir = model_dir.unwrap_or_else(|| Path::new("."));
        apply_tessera_active_imports(&mut cube, principal, dir)?;
        as_of_write_id = apply_writes_log(&mut cube, principal, &compiled.refs, dir)?;
    }

    Ok(LoadedModel {
        cube,
        root_principal: principal,
        refs: compiled.refs,
        policy,
        formulas,
        as_of_write_id,
        measure_descriptions,
    })
}

/// Backwards-compat shim defaulting to `CurrentReality` (the most-common
/// case for agent-facing verbs).
///
/// Item 1.1: prior callers of `query::load_model` keep working unchanged
/// and start seeing post-hoc writes automatically.
pub fn load_model(path: &str) -> Result<LoadedModel, LoadModelError> {
    load_model_with_policy(path, LoadPolicy::CurrentReality)
}

/// Replay every active Tessera import (per the manifest) into `cube`.
/// Mirrors `mc_tessera::Tessera::load_active`'s replay loop, but operates
/// on a cube that already has canonical_inputs applied.
///
/// Silent no-op if `.tessera/` does not exist or no imports are active —
/// this is the normal case for fresh models.
fn apply_tessera_active_imports(
    cube: &mut Cube,
    principal: PrincipalId,
    model_dir: &Path,
) -> Result<(), LoadModelError> {
    let tessera_dir = model_dir.join(".tessera");
    if !tessera_dir.exists() {
        return Ok(());
    }
    let sidecar = match mc_tessera::Sidecar::at_model_dir(model_dir) {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };
    let manifest = match mc_tessera::read_manifest(&sidecar) {
        Ok(m) => m,
        // Missing or unreadable manifest = no active imports yet. The
        // first-run `mc tessera apply` path creates the manifest before
        // appending; if it's truly corrupt it'll surface there.
        Err(_) => return Ok(()),
    };
    let cube_id = cube.id;
    for import_id in &manifest.imports {
        let cells_path = sidecar.import_cells_path(import_id);
        let cells = mc_tessera::read_cells_jsonl(cube_id, &cells_path).map_err(|e| {
            LoadModelError::Model(format!(
                "tessera audit replay failed for import {import_id:?}: {e}"
            ))
        })?;
        if cells.is_empty() {
            continue;
        }
        let ctx = WritebackContext {
            source_name: format!("replay:{import_id}"),
            import_id: import_id.clone(),
            principal,
        };
        let mut batch = mc_core::WriteBatch::new(cube, ctx);
        batch.push_batch(&cells).map_err(|e| {
            LoadModelError::Model(format!(
                "tessera audit replay push_batch failed for {import_id:?}: {e}"
            ))
        })?;
        batch.commit().map_err(|e| {
            LoadModelError::Model(format!(
                "tessera audit replay commit failed for {import_id:?}: {e}"
            ))
        })?;
    }
    Ok(())
}

/// Replay `.tessera/writes.jsonl` (post-hoc patch log) onto `cube`.
///
/// Each line is one append-only write event; events are applied in file
/// order via `Cube::write`. The log is line-independent (JSONL): we do
/// NOT roll back earlier lines on a mid-file failure (handoff matrix
/// W3 for item 1.1).
fn apply_writes_log(
    cube: &mut Cube,
    principal: PrincipalId,
    refs: &ModelRefs,
    model_dir: &Path,
) -> Result<Option<u64>, LoadModelError> {
    let log_path = model_dir.join(".tessera").join("writes.jsonl");
    if !log_path.exists() {
        return Ok(None);
    }
    let content = match std::fs::read_to_string(&log_path) {
        Ok(c) => c,
        Err(e) => {
            return Err(LoadModelError::Io(format!(
                "could not read .tessera/writes.jsonl: {e}"
            )));
        }
    };
    // Phase 6A.3 item 5 W2: write_id == 1-indexed line position. The
    // loader's `as_of_write_id` is the highest line position seen — i.e.
    // the count of `\n`-terminated lines in the file. An empty file
    // (zero lines) returns None to keep the envelope-null contract for
    // "no writes have happened yet" indistinguishable from a missing
    // file (handoff Decision Matrix W6).
    let line_count = content.lines().count() as u64;
    let max_write_id = if line_count == 0 {
        None
    } else {
        Some(line_count)
    };
    for (idx, line) in content.lines().enumerate() {
        let line_number = idx + 1;
        let trimmed = line.trim();
        // Skip blank lines silently — JSONL spec allows them and the
        // handoff edge-case tests cover empty-file / blank-line cases.
        if trimmed.is_empty() {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_str(trimmed).map_err(|e| LoadModelError::WriteLogCorrupt {
                line_number,
                message: format!("malformed JSON: {e}"),
            })?;
        let coord_str = value.get("coord").and_then(|v| v.as_str()).ok_or_else(|| {
            LoadModelError::WriteLogCorrupt {
                line_number,
                message: "missing or non-string \"coord\" field".into(),
            }
        })?;
        let value_num = value.get("value").and_then(|v| v.as_f64()).ok_or_else(|| {
            LoadModelError::WriteLogCorrupt {
                line_number,
                message: "missing or non-numeric \"value\" field".into(),
            }
        })?;
        let coord_names = parse_coord_string(coord_str);
        let coord = match refs.coord_from_names(&coord_names) {
            Some(c) => c,
            None => {
                let missing = describe_missing_element(&coord_names, refs);
                return Err(LoadModelError::WriteLogStaleCoord {
                    line_number,
                    coord_string: coord_str.to_string(),
                    missing_element: missing,
                });
            }
        };
        let request = WritebackRequest {
            coord,
            new_value: ScalarValue::F64(value_num),
            principal,
            intent: WriteIntent::Set,
            expected_revision: None,
            now_unix_seconds: 0,
        };
        if let Err(e) = cube.write(request) {
            return Err(LoadModelError::WriteLogReplayFailed {
                line_number,
                inner: e.to_string(),
            });
        }
    }
    Ok(max_write_id)
}

/// Parse `"Dim1=Elem1,Dim2=Elem2,..."` into a name-keyed map. Same shape
/// as `query::parse_coord_string` but inlined here so loader.rs has no
/// dependency on query.rs (loader is the lower module).
fn parse_coord_string(s: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((k, v)) = part.split_once('=') {
            out.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    out
}

/// Best-effort description of which `(dim, element)` pair in `names`
/// fails to resolve against `refs`. Used to populate `WriteLogStaleCoord.missing_element`.
fn describe_missing_element(names: &BTreeMap<String, String>, refs: &ModelRefs) -> String {
    // First check for an unknown dimension.
    for dim_name in names.keys() {
        if !refs.dimensions.contains_key(dim_name) {
            return format!("dimension {dim_name:?}");
        }
    }
    // Then check for an unknown element within a known dim.
    for (dim_name, elem_name) in names {
        if refs.element(dim_name, elem_name).is_none() {
            return format!("element {elem_name:?} in dimension {dim_name:?}");
        }
    }
    // Finally check for a missing slot — coord_names is missing a dim
    // the model expects.
    for dim in &refs.dimension_order {
        if !names.contains_key(dim) {
            return format!("missing slot for dimension {dim:?}");
        }
    }
    "unknown element".to_string()
}
