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
pub mod error;
pub mod parse;
pub mod schema;
pub mod validate;

use std::path::Path;

pub use compile::{compile, CompiledCube, ModelRefs};
pub use error::{Error, ParseError, ParseErrorKind, Span, ValidationError};
pub use parse::parse;
pub use schema::{
    ParsedDimension, ParsedElement, ParsedGoldenTest, ParsedHierarchy, ParsedHierarchyEdge,
    ParsedMeasure, ParsedMetadata, ParsedModel, ParsedRule, ParsedRuleBody, ParsedScalar,
    ValidatedModel,
};
pub use validate::validate;

/// Load a YAML model file from disk and produce a fully-built `Cube` plus
/// the auxiliary `ModelRefs` needed to build coordinates.
///
/// All errors (parse, validate, compile) are returned via the unified
/// [`Error`] enum; multiple errors collected during validation are
/// surfaced as `Error::Validation(Vec<ValidationError>)` per ADR-0004
/// Decision 6 ("all errors return at once").
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
    load_str(&bytes, Some(path_ref.display().to_string()))
}

/// Parse + validate + compile from an in-memory YAML string. Used by tests
/// and by future Phase 4 LLM-authoring paths that don't have a file path.
pub fn load_str(yaml: &str, source_label: Option<String>) -> Result<CompiledCube, Vec<Error>> {
    let parsed = parse(yaml, source_label).map_err(|e| vec![Error::Parse(e)])?;
    let validated = validate(parsed)
        .map_err(|errs| errs.into_iter().map(Error::Validation).collect::<Vec<_>>())?;
    compile(validated).map_err(|e| vec![Error::Compile(e.to_string())])
}
