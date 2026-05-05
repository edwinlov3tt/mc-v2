//! `mc-model` — Phase 3A model-definition layer.
//!
//! Translates human-authored YAML cube definitions into `mc_core::Cube`
//! instances. The kernel (`mc-core`) is unchanged by this crate; we layer
//! on top of its public builder API.
//!
//! # Three-stage pipeline (ADR-0004 Decision 9 — mandatory)
//!
//! ```text
//! YAML bytes
//!     │  parse  (serde_yaml deserialization + safe-subset prefilter)
//!     ▼
//! ParsedModel        ← mirrors YAML 1:1; owned strings; no IDs allocated
//!     │  validate (Decision 6 — 9 validators, all errors returned at once)
//!     ▼
//! ValidatedModel     ← every check passed; canonical name → index maps;
//!     │                still no `mc_core` IDs
//!     │  compile (allocate IDs, walk to mc_core builders)
//!     ▼
//! mc_core::Cube
//! ```
//!
//! Each stage emits its own error type with distinct blame semantics:
//! `ParseError` blames YAML syntax, `ValidationError` blames model
//! semantics, `EngineError` blames the kernel. Phase 4 (LLM authoring)
//! and Phase 6 (UI editor) consume the intermediate types directly;
//! that's why the staging is mandatory.

#![deny(rust_2018_idioms)]
#![warn(missing_debug_implementations)]
// Per CLAUDE.md §3.1: no unwrap/expect/panic in library code paths.
// Tests and examples are exempt.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]
#![cfg_attr(not(test), deny(clippy::expect_used))]

pub mod compile;
pub mod csv;
pub mod diagnostic;
pub mod error;
pub mod formula;
pub mod inputs;
pub mod inspect;
pub mod lint;
pub mod parse;
pub mod schema;
pub mod validate;

use std::path::Path;

pub use compile::{compile, CompiledCube, ModelRefs};
pub use diagnostic::{
    diagnostics_to_json, diagnostics_to_text, sort_diagnostics, Diagnostic, DiagnosticCode,
    ModelPath, Severity, SCHEMA_VERSION,
};
pub use error::{Error, ParseError, ParseErrorKind, Span, ValidationError};
pub use inputs::{
    apply_canonical_inputs, apply_fixture, resolve_inputs, ResolvedFixture, ResolvedInputSet,
    ResolvedInputs, ResolvedRow, VALUE_COLUMN,
};
pub use inspect::{inspect_json, inspect_text, inspect_text_with_diagnostics, ModelSummary};
pub use lint::{lint, lint_with_file};
pub use parse::parse;
pub use schema::{
    ParsedActualRefBody, ParsedBenchmark, ParsedBenchmarkRefBody, ParsedBinopBody,
    ParsedBucketBody, ParsedClampBody, ParsedDimension, ParsedElement, ParsedFixture,
    ParsedGoldenTest, ParsedHierarchy, ParsedHierarchyEdge, ParsedIfBody, ParsedInlineRows,
    ParsedInputSet, ParsedLagBody, ParsedLookupRefBody, ParsedLookupTable, ParsedMeasure,
    ParsedMeasureRefBody, ParsedMetadata, ParsedModel, ParsedRollingAvgBody, ParsedRowCell,
    ParsedRule, ParsedRuleBody, ParsedRuleBodyForm, ParsedSafeDivBody, ParsedScalar,
    ParsedStatusThreshold, ParsedSumOverBody, ParsedThresholdBand, ParsedUnaryBody,
    ParsedVarargBody, ValidatedModel, ValidatedRule,
};
pub use validate::validate;

/// Load a YAML model file from disk and produce a fully-built `Cube` plus
/// the auxiliary `ModelRefs` needed to build coordinates.
///
/// Phase 3C extends this to run the four-stage pipeline:
/// **parse → validate → resolve_inputs → compile**. Resolve-inputs is a
/// named stage that reads any sibling CSV files declared by
/// `canonical_inputs:` / `test_fixtures:`, canonicalizes their paths
/// relative to the YAML model file's directory, and type-checks rows
/// against measure declarations. Errors from resolve-inputs surface as
/// `Error::Validation` (the MC2012–MC2025 codes) so the diagnostic
/// envelope shape is unchanged.
///
/// `load()` does **not** apply the resolved inputs to the cube — the
/// returned `CompiledCube.cube` is empty of input data. `mc model test`
/// (the only consumer that needs populated cells) calls
/// [`resolve_inputs`] + [`apply_canonical_inputs`] / [`apply_fixture`]
/// separately.
///
/// All errors (parse, validate, resolve_inputs, compile) are returned
/// via the unified [`Error`] enum.
pub fn load(path: impl AsRef<Path>) -> Result<CompiledCube, Vec<Error>> {
    let path_ref = path.as_ref();
    let bytes = match std::fs::read_to_string(path_ref) {
        Ok(b) => b,
        Err(e) => {
            return Err(vec![Error::Io {
                path: path_ref.display().to_string(),
                message: e.to_string(),
            }]);
        }
    };
    let parsed =
        parse(&bytes, Some(path_ref.display().to_string())).map_err(|e| vec![Error::Parse(e)])?;
    // Phase 3D: validate now returns Vec<Error> (mixing ParseError from
    // formula bodies with ValidationError from semantic checks).
    let validated = validate(parsed)?;
    // Phase 3C resolve-inputs stage. We discard the resolved data here
    // (load() doesn't apply inputs to the cube); only the validation
    // side-effects matter at this layer.
    if let Err(errs) = resolve_inputs(&validated, path_ref.parent()) {
        return Err(errs.into_iter().map(Error::Validation).collect());
    }
    compile(validated).map_err(|e| vec![Error::Compile(e.to_string())])
}

/// Parse + validate + (resolve_inputs with no file context) + compile
/// from an in-memory YAML string. Used by tests and by future Phase 4
/// LLM-authoring paths that don't have a file path.
///
/// Because there is no file context, any `canonical_inputs:` /
/// `test_fixtures:` declaration that uses `source:` will fail with
/// MC2022 (`reason: "no file context: ..."`). Inline-only declarations
/// resolve cleanly.
pub fn load_str(yaml: &str, source_label: Option<String>) -> Result<CompiledCube, Vec<Error>> {
    let parsed = parse(yaml, source_label).map_err(|e| vec![Error::Parse(e)])?;
    // Phase 3D: validate returns Vec<Error> directly.
    let validated = validate(parsed)?;
    if let Err(errs) = resolve_inputs(&validated, None) {
        return Err(errs.into_iter().map(Error::Validation).collect());
    }
    compile(validated).map_err(|e| vec![Error::Compile(e.to_string())])
}
