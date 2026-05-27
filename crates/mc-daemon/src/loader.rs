//! Cube loading for the daemon — four-source model.
//!
//! Replicates the essential loading logic from `mc-cli/src/loader.rs` for
//! daemon use. The daemon loads cubes via the same four-source pipeline:
//!
//! ```text
//! Cube state = compile(YAML)
//!            + apply(canonical_inputs)
//!            + apply(Tessera imports)
//!            + apply(post-hoc writes from .tessera/writes.jsonl)
//! ```

use mc_core::{Cube, PrincipalId, ScalarValue, WriteIntent, WritebackContext, WritebackRequest};
use mc_model::ModelRefs;
use std::collections::BTreeMap;
use std::path::Path;

/// A fully loaded cube ready for daemon use.
pub struct LoadedCube {
    pub cube: Cube,
    pub root_principal: PrincipalId,
    pub refs: ModelRefs,
}

/// Errors during cube loading.
#[derive(Debug)]
pub enum LoadError {
    Io(String),
    Model(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(m) => write!(f, "I/O error: {m}"),
            LoadError::Model(m) => write!(f, "model error: {m}"),
        }
    }
}

/// Load a cube from a YAML model path using the four-source pipeline.
///
/// This is the cold-load path: parse YAML → validate → compile → apply
/// canonical inputs → replay Tessera imports → replay post-hoc writes.
pub fn load_cube(model_path: &Path) -> Result<LoadedCube, LoadError> {
    let path_str = model_path.display().to_string();

    // 1. YAML compile path
    let yaml = std::fs::read_to_string(model_path)
        .map_err(|e| LoadError::Io(format!("could not read {path_str}: {e}")))?;
    let parsed = mc_model::parse(&yaml, Some(path_str.clone()))
        .map_err(|e| LoadError::Model(format!("parse error: {e}")))?;
    let mut validated = mc_model::validate(parsed).map_err(|errs| {
        LoadError::Model(
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    let model_dir = model_path.parent();
    // Phase 3K (ADR-0030): auto-populate empty Standard/Time dimensions
    // from canonical_inputs columns before downstream resolve/compile.
    // Without this, cartridges with `elements: []` fail at resolve_inputs.
    let _ = mc_model::auto_populate_dimensions(&mut validated, model_dir).map_err(|errs| {
        LoadError::Model(
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    let inputs = mc_model::resolve_inputs(&validated, model_dir).map_err(|errs| {
        LoadError::Model(
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;

    let compiled = mc_model::compile(validated)
        .map_err(|e| LoadError::Model(format!("compile error: {e}")))?;
    let mut cube = compiled.cube;
    let principal = compiled.root_principal;

    // 2. canonical_inputs
    if let Err(e) = mc_model::apply_canonical_inputs(&mut cube, &compiled.refs, principal, &inputs)
    {
        return Err(LoadError::Model(format!(
            "apply_canonical_inputs failed: {e}"
        )));
    }

    // 3. Tessera active imports
    if let Some(dir) = model_dir {
        apply_tessera_imports(&mut cube, principal, dir)?;
    }

    // 4. Post-hoc writes
    if let Some(dir) = model_dir {
        apply_writes_log(&mut cube, principal, &compiled.refs, dir)?;
    }

    Ok(LoadedCube {
        cube,
        root_principal: principal,
        refs: compiled.refs,
    })
}

/// Replay active Tessera imports into the cube.
fn apply_tessera_imports(
    cube: &mut Cube,
    principal: PrincipalId,
    model_dir: &Path,
) -> Result<(), LoadError> {
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
        Err(_) => return Ok(()),
    };
    let cube_id = cube.id;
    for import_id in &manifest.imports {
        let cells_path = sidecar.import_cells_path(import_id);
        let cells = mc_tessera::read_cells_jsonl(cube_id, &cells_path)
            .map_err(|e| LoadError::Model(format!("tessera replay failed for {import_id}: {e}")))?;
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
            LoadError::Model(format!("tessera push_batch failed for {import_id}: {e}"))
        })?;
        batch
            .commit()
            .map_err(|e| LoadError::Model(format!("tessera commit failed for {import_id}: {e}")))?;
    }
    Ok(())
}

/// Replay `.tessera/writes.jsonl` onto the cube.
fn apply_writes_log(
    cube: &mut Cube,
    principal: PrincipalId,
    refs: &ModelRefs,
    model_dir: &Path,
) -> Result<(), LoadError> {
    let log_path = model_dir.join(".tessera").join("writes.jsonl");
    if !log_path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(&log_path)
        .map_err(|e| LoadError::Io(format!("could not read .tessera/writes.jsonl: {e}")))?;

    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(trimmed).map_err(|e| {
            LoadError::Model(format!(
                "writes.jsonl line {}: malformed JSON: {e}",
                idx + 1
            ))
        })?;
        let coord_str = value.get("coord").and_then(|v| v.as_str()).ok_or_else(|| {
            LoadError::Model(format!(
                "writes.jsonl line {}: missing coord field",
                idx + 1
            ))
        })?;
        let value_num = value.get("value").and_then(|v| v.as_f64()).ok_or_else(|| {
            LoadError::Model(format!(
                "writes.jsonl line {}: missing value field",
                idx + 1
            ))
        })?;

        let coord_names = parse_coord_string(coord_str);
        let coord = match refs.coord_from_names(&coord_names) {
            Some(c) => c,
            None => {
                return Err(LoadError::Model(format!(
                    "writes.jsonl line {}: coordinate {coord_str:?} references unknown element",
                    idx + 1
                )));
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
            return Err(LoadError::Model(format!(
                "writes.jsonl line {}: replay failed: {e}",
                idx + 1
            )));
        }
    }
    Ok(())
}

/// Parse `"Dim1=Elem1,Dim2=Elem2,..."` into a name-keyed map.
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
