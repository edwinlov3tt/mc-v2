//! `mc-recipe` — Phase 5A Stream B Tessera recipe parser + validator.
//!
//! A *recipe* is a declarative YAML document that describes how to bring
//! external data into a Mosaic cube: where the data lives (source +
//! driver), how source columns map onto cube dimensions and measures
//! (column mappings + defaults), and what to do when things go wrong
//! (`on_error` + diagnostics).
//!
//! This crate is a **parser + validator library only**. It does NOT
//! connect to data sources, fetch rows, transform data, or write to
//! cubes. Source-driver execution lives in `mc-drivers` (Stream C);
//! orchestration + writeback lives in `mc-tessera` (Stream D). See
//! [ADR-0010](../../../docs/decisions/0010-phase-5-tessera-architecture.md)
//! for the full Phase 5 architecture.
//!
//! ## Public surface
//!
//! - [`Recipe`] and the supporting schema types (frozen by ADR-0010
//!   Appendix B).
//! - [`parse`] — `&str` → [`Recipe`] (MC5001 / MC5002 / MC5007 / MC5012).
//! - [`to_yaml`] — [`Recipe`] → `String` (roundtrip stability).
//! - [`validate_recipe`] — validate a parsed recipe against a loaded
//!   `mc_model::ValidatedModel` (MC5003-MC5006, MC5008-MC5011,
//!   MC5016-MC5018).
//! - [`PathContext`] — optional file-system context enabling MC5017
//!   path-escape detection.
//! - [`RecipeError`] — every distinct way a recipe can be wrong, with a
//!   stable `.code()` accessor.
//! - [`Diagnostic`] / [`Severity`] / [`sort_diagnostics`] /
//!   [`diagnostics_to_json`] — the JSON envelope shape (Phase 3B
//!   `schema_version: "1.0"`).
//!
//! ## Diagnostic codes (MC5xxx)
//!
//! | Code   | Meaning                                                  | Stage      |
//! |--------|----------------------------------------------------------|------------|
//! | MC5001 | YAML / deserialization failure                           | parse      |
//! | MC5002 | Unknown driver kind                                      | parse      |
//! | MC5003 | `source.table` and `source.query` both set               | validate   |
//! | MC5004 | Column references unknown dimension                      | validate   |
//! | MC5005 | Column references unknown measure                        | validate   |
//! | MC5006 | Column type incompatible with target measure type        | validate   |
//! | MC5007 | Missing required field                                   | parse      |
//! | MC5008 | Default references unknown dimension                     | validate   |
//! | MC5009 | Default references unknown element                       | validate   |
//! | MC5010 | Duplicate source column                                  | validate   |
//! | MC5011 | Column has no single target (no-target or ambiguous)     | validate   |
//! | MC5012 | Recipe `version` is not 1                                | parse      |
//! | MC5013 | Credential `${env.X}` interpolation failure              | runtime ⁎  |
//! | MC5014 | Source file not readable                                 | runtime ⁎  |
//! | MC5015 | Source connection failure                                | runtime ⁎  |
//! | MC5016 | Dimension in both `columns:` and `defaults:`             | validate   |
//! | MC5017 | `model:` path escapes workspace root                     | validate   |
//! | MC5018 | Column maps to a Derived measure                         | validate   |
//!
//! ⁎ Codes MC5013-MC5015 are reserved for runtime emission by Stream D
//! (`mc-tessera`); `mc-recipe` defines them as variants for namespace
//! uniformity but does not fire them itself (no FS / network / env
//! access in Stream B).

#![deny(rust_2018_idioms)]
#![warn(missing_debug_implementations)]
// Per CLAUDE.md §3.1 + the Stream B handoff acceptance gate #10: no
// unwrap/expect/panic in non-test library code paths. Tests under
// `#[cfg(test)]` are exempt (matches the mc-model crate's policy).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]
#![cfg_attr(not(test), deny(clippy::expect_used))]

pub mod diagnostic;
pub mod error;
pub mod parse;
pub mod schema;
pub mod validate;

pub use diagnostic::{
    diagnostics_to_json, sort_diagnostics, Diagnostic, DiagnosticCode, Severity, SCHEMA_VERSION,
};
pub use error::{ColumnTargetIssue, RecipeError};
pub use parse::{parse, to_yaml};
pub use schema::{
    BatchConfig, ColumnMapping, DriverKind, LongFormatConfig, OnError, OnMissingElement, Recipe,
    SourceConfig, SourceFormat, WriteDisposition,
};
pub use validate::{validate_recipe, PathContext};
