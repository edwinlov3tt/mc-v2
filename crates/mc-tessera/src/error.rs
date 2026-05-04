//! `TesseraError` — every distinct failure mode of the Tessera orchestrator.
//!
//! Stream D's runtime errors come from five sources:
//!
//! 1. **Recipe** — parse and validate failures fall through `mc-recipe`'s
//!    [`RecipeError`](mc_recipe::RecipeError) (MC5001 - MC5012, MC5016 -
//!    MC5018). Wrapped in [`TesseraError::Recipe`] so callers see a single
//!    error type.
//! 2. **Model** — model load / validate failures from `mc-model`. Wrapped
//!    in [`TesseraError::Model`].
//! 3. **Driver** — failures from a source driver (file not found, query
//!    failed, ...). Mapped onto MC5014 / MC5015 in [`TesseraError::driver_diagnostic`].
//! 4. **Kernel** — engine errors from `WriteBatch::commit()`. Wrapped in
//!    [`TesseraError::Engine`].
//! 5. **Tessera-specific** — element-resolution failures, transform
//!    errors, sidecar IO, secret-resolver failures. Each is its own
//!    variant.

use std::io;
use std::path::PathBuf;

use mc_core::EngineError;
use mc_drivers::DriverError;
use mc_model::Error as ModelError;
use mc_recipe::{Diagnostic, RecipeError, Severity};

use crate::secrets::SecretError;

/// Every way a Tessera orchestration can fail.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TesseraError {
    /// IO error reading the recipe YAML or a sidecar file.
    #[error("io error at {path}: {message}")]
    Io {
        /// Path that errored.
        path: PathBuf,
        /// Underlying message.
        message: String,
    },

    /// One or more validation failures against the parsed recipe.
    /// `errors` is the full list (sorted by [`mc_recipe::sort_diagnostics`]
    /// before emission). Phase 5A: every variant is severity Error.
    #[error("recipe validation failed: {} error(s)", .errors.len())]
    Recipe {
        /// The full list of validation errors.
        errors: Vec<RecipeError>,
    },

    /// Model load / validate / compile failure.
    #[error("model load failed: {} error(s)", .errors.len())]
    Model {
        /// The full list of model errors.
        errors: Vec<ModelError>,
    },

    /// Source-driver construction or fetch error. Mapped to MC5014 /
    /// MC5015 (or surfaced verbatim for other DriverError variants) by
    /// the orchestrator before this is returned.
    #[error("driver error: {0}")]
    Driver(#[from] DriverError),

    /// Engine error from `WriteBatch::commit()` (validation reject, type
    /// mismatch, derived-cell write rejection, ...).
    #[error("engine error: {0}")]
    Engine(#[from] EngineError),

    /// Credential resolution failed. Carries the path of the offending
    /// `${env.X}` reference and the variable name. Maps to MC5013.
    #[error("credential resolution failed for {variable:?}: {message}")]
    Secret {
        /// Path of the credential entry in the recipe.
        path: String,
        /// The `${env.X}` variable name.
        variable: String,
        /// Underlying message.
        message: String,
    },

    /// A row references a dimension element that doesn't exist in the
    /// model. Phase 5A's `on_missing_element` is always `Error`; this
    /// variant is what gets logged or quarantined per `on_error`.
    #[error("row {row_index}: unknown element {element:?} in dimension {dimension:?}")]
    UnknownElement {
        /// Zero-based row index within the import.
        row_index: usize,
        /// Dimension name.
        dimension: String,
        /// Element value as it appeared in the source row.
        element: String,
    },

    /// A row contained a NULL or empty value where a measure value was
    /// required.
    #[error("row {row_index}: missing value for measure {measure:?}")]
    MissingMeasureValue {
        /// Zero-based row index within the import.
        row_index: usize,
        /// Target measure name.
        measure: String,
    },

    /// A column type didn't coerce to the expected ScalarValue (e.g.,
    /// non-numeric in a measure column, NaN/Inf).
    #[error("row {row_index}: type coercion failed for column {column:?}: {message}")]
    TypeCoercion {
        /// Zero-based row index within the import.
        row_index: usize,
        /// Source column name.
        column: String,
        /// Description of the mismatch.
        message: String,
    },

    /// `on_error: abort` and at least one row failed during the import.
    /// Carries the underlying first failure for context. The cube was
    /// not mutated (no commit ran).
    #[error("import aborted at row {row_index}: {cause}")]
    AbortedImport {
        /// Zero-based row index of the failure.
        row_index: usize,
        /// The underlying error.
        cause: Box<TesseraError>,
    },

    /// The recipe references a source column that doesn't exist in the
    /// driver's schema.
    #[error("recipe references source column {column:?} not present in driver schema")]
    SourceColumnMissing {
        /// Source column name from the recipe.
        column: String,
    },

    /// The driver's schema names a column the recipe doesn't mention and
    /// it isn't `skip: true` — defensive, fires only when strict-mode
    /// is enabled. Phase 5A is permissive (extra columns are silently
    /// ignored), so this variant is reserved.
    #[error("source column {column:?} is not mapped and not marked skip")]
    UnmappedSourceColumn {
        /// Source column name.
        column: String,
    },

    /// The cube being written to doesn't have the dimension the recipe
    /// targets. Defensive — should be caught by recipe validation.
    #[error("internal: cube does not have dimension {dimension:?}")]
    DimensionNotInCube {
        /// Dimension name.
        dimension: String,
    },

    /// The model file path resolution (relative to the recipe directory)
    /// failed.
    #[error("could not resolve model path {model:?} relative to {recipe_dir:?}")]
    ModelPathResolution {
        /// `model:` field from the recipe.
        model: String,
        /// Recipe directory.
        recipe_dir: PathBuf,
    },

    /// The audit / sidecar layout produced an inconsistent state (e.g.,
    /// `active-imports.json` references an `import_id` whose
    /// `imports/<id>.cells.jsonl` is missing).
    #[error("sidecar inconsistent: {message}")]
    SidecarInconsistent {
        /// Description of the inconsistency.
        message: String,
    },

    /// Failed to serialize a sidecar record to JSON.
    #[error("sidecar serialization failed: {message}")]
    SidecarSerialize {
        /// Description.
        message: String,
    },

    /// Failed to deserialize a sidecar record from JSON.
    #[error("sidecar deserialization failed at {path}: {message}")]
    SidecarDeserialize {
        /// Path that failed.
        path: PathBuf,
        /// Description.
        message: String,
    },

    /// A `Tessera::rollback` was requested for an import_id that does
    /// not appear in the audit log or active-imports manifest.
    #[error("import {import_id:?} not found in audit log at {audit_path:?}")]
    ImportNotFound {
        /// Requested import id.
        import_id: String,
        /// Path of the audit log searched.
        audit_path: PathBuf,
    },
}

impl TesseraError {
    /// Convert an [`io::Error`] for `path` into a [`TesseraError::Io`].
    pub fn io(path: impl Into<PathBuf>, e: io::Error) -> Self {
        TesseraError::Io {
            path: path.into(),
            message: e.to_string(),
        }
    }

    /// Convert a [`SecretError`] from credential resolution into a
    /// [`TesseraError::Secret`]. Stamps MC5013 at diagnostic-emission
    /// time.
    pub fn from_secret(path: impl Into<String>, e: SecretError) -> Self {
        match e {
            SecretError::EnvNotSet { variable } => TesseraError::Secret {
                path: path.into(),
                variable,
                message: "environment variable not set".to_string(),
            },
            SecretError::UnsupportedScheme { scheme } => TesseraError::Secret {
                path: path.into(),
                variable: scheme.clone(),
                message: format!("unsupported secret scheme: {scheme}"),
            },
        }
    }

    /// Map a [`DriverError`] to a Phase-3B-style diagnostic. MC5014 for
    /// `SourceFileNotFound`, MC5015 for `ConnectionFailed`, MC5xxx
    /// fallback for all other variants.
    pub fn driver_diagnostic(driver_path: &str, err: &DriverError) -> Diagnostic {
        match err {
            DriverError::SourceFileNotFound { path, message } => Diagnostic {
                code: "MC5014",
                severity: Severity::Error,
                path: driver_path.to_string(),
                message: format!("source file not found: {} — {message}", path.display()),
            },
            DriverError::ConnectionFailed { target, message } => Diagnostic {
                code: "MC5015",
                severity: Severity::Error,
                path: driver_path.to_string(),
                message: format!("connection failed to {target}: {message}"),
            },
            other => Diagnostic {
                code: "MC5015",
                severity: Severity::Error,
                path: driver_path.to_string(),
                message: format!("driver error: {other}"),
            },
        }
    }

    /// Convert a [`TesseraError::Secret`] into a Phase-3B-style MC5013
    /// diagnostic.
    pub fn secret_diagnostic(&self) -> Option<Diagnostic> {
        match self {
            TesseraError::Secret {
                path,
                variable,
                message,
            } => Some(Diagnostic {
                code: "MC5013",
                severity: Severity::Error,
                path: path.clone(),
                message: format!("credential interpolation failed for {variable:?}: {message}"),
            }),
            _ => None,
        }
    }

    /// Render every recipe error inside this [`TesseraError::Recipe`]
    /// variant as a [`Diagnostic`]. Returns an empty Vec for other
    /// variants.
    pub fn recipe_diagnostics(&self) -> Vec<Diagnostic> {
        match self {
            TesseraError::Recipe { errors } => errors.iter().map(|e| e.to_diagnostic()).collect(),
            _ => Vec::new(),
        }
    }
}
