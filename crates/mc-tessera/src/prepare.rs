//! `PreparedImport` — the materialized recipe + cube + driver + column plan.
//!
//! [`Tessera::prepare`](crate::Tessera::prepare) is the entry point. It runs
//! the full pre-execution pipeline:
//!
//! 1. Read the recipe YAML from disk; parse via [`mc_recipe::parse`].
//! 2. Resolve the recipe's `model:` field relative to the recipe directory.
//! 3. Parse + validate the model via [`mc_model::parse`] + [`mc_model::validate`].
//! 4. Validate the recipe against the model via [`mc_recipe::validate_recipe`].
//!    Any errors become [`TesseraError::Recipe`].
//! 5. Resolve credentials via the [`SecretResolver`] (Phase 5A:
//!    [`EnvVarSecretResolver`]).
//! 6. Compile the model into a fresh empty [`Cube`] via [`mc_model::load`].
//! 7. Construct a [`Box<dyn SourceDriver>`] from the recipe's
//!    `source.driver` enum, with credentials interpolated.
//! 8. Resolve column mappings to a [`Vec<ResolvedColumnMapping>`] that binds
//!    each non-skipped source column to a typed target (dimension index +
//!    DimensionId, or measure ElementId).
//! 9. Resolve `defaults` to a [`Vec<ResolvedDefault>`] (DimensionId +
//!    ElementId pairs).
//! 10. Wrap everything in a [`PreparedImport`] and hand it to the caller.
//!
//! `prepare()` does NOT mutate the cube — that is `apply()`'s job.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use mc_core::{Cube, DimensionId, ElementId, PrincipalId};
use mc_drivers::SourceDriver;
use mc_model::ModelRefs;
use mc_recipe::{
    parse as parse_recipe, validate_recipe, DriverKind, PathContext, Recipe, SourceConfig,
};

use crate::error::TesseraError;
use crate::secrets::{interpolate, EnvVarSecretResolver, SecretResolver};

/// Materialized, validated, ready-to-execute import.
///
/// Holds the parsed recipe, the loaded cube, the constructed driver, the
/// resolved column plan, and the resolved defaults. The cube is owned —
/// [`crate::Tessera::apply`] borrows it mutably to drive the
/// [`mc_core::WriteBatch`].
pub struct PreparedImport {
    /// The parsed recipe.
    pub recipe: Recipe,
    /// Path to the recipe YAML file (used for relative resolution).
    pub recipe_path: PathBuf,
    /// Resolved absolute path of the model YAML.
    pub model_path: PathBuf,
    /// The loaded cube. Empty of input data — call
    /// [`crate::Tessera::apply`] to populate.
    pub cube: Cube,
    /// Root principal of the cube (used as `WritebackContext.principal`).
    pub principal: PrincipalId,
    /// Name → ID resolver for the loaded cube.
    pub refs: ModelRefs,
    /// Boxed driver, ready for `fetch_batch` calls.
    pub driver: Box<dyn SourceDriver>,
    /// One entry per non-skipped recipe column, in source order. Skipped
    /// columns are absent. Each entry binds the source-column name to
    /// either a dimension target or a measure target.
    pub column_plan: Vec<ResolvedColumnMapping>,
    /// One entry per `recipe.defaults` key. Holds the resolved
    /// `(DimensionId, ElementId)` pair plus the cube-position index for
    /// the dimension (so the transformer can drop the element into the
    /// right slot of [`mc_core::CellCoordinate`]).
    pub defaults: Vec<ResolvedDefault>,
    /// Driver schema (column-name list with inferred types). Cached at
    /// `prepare()` time so the transformer doesn't re-query the driver.
    pub driver_schema_names: Vec<String>,
    /// Credentials with `${env.X}` references already interpolated.
    /// Map key = original recipe `credentials:` key; value = resolved.
    pub resolved_credentials: HashMap<String, String>,
}

impl std::fmt::Debug for PreparedImport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedImport")
            .field("recipe_name", &self.recipe.name)
            .field("recipe_path", &self.recipe_path)
            .field("model_path", &self.model_path)
            .field("cube_id", &self.cube.id)
            .field("column_plan_len", &self.column_plan.len())
            .field("defaults_len", &self.defaults.len())
            .field("driver_columns", &self.driver_schema_names)
            .finish()
    }
}

/// One non-skipped source column → typed target binding.
///
/// For dimension columns: every row's value is looked up in
/// `model_refs.elements[(dim, value)]` to get an `ElementId`.
/// For measure columns: the target `ElementId` (an element of the
/// `Measure` dimension) is fixed at prepare-time; every row's `value` is
/// coerced to `ScalarValue::F64` and written to that measure.
#[derive(Clone, Debug)]
pub struct ResolvedColumnMapping {
    /// Source column name (as it appears in the recipe + driver schema).
    pub source: String,
    /// Index of this column in the driver's schema (`schema_names`).
    pub source_index: usize,
    /// What this column targets in the cube.
    pub target: MappingTarget,
    /// Optional numeric scale factor (applied at row-transform time for
    /// measure columns; ignored for dimension columns).
    pub scale: Option<f64>,
}

/// What a non-skipped column targets.
#[derive(Clone, Debug)]
pub enum MappingTarget {
    /// Dimension column. Each row's value resolves to an `ElementId` via
    /// `ModelRefs::element(dim_name, value)`.
    Dimension {
        /// Dimension name.
        dim_name: String,
        /// `DimensionId` of the dimension in the cube.
        dim_id: DimensionId,
        /// 0-based index of this dimension in
        /// `ModelRefs::dimension_order` (used to drop the resolved
        /// `ElementId` into the correct coordinate slot).
        dim_position: usize,
    },
    /// Measure column. The `Measure` dimension element is fixed at
    /// prepare-time; the column carries the F64 value.
    Measure {
        /// Measure name (also the element name in the `Measure` dim).
        measure_name: String,
        /// `ElementId` of the measure (an element of the `Measure` dim).
        measure_element_id: ElementId,
    },
}

/// One `recipe.defaults` entry resolved to concrete IDs + slot index.
#[derive(Clone, Debug)]
pub struct ResolvedDefault {
    /// Dimension name (e.g., `"Scenario"`).
    pub dim_name: String,
    /// `DimensionId` in the cube.
    pub dim_id: DimensionId,
    /// 0-based index of this dimension in `ModelRefs::dimension_order`.
    pub dim_position: usize,
    /// `ElementId` of the resolved element.
    pub element_id: ElementId,
    /// Element name as it appeared in `recipe.defaults`.
    pub element_name: String,
}

/// Run the full prepare pipeline against a recipe path on disk.
///
/// `recipe_path` is treated as the canonical recipe file location. The
/// `model:` field inside the recipe is resolved relative to its parent
/// directory.
pub fn prepare_from_path(recipe_path: &Path) -> Result<PreparedImport, TesseraError> {
    let recipe_yaml =
        std::fs::read_to_string(recipe_path).map_err(|e| TesseraError::io(recipe_path, e))?;

    let recipe_dir = recipe_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    prepare_from_yaml(&recipe_yaml, recipe_path, &recipe_dir)
}

/// Like [`prepare_from_path`] but takes the recipe YAML as a string.
/// `recipe_path` and `recipe_dir` still need to be supplied for path
/// resolution + diagnostic context.
pub fn prepare_from_yaml(
    recipe_yaml: &str,
    recipe_path: &Path,
    recipe_dir: &Path,
) -> Result<PreparedImport, TesseraError> {
    // Step 1: parse recipe.
    let recipe = parse_recipe(recipe_yaml).map_err(|e| TesseraError::Recipe { errors: vec![e] })?;

    // Step 2: resolve model path relative to recipe directory.
    let model_path = if Path::new(&recipe.model).is_absolute() {
        PathBuf::from(&recipe.model)
    } else {
        recipe_dir.join(&recipe.model)
    };

    // Step 3: load the model YAML so we can validate the recipe against
    // a ValidatedModel. Read the YAML and validate first (Phase 3B
    // validate-on-validation is just parse + validate; it does NOT compile
    // a Cube) — we only need to compile a Cube on apply().
    let model_yaml =
        std::fs::read_to_string(&model_path).map_err(|e| TesseraError::io(&model_path, e))?;
    let parsed_model = mc_model::parse(&model_yaml, Some(model_path.display().to_string()))
        .map_err(|e| TesseraError::Model {
            errors: vec![mc_model::Error::Parse(e)],
        })?;
    let validated_model =
        mc_model::validate(parsed_model).map_err(|errs| TesseraError::Model { errors: errs })?;

    // Step 4: validate the recipe against the model.
    let path_ctx = PathContext {
        recipe_dir,
        // Phase 5A: workspace_root falls back to the recipe_dir's parent
        // (best-effort) so the path-escape check is permissive but
        // present. Real callers (the CLI) supply the workspace explicitly
        // when the binding is meaningful.
        workspace_root: recipe_dir,
    };
    let errors = validate_recipe(&recipe, &validated_model, Some(path_ctx));
    if !errors.is_empty() {
        return Err(TesseraError::Recipe { errors });
    }

    // Step 5: resolve credentials.
    let resolver = EnvVarSecretResolver;
    let resolved_credentials = resolve_credentials(&recipe.credentials, &resolver)?;

    // Step 6: compile the model into a Cube.
    let compiled =
        mc_model::load(&model_path).map_err(|errs| TesseraError::Model { errors: errs })?;

    // Step 7: construct the source driver from the recipe.source block.
    let driver = construct_driver(&recipe.source, recipe_dir, &resolved_credentials)?;
    let driver_schema = driver.schema().map_err(TesseraError::Driver)?;
    let driver_schema_names: Vec<String> = driver_schema.iter().map(|c| c.name.clone()).collect();

    // Step 8: resolve column mappings.
    let column_plan = resolve_column_plan(&recipe, &compiled.refs, &driver_schema_names)?;

    // Step 9: resolve defaults.
    let defaults = resolve_defaults(&recipe, &compiled.refs)?;

    Ok(PreparedImport {
        recipe,
        recipe_path: recipe_path.to_path_buf(),
        model_path,
        cube: compiled.cube,
        principal: compiled.root_principal,
        refs: compiled.refs,
        driver,
        column_plan,
        defaults,
        driver_schema_names,
        resolved_credentials,
    })
}

/// Walk `credentials:` and apply `${env.X}` interpolation via `resolver`.
fn resolve_credentials<R: SecretResolver>(
    credentials: &HashMap<String, String>,
    resolver: &R,
) -> Result<HashMap<String, String>, TesseraError> {
    let mut out = HashMap::with_capacity(credentials.len());
    for (k, v) in credentials {
        match interpolate(v, resolver) {
            Ok(resolved) => {
                out.insert(k.clone(), resolved);
            }
            Err(e) => {
                return Err(TesseraError::from_secret(format!("/credentials/{k}"), e));
            }
        }
    }
    Ok(out)
}

/// Construct a `Box<dyn SourceDriver>` from the recipe's source block.
///
/// Per ADR-0010 Decision 8 + the Stream C handoff: each driver kind has
/// its own constructor and field expectations. Missing required fields
/// surface as MC5xxx-relevant DriverError variants.
fn construct_driver(
    source: &SourceConfig,
    recipe_dir: &Path,
    credentials: &HashMap<String, String>,
) -> Result<Box<dyn SourceDriver>, TesseraError> {
    use mc_drivers::{
        csv_driver, duckdb_driver, duckdb_postgres_driver, http_json_driver, postgres_driver,
        sqlite_driver, DriverError,
    };

    // Resolve a path string relative to recipe_dir if it isn't absolute.
    let resolve_path = |p: &str| -> PathBuf {
        let raw = Path::new(p);
        if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            recipe_dir.join(raw)
        }
    };

    match source.driver {
        DriverKind::Csv => {
            let path_str = source.path.as_deref().ok_or_else(|| {
                TesseraError::Driver(DriverError::SourceFileNotFound {
                    path: PathBuf::new(),
                    message: "csv driver requires source.path".to_string(),
                })
            })?;
            let path = resolve_path(path_str);
            let driver = csv_driver(&path).map_err(TesseraError::Driver)?;
            Ok(Box::new(driver))
        }
        DriverKind::Sqlite => {
            let path_str = source.path.as_deref().ok_or_else(|| {
                TesseraError::Driver(DriverError::SourceFileNotFound {
                    path: PathBuf::new(),
                    message: "sqlite driver requires source.path".to_string(),
                })
            })?;
            let query = effective_query(source)?;
            let path = resolve_path(path_str);
            let driver = sqlite_driver(&path, &query).map_err(TesseraError::Driver)?;
            Ok(Box::new(driver))
        }
        DriverKind::Duckdb => {
            let path_str = source.path.as_deref().ok_or_else(|| {
                TesseraError::Driver(DriverError::SourceFileNotFound {
                    path: PathBuf::new(),
                    message: "duckdb driver requires source.path".to_string(),
                })
            })?;
            let query = effective_query(source)?;
            let path = resolve_path(path_str);
            let driver = duckdb_driver(&path, &query).map_err(TesseraError::Driver)?;
            Ok(Box::new(driver))
        }
        DriverKind::Postgres => {
            // Phase 5A convention: the DSN is supplied via credentials
            // under the key "PG_DSN" or via the first credential value.
            // Falls back to a single credential entry if there's exactly
            // one.
            let dsn = credentials
                .get("PG_DSN")
                .or_else(|| credentials.get("dsn"))
                .or_else(|| {
                    if credentials.len() == 1 {
                        credentials.values().next()
                    } else {
                        None
                    }
                })
                .ok_or_else(|| {
                    TesseraError::Driver(DriverError::ConnectionFailed {
                        target: "<unknown postgres dsn>".to_string(),
                        message:
                            "postgres driver requires PG_DSN credential (or a single credential)"
                                .to_string(),
                    })
                })?;
            let query = effective_query(source)?;
            let driver = postgres_driver(dsn, &query).map_err(TesseraError::Driver)?;
            Ok(Box::new(driver))
        }
        DriverKind::DuckdbPostgres => {
            let path_str = source.path.as_deref().ok_or_else(|| {
                TesseraError::Driver(DriverError::SourceFileNotFound {
                    path: PathBuf::new(),
                    message: "duckdb_postgres driver requires source.path (DuckDB file)"
                        .to_string(),
                })
            })?;
            let dsn = credentials
                .get("PG_DSN")
                .or_else(|| credentials.get("dsn"))
                .ok_or_else(|| {
                    TesseraError::Driver(DriverError::ConnectionFailed {
                        target: "<unknown postgres dsn>".to_string(),
                        message: "duckdb_postgres driver requires PG_DSN credential".to_string(),
                    })
                })?;
            let query = effective_query(source)?;
            let path = resolve_path(path_str);
            let driver =
                duckdb_postgres_driver(&path, dsn, &query).map_err(TesseraError::Driver)?;
            Ok(Box::new(driver))
        }
        DriverKind::HttpJson => {
            let url = source.url.as_deref().ok_or_else(|| {
                TesseraError::Driver(DriverError::ConnectionFailed {
                    target: "<no url>".to_string(),
                    message: "http_json driver requires source.url".to_string(),
                })
            })?;
            let driver =
                http_json_driver(url, source.json_path.as_deref()).map_err(TesseraError::Driver)?;
            Ok(Box::new(driver))
        }
    }
}

/// Compute the effective SQL query for a query-based driver. If `query`
/// is set, use it as-is. If only `table` is set, expand to
/// `SELECT * FROM <table>`. Recipe validation already rejects
/// both-set (MC5003).
fn effective_query(source: &SourceConfig) -> Result<String, TesseraError> {
    if let Some(q) = &source.query {
        return Ok(q.clone());
    }
    if let Some(t) = &source.table {
        return Ok(format!("SELECT * FROM {t}"));
    }
    Err(TesseraError::Driver(mc_drivers::DriverError::QueryFailed {
        query: String::new(),
        message: "source.query or source.table must be specified".to_string(),
    }))
}

/// Resolve every non-skipped column in the recipe to a typed target.
fn resolve_column_plan(
    recipe: &Recipe,
    refs: &ModelRefs,
    driver_schema_names: &[String],
) -> Result<Vec<ResolvedColumnMapping>, TesseraError> {
    // Build a name → position lookup over the driver schema for fast
    // source-index resolution.
    let schema_index: HashMap<&str, usize> = driver_schema_names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect();

    let dim_position: HashMap<&str, usize> = refs
        .dimension_order
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect();

    let mut plan = Vec::with_capacity(recipe.columns.len());
    for col in &recipe.columns {
        if matches!(col.skip, Some(true)) {
            continue;
        }
        let source_index = match schema_index.get(col.source.as_str()) {
            Some(i) => *i,
            None => {
                return Err(TesseraError::SourceColumnMissing {
                    column: col.source.clone(),
                });
            }
        };

        let target = if let Some(dim_name) = &col.dimension {
            let dim_id = refs.dimensions.get(dim_name).copied().ok_or_else(|| {
                TesseraError::DimensionNotInCube {
                    dimension: dim_name.clone(),
                }
            })?;
            let dim_pos = *dim_position.get(dim_name.as_str()).ok_or_else(|| {
                TesseraError::DimensionNotInCube {
                    dimension: dim_name.clone(),
                }
            })?;
            MappingTarget::Dimension {
                dim_name: dim_name.clone(),
                dim_id,
                dim_position: dim_pos,
            }
        } else if let Some(measure_name) = &col.measure {
            // The measure target is an element of the "Measure" dimension.
            // mc-recipe validate already enforced (a) the measure exists
            // in the model and (b) its role is Input. We just need the
            // ElementId.
            let measure_element_id = refs.element("Measure", measure_name).ok_or_else(|| {
                TesseraError::DimensionNotInCube {
                    dimension: format!("Measure/{measure_name}"),
                }
            })?;
            MappingTarget::Measure {
                measure_name: measure_name.clone(),
                measure_element_id,
            }
        } else {
            // mc-recipe validate already rejected this (MC5011 NoTarget),
            // so reaching here is an internal bug. Surface as a typed
            // error rather than panicking.
            return Err(TesseraError::SourceColumnMissing {
                column: col.source.clone(),
            });
        };

        plan.push(ResolvedColumnMapping {
            source: col.source.clone(),
            source_index,
            target,
            scale: col.scale,
        });
    }
    Ok(plan)
}

/// Resolve every `recipe.defaults` entry to a (DimensionId, ElementId,
/// dim_position) triple.
fn resolve_defaults(
    recipe: &Recipe,
    refs: &ModelRefs,
) -> Result<Vec<ResolvedDefault>, TesseraError> {
    let dim_position: HashMap<&str, usize> = refs
        .dimension_order
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect();

    let mut out = Vec::with_capacity(recipe.defaults.len());
    for (dim_name, element_name) in &recipe.defaults {
        let dim_id = refs.dimensions.get(dim_name).copied().ok_or_else(|| {
            TesseraError::DimensionNotInCube {
                dimension: dim_name.clone(),
            }
        })?;
        let dim_pos = *dim_position.get(dim_name.as_str()).ok_or_else(|| {
            TesseraError::DimensionNotInCube {
                dimension: dim_name.clone(),
            }
        })?;
        let element_id =
            refs.element(dim_name, element_name)
                .ok_or_else(|| TesseraError::UnknownElement {
                    row_index: 0,
                    dimension: dim_name.clone(),
                    element: element_name.clone(),
                })?;
        out.push(ResolvedDefault {
            dim_name: dim_name.clone(),
            dim_id,
            dim_position: dim_pos,
            element_id,
            element_name: element_name.clone(),
        });
    }
    Ok(out)
}
