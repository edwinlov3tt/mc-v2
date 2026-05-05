//! `mc-tessera` — Phase 5A Stream D Tessera orchestrator.
//!
//! Tessera is Mosaic's data ingestion engine — the modern replacement for
//! TM1's TurboIntegrator. It reads a declarative YAML *recipe*, connects
//! to a *source* (CSV, SQLite, DuckDB, Postgres, HTTP/JSON), transforms
//! source rows into Mosaic cell coordinates, bulk-writes via
//! [`mc_core::WriteBatch`], persists a sidecar audit trail, and exits with
//! a [`ImportReport`].
//!
//! The orchestrator is the *integration point* for Phase 5A's three other
//! streams:
//!
//! - **Stream A** — [`mc_core::WriteBatch`] (the bulk-write kernel).
//! - **Stream B** — [`mc_recipe`] (recipe parser + validator).
//! - **Stream C** — [`mc_drivers`] (`SourceDriver` trait + 6 drivers).
//!
//! See [ADR-0010](../../docs/decisions/0010-phase-5-tessera-architecture.md)
//! Decision 1 (acceptance criteria), Decision 2.5 (`.tessera/` sidecar
//! state), Decision 7 (recipe semantic rules), and Appendix D (Stream D
//! interface contract).
//!
//! ## Public surface
//!
//! - [`Tessera`] — the namespace for the four CLI verbs (`prepare`,
//!   `dry_run`, `apply`, `rollback`, `history`, plus `load_active` for
//!   "what does the planner see right now?").
//! - [`PreparedImport`] — the materialized recipe + cube + driver +
//!   column plan.
//! - [`ImportReport`] / [`TimingBreakdown`] — what `apply()` returns.
//! - [`DryRunReport`] — what `dry_run()` returns.
//! - [`AuditRecord`] — one entry in `<model_dir>/.tessera/audit.jsonl`.
//! - [`TesseraError`] — every failure mode.
//! - [`SecretResolver`] / [`EnvVarSecretResolver`] / [`SecretError`] —
//!   the Grout (Phase 5E) forward-compat trait + Phase 5A's only impl.
//! - [`Sidecar`] — utility for opening / inspecting a `.tessera/` layout.
//!
//! ## Quick start
//!
//! ```no_run
//! use std::path::Path;
//! use mc_tessera::Tessera;
//!
//! // 1. prepare: load recipe + model + driver, validate, resolve.
//! let prepared = Tessera::prepare(Path::new("./acme-import.recipe.yaml"))?;
//!
//! // 2. dry_run: report the plan without writing.
//! let dry = Tessera::dry_run(&prepared)?;
//! println!("Will write {} mapped columns × N rows", dry.mapped_columns);
//!
//! // 3. apply: run the full pipeline.
//! let report = Tessera::apply(prepared)?;
//! println!("import {}: {} rows, {} ms", report.import_id, report.rows_written, report.timing.total_ms);
//! # Ok::<_, mc_tessera::TesseraError>(())
//! ```

#![deny(rust_2018_idioms)]
#![warn(missing_debug_implementations)]
// Per CLAUDE.md §3.1 + the Stream D handoff: no unwrap/expect/panic in
// non-test library code. Tests under `#[cfg(test)]` are exempt.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]
#![cfg_attr(not(test), deny(clippy::expect_used))]

pub mod error;
pub mod incremental;
pub mod prepare;
pub mod runner;
pub mod schedule;
pub mod secrets;
pub mod sidecar;
pub mod transform;

pub use error::TesseraError;
pub use incremental::{
    compute_new_watermark, inject_watermark, load_state, reset_state, save_state, IncrementalState,
};
pub use prepare::{
    prepare_from_path, prepare_from_yaml, MappingTarget, PreparedImport, ResolvedColumnMapping,
    ResolvedDefault,
};
pub use runner::{
    DryRunReport, ImportReport, LoadedActive, Tessera, TimingBreakdown, DEFAULT_BATCH_SIZE,
};
pub use schedule::{
    schedule_add, schedule_list, schedule_remove, CronExpr, Daemon, Schedule, ScheduleRegistry,
};
pub use secrets::{interpolate, EnvVarSecretResolver, SecretError, SecretResolver};
pub use sidecar::{
    append_audit, append_quarantine, manifest_mark_active, manifest_mark_inactive, read_audit,
    read_cells_jsonl, read_manifest, write_cells_jsonl, write_manifest, ActiveImports, AuditRecord,
    CellRecord, CellValueJson, QuarantineCell, QuarantineRecord, Sidecar,
};
pub use transform::{
    transform_batch, transform_batch_long, transform_batch_long_with_policy,
    transform_batch_with_policy, RowFailure, TesseraErrorOwned, TransformedBatch,
};
