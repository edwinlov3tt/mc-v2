# ADR-0010: Phase 5 — Tessera Architecture (Mosaic's Data Ingestion Engine)

**Status:** Accepted (with 12 acceptance amendments — see "Acceptance amendments" section below)
**Date:** 2026-05-04 (Proposed); 2026-05-04 (Accepted, same day after GPT + Desktop reviews)
**Deciders:** project owner
**Phase:** 5 precondition (per [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md))

> Phase 4 shipped: the Mosaic plugin (4A at `36af56c`, tag `phase-4a-mosaic-plugin`) + Python reference adapters (4B at `b5b6229`, tag `phase-4b-python-adapters`). Mosaic can now author models via natural language. **Phase 5 answers the next question: how does real-world data get INTO a Mosaic cube?** The answer is **Tessera** — Mosaic's declarative, schema-validated, LLM-authorable, blazing-fast data ingestion engine, positioned as the modern replacement for TM1's TurboIntegrator.
>
> Per [`../process-notes.md`](../process-notes.md) §1's self-test, Phase 5 requires **ADR-first flow** — it fails questions 1 (kernel change: `WriteBatch` in `mc-core`), 2 (new runtime deps: `rusqlite`, `duckdb`, `postgres`, `ureq`, `csv`), 3 (public API changes: new types in `mc-core`), and 4 (scope >> 1500 LOC). Phase 5 is the project's largest phase to date.

---

## Strategic centerpiece (read this first)

**Performance is the moat. Connectors are commodity; Tessera's bulk-write architecture is proprietary IP.**

TM1's TurboIntegrator (TI) is a 30-year-old scripting language requiring hand-written row-by-row mapping logic. Every enterprise planning tool (Anaplan, Pigment, Planful, Board, Jedox) has some form of data import — they're all roughly equivalent on features. The differentiation opportunity is speed and ergonomics:

- **Speed:** Tessera's `WriteBatch` API amortizes the per-cell costs that dominate the current `Cube::write` path (revision bumps, dirty-set updates, listener fires, hierarchy ancestor walks) into a single commit. The performance target is **50–100× over per-cell writes at 1M+ cell scale** (Tiers 1+2 combined). At that speed, a planner can run a full quarterly data refresh while they wait — no overnight batch job, no "come back tomorrow."

- **Ergonomics:** Tessera's recipe format is declarative YAML (not a programming language), schema-validated against the target cube (type errors caught before execution, not at row 500,000), and LLM-authorable via the Phase 4 plugin infrastructure (Phase 5B). A non-technical planner says "import last quarter's HubSpot data from this CSV"; the system proposes a recipe, the planner reviews it, and Tessera executes it with audit log + rollback.

**Future implementers reading this ADR will optimize for the wrong thing if they think connectors are the deliverable.** Connectors are the commodity surface (4 in Phase 5A, more in 5C — CSV, SQLite, DuckDB, Postgres, HTTP/JSON, eventually MySQL, D1, Snowflake, BigQuery). The load-bearing deliverables are: (1) `WriteBatch` (the kernel performance unlock), (2) the recipe format (the declarative contract that makes ingestion schema-validated and LLM-authorable), and (3) the reconciliation + audit layer (the "defensible in an audit" property that every enterprise deployment needs).

The name is **Tessera** — in mosaic art, a tessera is a single tile. Each ingested cell is a tessera being placed into the larger Mosaic. The crate is `mc-tessera`; the CLI verbs are `mc tessera {apply, dry-run, history, rollback, audit}`.

The companion secrets layer is **Grout** — what holds the mosaic tiles together. For Phase 5A, Grout is a `SecretResolver` trait + `EnvVarSecretResolver` implementation living inside `mc-tessera`. The future `mc-grout` crate ships when Phase 5E promotes.

---

## Context

Phases 1–2 built the kernel (deterministic compute, consolidation, dirty propagation, snapshots). Phase 3 built the model layer (YAML schema, validation, diagnostics, formulas). Phase 4 built the authoring layer (plugin, LLM adapters). **Every piece an enterprise deployment needs exists — except the ability to connect to real data.**

Today, the only way to populate a Mosaic cube is:

- `mc_fixtures::write_canonical_inputs()` (Rust code, test-only)
- `mc model test <yaml>` with inline/CSV canonical inputs (Phase 3C)
- Manual `Cube::write()` calls from Rust

None of these work for real-world data ingestion at scale. Phase 5's job is to close this gap with an engine that:

1. **Connects** to external data sources (databases, APIs, files).
2. **Transforms** source rows into cube coordinates using a declarative mapping recipe.
3. **Writes** the data into the cube at bulk speed (not per-cell).
4. **Audits** every import with provenance, rollback, and reconciliation.
5. **Is authorable** by LLMs using the Phase 4 plugin infrastructure (Phase 5B).

**Phase 5 is structurally different from Phases 1–4.** Prior phases had a single load-bearing deliverable and were built sequentially. Phase 5 has four parallelizable workstreams (kernel performance, recipe format, source drivers, orchestrator) that depend on each other through clean interfaces but don't need to be built sequentially. This is the first phase where parallel development isn't just nice-to-have — it's the right organizational shape. ADR-0010 defines the interfaces between workstreams; the per-stream handoffs commit the implementation contracts.

---

## Decisions needed

The 13 decisions below are listed in dependency order — answering #1 informs #2, etc.

### Decision 1: What does Phase 5 deliver?

**Question:** What does the user observe at Phase 5A exit?

**My recommendation:** Phase 5A ships when **all** of the following hold:

1. A user writes a Tessera recipe (declarative YAML) that maps an external data source (CSV, SQLite query, DuckDB query, Postgres query, or HTTP/JSON endpoint) to a Mosaic cube's dimensions and measures.
2. `mc tessera dry-run <recipe>` validates the recipe against the target cube schema, reports any mapping errors via MC5xxx diagnostic codes in the Phase 3B JSON envelope, and exits without writing.
3. `mc tessera apply <recipe>` executes the recipe: connects to the source, fetches data in batches, transforms rows to cube coordinates, bulk-writes via `WriteBatch`, captures an audit record, and exits with a summary report (rows written, rows failed, timing breakdown).
4. `mc tessera rollback <import_id>` restores the cube to its pre-import snapshot exactly.
5. The headline performance gate: a 100K-row SQLite recipe import completes in ≤ 3 seconds on the project's reference hardware. The stretch target: a 1M-cell `WriteBatch::commit()` completes in ≤ 5 seconds (≥ 30× speedup over extrapolated per-cell baseline).
6. An equivalence test proves that ingesting Acme's `acme.inputs.csv` via a Tessera recipe produces byte-identical cube state to `mc_fixtures::write_canonical_inputs()` — the Phase 3C equivalence pattern, extended to Tessera.
7. `mc-core` and `mc-fixtures` remain locked except for the documented `WriteBatch` + `WritebackContext` additions to `mc-core`. `mc-model` remains fully locked. The Phase 4 plugin content (`mosaic-plugin/skills/`, `agents/`, `commands/`) remains locked except for new import-related skill files added in Phase 5A (content only, no structural changes to the plugin).

### Decision 2: Architecture — 4-stream parallel decomposition

**Question:** How is Phase 5A organized for parallel development?

**Decision:** Four workstreams that meet at well-defined interface contracts:

```
                    ┌────────────────────────────────────────────────────┐
                    │              Phase 5 Master Plan (ADR-0010)        │
                    │   (interface contracts; locked before parallel)    │
                    └────────────────────────────────────────────────────┘
                                          │
        ┌─────────────────────────────────┼─────────────────────────────────┐
        │                                 │                                 │
        ▼                                 ▼                                 ▼
  ┌───────────┐                    ┌───────────┐                      ┌───────────┐
  │ Stream A: │                    │ Stream B: │                      │ Stream C: │
  │  Kernel   │                    │  Recipe   │                      │  Source   │
  │ WriteBatch│                    │  Format   │                      │ Drivers   │
  │ (mc-core) │                    │(mc-recipe)│                      │(mc-drivers│
  └───────────┘                    └───────────┘                      └───────────┘
        │                                 │                                 │
        └─────────────────────────────────┼─────────────────────────────────┘
                                          ▼
                                  ┌───────────────┐
                                  │  Stream D:    │
                                  │  Tessera      │
                                  │  Orchestrator │
                                  │ (mc-tessera)  │
                                  └───────────────┘
```

**Streams A, B, C develop fully in parallel** against frozen interface contracts (see Appendices A–D). Each stream gets its own git worktree, its own Claude Code instance, its own handoff document, and its own completion report.

**Stream D starts after Streams A, B, C reach interface stability** (typically week 3–4). It integrates the three streams into the Tessera orchestrator.

**Integration order for merging to `main`:** A first (kernel foundation), then B and C (independent new crates, either order), then D (depends on all three).

**Why this matters:** This is the first time multiple Claude Code instances develop in parallel. The master plan (this ADR) locks the interfaces BEFORE any stream starts coding; any interface change during development requires a SPEC QUESTION + master plan amendment, NOT a unilateral stream decision.

**Parallel-stream Cargo governance (binding):** Only the PM / integration branch owns root `Cargo.toml` and `Cargo.lock`. Streams may use local dependency changes in their worktree, but final dependency integration happens through the PM merge branch. Any stream needing a new dependency NOT listed in Decision 4's pinning matrix must open a SPEC QUESTION — no unilateral dep additions. This prevents parallel Claude Code instances from creating dependency chaos or conflicting Cargo.lock states.

### Decision 2.5: Durability — where do imported cells live after `mc tessera apply` exits?

**Question:** `mc tessera apply` writes cells into a `Cube` in memory and records audit metadata. But where is the updated cube state when the CLI process exits? Without a persistence answer, `mc tessera {apply, history, rollback}` are ambiguous.

**Decision:** Phase 5A ships a **sidecar state model** under `<model_dir>/.tessera/`. No full persistence engine. The sidecar is the minimal state that makes the CLI verbs meaningful without pretending we already have a database.

```
<model_dir>/.tessera/
├── audit.jsonl                         # append-only; one JSON record per import
├── imports/
│   └── <import_id>.cells.jsonl         # persisted cells written by this import
├── snapshots/
│   └── <snapshot_id>.cells.jsonl       # pre-commit snapshot (for rollback)
└── active-imports.json                 # manifest of currently-active imports
```

**Phase 5A CLI verb semantics (now unambiguous):**

- **`mc tessera apply <recipe>`** — validate recipe, execute import, write cells into in-memory Cube, persist the imported cells to `imports/<import_id>.cells.jsonl`, capture pre-commit snapshot to `snapshots/`, append audit record to `audit.jsonl`, update `active-imports.json` manifest.
- **`mc tessera rollback <import_id>`** — mark the import inactive in the manifest, restore the pre-commit snapshot. The import's `.cells.jsonl` stays on disk for audit purposes; it's just no longer active.
- **`mc tessera history <model>`** — read `audit.jsonl`; print the import timeline.
- **`mc tessera dry-run <recipe>`** — validate + plan without writing. No `.tessera/` side effects.
- **Model load with actuals** — `mc_model::load(path)` + active Tessera imports = the full cube state a planner sees. Phase 5A's model-load-with-imports integration is Stream D's deliverable.

**Why sidecar (not a real database):**

- Phase 5A is proving the ingestion architecture, not building a storage engine.
- JSON Lines (`.jsonl`) is trivially debuggable (tail, grep, jq), human-readable, append-only.
- The sidecar is `.gitignore`-able by default (users don't commit imported data to source control).
- Phase 7 may introduce a real persistence layer (SQLite-backed, mmap-backed, or external); the sidecar is the Phase-5A-scoped stepping stone, not the long-term answer.

**Hard rule:** the `.tessera/` directory is NEVER committed to source control. The `.gitignore` in the model directory should include `.tessera/`. Imported data is ephemeral; the model definition (YAML + CSV) is the source of truth.

### Decision 3: mc-core unlock — WriteBatch API

**Question:** How does bulk data get written into the cube, and what changes to the locked kernel does this require?

**Decision:** `mc-core` gains two new public types: `WriteBatch` and `WritebackContext`. This is the ONLY `mc-core` change permitted in Phase 5A. All other `mc-core` public API surface remains unchanged.

**The current write API** (for context — unchanged, still the per-cell path):

```rust
// Existing — stays exactly as-is
pub fn write(&mut self, req: WritebackRequest) -> Result<WritebackResult, EngineError>
```

**The new bulk-write API** (Phase 5A addition):

```rust
/// Source identification + audit metadata for a bulk import.
pub struct WritebackContext {
    pub source_name: String,       // e.g., "hubspot_q3_export.csv"
    pub import_id: String,         // unique per-import; generated by mc-tessera
    pub principal: PrincipalId,    // who initiated the import
}

/// Stages writes for atomic batch commit.
///
/// Lifecycle: `new()` → `push()` / `push_batch()` → `commit()`.
///
/// **Atomicity contract (binding):**
/// - `new()` does NOT mutate the cube. No snapshot captured at this point.
/// - `push()` / `push_batch()` only stage. No cube mutation. No side effects.
///   Dropping a WriteBatch before commit has NO side effects — no rollback needed.
/// - `commit()` runs in three phases:
///   1. **Validate** all staged writes (type checks, derived-cell rejection,
///      lock/permission checks). On validation failure → return Err, no mutation.
///   2. **Snapshot** — capture pre-commit snapshot immediately before mutation.
///   3. **Apply** — write all cells, single revision bump, single dirty-set
///      update, single listener fire. On mid-apply failure → rollback to the
///      snapshot captured in step 2; return Err.
/// - Dropping after successful commit does NOT rollback (commit is final).
/// - `rollback()` is available for explicit rollback AFTER commit (restores the
///   step-2 snapshot). This is the `mc tessera rollback <import_id>` path.
pub struct WriteBatch<'cube> {
    cube: &'cube mut Cube,
    context: WritebackContext,
    staged: Vec<(CellCoordinate, ScalarValue)>,
    // Note: pre_snapshot is NOT captured at new(); it's captured at commit()
    // step 2. This avoids paying snapshot-clone cost for batches that are
    // staged but never committed (common in dry-run / validation flows).
}

impl<'cube> WriteBatch<'cube> {
    /// Create a new batch against the given cube. Does NOT mutate the cube.
    pub fn new(cube: &'cube mut Cube, context: WritebackContext) -> Self;

    /// Stage a single cell write. Does NOT mutate the cube.
    pub fn push(&mut self, coord: CellCoordinate, value: ScalarValue) -> Result<(), EngineError>;

    /// Stage a batch of cell writes. Does NOT mutate the cube.
    pub fn push_batch(&mut self, cells: &[(CellCoordinate, ScalarValue)]) -> Result<(), EngineError>;

    /// Return the number of staged writes.
    pub fn staged_count(&self) -> usize;

    /// Validate all staged writes, snapshot, then commit atomically.
    /// On validation failure: no mutation, no snapshot cost.
    /// On mid-apply failure: rollback to pre-commit snapshot automatically.
    pub fn commit(self) -> Result<CommitResult, EngineError>;

    /// Explicitly roll back a PREVIOUSLY COMMITTED batch by import_id.
    /// (This is a Cube method, not a WriteBatch method — WriteBatch is consumed by commit.)
    /// Lives at: `Cube::rollback_import(import_id: &str) -> Result<(), EngineError>`
}

/// Summary of a committed batch.
pub struct CommitResult {
    pub rows_written: usize,
    pub rows_failed: usize,
    pub revision_before: Revision,
    pub revision_after: Revision,
    pub dirty_count_after: usize,      // total dirty-set size post-commit
    pub newly_dirtied_count: usize,    // cells that transitioned clean → dirty in THIS commit
    pub snapshot_id: String,           // the pre-commit snapshot captured at step 2
}
```

**Why `dirty_count_after` + `newly_dirtied_count` (not `invalidated_count`):** Phase 2D (commit `0678a98`) fixed a major semantic bug where `WritebackResult.invalidated` was conflated between "cumulative dirty set" and "marginal per-write dirtied cells." The fix pinned `invalidated` to mean MARGINAL (cells dirtied by THIS write only). `CommitResult` uses two distinct fields to avoid reopening that ambiguity: `dirty_count_after` is the total dirty-set size (cumulative, useful for diagnostics); `newly_dirtied_count` is the marginal count (cells that went clean → dirty during this batch commit). Both are well-defined; neither reuses the word "invalidated."

**Why snapshot at commit-time (not new()-time):** Capturing a snapshot at `new()` would pay the clone cost even for batches that are staged but never committed (e.g., dry-run flows that validate without executing). Snapshot at commit() step 2 (immediately before mutation) defers the cost to the moment it's actually needed. For a 1M-cell batch, this saves one full cube-clone on every dry-run invocation.

**Performance strategy (tiers):**

| Tier | What it does | Expected speedup | Ships in |
|---|---|---|---|
| **Tier 1** | Single revision bump per batch, batched dirty tracking, deferred listener firing | 10–30× over per-cell writes | Phase 5A (Stream A) |
| **Tier 2** | SoA memory layout for staged writes, sorted-by-coordinate insertion path, SIMD-amenable validation pass | Additional 2–5× on Tier 1 | Phase 5A (Stream A, if time permits) |
| **Tier 3** | Bounded parallelism with `rayon` for parse/validate phases, single-writer commit | Additional 4–8× on multi-core | Gated behind separate ADR-0012 |

**The Tier 1 performance insight:** the current `Cube::write` does a revision bump, dirty-set update, hierarchy ancestor walk, and listener fire PER CELL. For 1M cells, that's 1M revision bumps + 1M dirty-set updates. `WriteBatch::commit()` does ONE revision bump + ONE dirty-set scan + ONE listener fire for the entire batch. The amortization is the speedup.

**Why the mc-core unlock is justified:** the locked-surfaces rule (process-notes §4) says "any phase that needs to unlock either crate requires an explicit ADR documenting why." This ADR is that explicit ADR. The justification: bulk-write performance is a foundational capability that MUST live in the kernel (not in a wrapper crate) because it requires direct access to `HashMapStore`, `DirtyTracker`, `Revision`, and `Snapshot` internals. A wrapper crate calling `Cube::write()` in a loop cannot achieve the amortization — it would still be N revision bumps.

**Hard constraints on the mc-core change:**

- The ONLY new public types are `WriteBatch`, `WritebackContext`, and `CommitInfo`.
- The existing `Cube::write()` method is NOT modified. It continues to work exactly as today. Phase 5A adds a PARALLEL path, not a replacement.
- All 416 existing tests must still pass unchanged.
- No new `mc-core` dependencies beyond the existing 4 (`smallvec`, `ahash`, `thiserror`, `once_cell`).
- No `unsafe` in `WriteBatch` code (unless SIMD intrinsics in Tier 2 require it, in which case: documented, justified, minimal, and gated behind a `#[cfg]` feature flag).
- No `async`, no `rayon`, no threads in the base Phase 5A scope. Tier 3 parallelism is a separate ADR-0012.

### Decision 4: Dependency strategy — ADBC rejection + pinning matrix

**Question:** What external dependencies does Phase 5A introduce, and how are they compatible with the Rust 1.78 toolchain pin?

**Decision:** ADBC is rejected for Phase 5A. Tessera ships concrete source drivers with Mosaic-native types.

**ADBC rejection rationale (from the May 2026 due-diligence report):**

1. `arrow-rs` MSRV is 1.85, incompatible with Mosaic's 1.78 pin.
2. Non-SQLite ADBC drivers are Go FFI shims with documented Windows build breakage (arrow-adbc issue #3149).
3. ADBC's "10–100× faster" benchmarks measure warehouse-to-warehouse pulls, not 100K-row recipe imports — irrelevant to Tessera's use case.

**ADBC reconsidered only when:** arrow-rs publishes an LTS line, OR a pure-Rust ADBC Postgres driver lands, OR MSRV 1.85 is approved for unrelated reasons. The `SourceDriver` trait is designed to admit an `AdbcDriver` implementation without public API changes.

**Phase 5A dependency pinning matrix (all verified against Rust 1.78 via scratch test):**

| Crate | Version | Where it lives | Why pinned | Verified on 1.78 |
|---|---|---|---|---|
| `rusqlite` | `=0.31.0` | `mc-drivers` | Latest 1.78-clean version; `bundled` feature for zero-install SQLite | ✓ (per research §11 scratch test) |
| `duckdb` | `=1.3.2` | `mc-drivers` | Last 1.78-credible `duckdb-rs` version; `bundled` feature for zero-install DuckDB | ✓ (per research §11 scratch test) |
| `postgres` | `=0.19.9` | `mc-drivers` | Sync Postgres client; latest 0.19.13 pulls `postgres-protocol 0.6.11` → RustCrypto 0.11/0.13 → `block-buffer 0.12.0` requiring `edition2024` | ✓ (toolchain gate 2026-05-04) |
| `postgres-protocol` | `=0.6.7` | transitive pin | Last version using pre-edition2024 RustCrypto chain (sha2 0.10.x / hmac 0.12.x / md-5 0.10.x) | ✓ (toolchain gate) |
| `sha2` | `=0.10.8` | transitive pin | Pre-edition2024; sha2 0.11.0 → digest 0.11.3 → block-buffer 0.12.0 | ✓ (toolchain gate) |
| `hmac` | `=0.12.1` | transitive pin | Pre-edition2024; hmac 0.13.0 pulls digest 0.11.3 | ✓ (toolchain gate) |
| `md-5` | `=0.10.6` | transitive pin | Pre-edition2024; md-5 0.11.0 pulls digest 0.11.3 | ✓ (toolchain gate) |
| `digest` | `=0.10.7` | transitive pin | Pre-edition2024 | ✓ (toolchain gate) |
| `block-buffer` | `=0.10.4` | transitive pin | The actual blocker; 0.12.0 requires `edition2024` Cargo feature | ✓ (toolchain gate) |
| `ureq` | `2` | `mc-drivers` | Sync HTTP client for HTTP/JSON + D1 drivers; 1.78-clean, no tokio | ✓ (per research §11) |
| `csv` | `1` | `mc-drivers` | CSV parsing; pure Rust, trivially 1.78-clean | ✓ |

**Same Phase 1B precedent:** Cargo.lock transitive pins to avoid edition2024 without bumping the toolchain. The existing Phase 1B pins (`clap → 4.4.18`, `clap_lex → 0.6.0`, `half → 2.4.1`) and Phase 3A pins (`indexmap → 2.7.0`, `hashbrown → 0.15.5`) all remain; Phase 5A adds 7 new transitive pins for the Postgres crypto chain.

### Decision 5: Tokio-transitive-dependency policy (Path 2)

**Question:** Does Phase 5A accept `tokio` in the dependency tree?

**Decision: Path 2 — conditional acceptance.** Tokio is permitted as a transitive dependency of `postgres` / `tokio-postgres` ONLY. Mosaic source code remains fully synchronous.

**Binding rules:**

- No `async` keyword anywhere in Mosaic source (`crates/mc-core/`, `crates/mc-drivers/`, `crates/mc-recipe/`, `crates/mc-tessera/`, `crates/mc-cli/`, `crates/mc-model/`, `crates/mc-fixtures/`).
- No `.await` anywhere in Mosaic source.
- No `use tokio::*` anywhere in Mosaic source.
- No `tokio` or async runtime types in any public API signature.
- `tokio` appears in `Cargo.lock` ONLY as a transitive dependency of `postgres → tokio-postgres`.
- The Postgres driver lives in `crates/mc-drivers/` ONLY. `mc-core`, `mc-model`, `mc-fixtures`, and `mc-cli`'s existing source paths remain free of any tokio reference (transitive or direct).

**Mechanical enforcement:** a CI / test check that `cargo tree` shows tokio appearing only as a transitive of `postgres`/`tokio-postgres`. Implementation: a `#[test]` in `crates/mc-drivers/tests/dependency_gate.rs` that runs `cargo tree` as a subprocess and asserts the tokio-path constraint. Same pattern as Phase 4A's `plugin_lint.rs` test.

**Why Path 2 (not Path 1):** the "no tokio" rule from Phase 1 was a proxy for "no async in our code." The substance (sync-only API surface, no async contamination) is preserved even with transitive tokio. Path 1 (strict no-tokio) would limit Postgres support to DuckDB federation, which is functionally correct but slower and feature-limited vs. native `postgres` crate. Path 2 gives native Postgres at the cost of tokio in `Cargo.lock` (but never in source).

**DuckDB-federated Postgres remains a supported driver** (`driver: duckdb_postgres`) regardless. It's a legitimate driver in its own right for DuckDB-mediated cross-database joins; it is NOT a "fallback" for Path 2.

### Decision 6: Performance targets and measurement discipline

**Question:** What are the concrete performance gates for Phase 5A?

**Decision:**

| Benchmark | Target | Baseline (extrapolated) | Stream |
|---|---|---|---|
| `write_batch/commit/1K` | ≤ 10 ms | ~165 ms (1K × 165 µs per-cell) | A |
| `write_batch/commit/10K` | ≤ 100 ms | ~1.65 s | A |
| `write_batch/commit/100K` | ≤ 1 s | ~16.5 s | A |
| `write_batch/commit/1M` | ≤ 5 s | ~165 s | A |
| `tessera_apply/100K_sqlite` | ≤ 3 s (end-to-end) | n/a (new path) | D |
| `tessera_apply/acme_csv_equivalence` | byte-identical to `write_canonical_inputs()` | n/a | D |

**Baselines are extrapolated** from the current per-cell `Cube::write` cost (~165 µs on Acme per PERF.md §7.3). **Stream A's first deliverable is measured baselines at all four scale points** (1K, 10K, 100K, 1M) using the real `Cube::write` path, documented in PERF.md before any optimization. The extrapolated numbers above may be wrong — Phase 2D's writeback semantic correction changed the per-write cost profile. Measured baselines override extrapolations.

**Measurement discipline (extends PERF.md rigor to ingestion):**

- Fixed datasets per scale point (committed as test fixtures, deterministic).
- Fixed hardware specification documented alongside every number.
- Multiple measurement runs; report mean + p99 + variance.
- Before/after in the same PR; no optimization claim without a diff.
- New PERF.md section: "§7.X Tessera bulk-write performance."

### Decision 7: Recipe format

**Question:** What does a Tessera recipe look like?

**Decision:** Single-file declarative YAML, no Jinja, inheriting dbt's hierarchical defaults + dlt's `write_disposition`/`incremental` fields + Singer's config/catalog/state (flattened into one file).

**Example recipe (SQLite import):**

```yaml
version: 1
name: hubspot_q3_import
description: "Import Q3 HubSpot campaign data into the marketing-mix cube."
model: acme-marketing.yaml       # resolved relative to this recipe file's directory

source:
  driver: sqlite
  path: ./data/hubspot_export.db
  query: |
    SELECT campaign_name, channel, market, month, spend, cpc
    FROM campaign_metrics
    WHERE quarter = 'Q3'

columns:
  - source: campaign_name    # ignored — not a cube dimension
    skip: true
  - source: channel
    dimension: Channel
  - source: market
    dimension: Market
  - source: month
    dimension: Time
  - source: spend
    measure: Spend            # Input measure ✓
    type: f64
  - source: cpc
    measure: CPC              # Input measure ✓
    type: f64
  # NOTE: source `clicks` is NOT mapped. In the Acme model, Clicks is a Derived
  # measure (Clicks = Spend / CPC); the kernel computes it automatically. Phase 5A
  # writes to Input measures ONLY. See MC5018 below.

defaults:
  scenario: Actual            # all rows go to the "Actual" scenario
  version: Working            # all rows go to the "Working" version

write_disposition: replace    # Phase 5A: coordinate-level overwrite only (see below)
incremental: false            # full load; incremental config deferred to Phase 5C

batch:
  size: 50000                 # rows per WriteBatch; default 50K per research recommendation

on_error: abort               # abort | skip_row | quarantine (see semantics below)
on_missing_element: error     # error | create (create deferred to Phase 5C)

credentials:
  # Phase 5A: env-only. ${env.VAR_NAME} syntax.
  # Phase 5E (Grout): ${secret.ref} syntax.
  # SQLite example doesn't need credentials.
```

**Recipe schema (`mc-recipe` crate):**

- `version: 1` pin in every recipe; explicit handling for future versioning.
- `source:` block with `driver:` field (enum: `csv`, `sqlite`, `duckdb`, `postgres`, `duckdb_postgres`, `http_json`).
- `columns:` mapping array: each entry maps a source column to a cube dimension or measure. Type coercion (`type:`, `scale:`, `format:`) per column.
- `defaults:` block: static dimension-element assignments for dimensions not in the source data.
- `write_disposition:` enum: `replace` (overwrite), `append` (add), `merge` (upsert by coordinate). Phase 5A ships `replace` only; `append` and `merge` are Phase 5C.
- `incremental:` config: false for Phase 5A; true + watermark/cursor config for Phase 5C.
- `on_error:` enum: `abort` (default; transactional, no partial commit), `skip_row`, `quarantine`.
- `on_missing_element:` enum: `error` (default), `create` (deferred to Phase 5C — element creation during import).
- `credentials:` block: `${env.VAR_NAME}` interpolation only in Phase 5A. `${secret.ref}` resolver deferred to Phase 5E (Grout).
- `batch.size:` optional; default 50,000 rows per research recommendation.

**Recipe semantic rules (binding for Phase 5A):**

1. **Column mappings are 1:1.** A single source column maps to either one dimension OR one measure. 1:N mappings (one source column populating multiple cube targets) are deferred to Phase 5C; users who need this in 5A run multiple recipes against the same source.

2. **Defaults vs. columns mutual exclusion.** A dimension cannot appear in both `columns:` (with `dimension: X`) and `defaults: { X: ... }`. Recipe validation rejects this with **MC5016**.

3. **Input measures only.** Phase 5A writes to Input measures ONLY. Column mappings that target a Derived measure, a consolidated cell, a locked cell, or a non-writeable version/scenario state are rejected at recipe validation (`mc tessera dry-run`) with **MC5018** — before any data is fetched. The kernel already enforces `EngineError::DerivedCellNotWritable` at write-time; Tessera catches it earlier at recipe-validation-time for better UX. If a user needs to import actuals for a quantity that's currently derived (e.g., actual Clicks from HubSpot), the correct pattern is to add a separate `Actual_Clicks` input measure to the model, not to override the derived measure.

4. **`write_disposition: replace` = coordinate-level overwrite.** Phase 5A `replace` overwrites ONLY the coordinates produced by the current recipe. It does NOT clear existing values in the target slice that are absent from the incoming data. An import with missing rows does NOT wipe data. Full-slice replace (clear-then-write) is deferred until target-slice semantics exist (Phase 5C or later).

5. **`model:` path resolution.** The `model:` field is resolved relative to the recipe file's directory, matching the Phase 3C `canonical_inputs.source:` resolution rule. Path-escape (`../`) outside the recipe's directory is allowed (recipes legitimately reference models in sibling directories); path-escape outside the workspace root is rejected with **MC5017**.

6. **`on_error:` semantics (binding):**
   - **`abort`** (default): transactional. On any row error, no partial commit. `CommitResult.rows_failed` in the error report; cube state unchanged.
   - **`skip_row`**: row is logged to the import audit record with `status: "skipped"` + the diagnostic that caused it. Counted toward `rows_failed` in `ImportReport`. Does NOT block the import; remaining rows proceed.
   - **`quarantine`**: row is written to `<model_dir>/.tessera/quarantine/<import_id>.jsonl` with the original row data + the diagnostic. Counted toward `rows_failed`. Does NOT block. Quarantined rows are NOT auto-reprocessed; a future `mc tessera retry-quarantine <import_id>` command (Phase 5C) handles re-processing.

**Diagnostic codes:** MC5xxx range for recipe errors (extending Phase 3B's MC1xxx/MC2xxx/MC3xxx/MC4xxx convention). 18 codes for Phase 5A. JSON envelope shape unchanged (`schema_version: "1.0"`). See Appendix B for the full table.

### Decision 8: SourceDriver trait

**Question:** What abstraction do source connectors implement?

**Decision:** A Mosaic-native `SourceDriver` trait returning `RowBatch` (NOT Arrow `RecordBatch`).

```rust
/// A batch of rows from an external source, in column-oriented layout.
pub struct RowBatch {
    pub columns: Vec<Column>,
    pub row_count: usize,
}

pub struct Column {
    pub name: String,
    pub data: ColumnData,
}

pub enum ColumnData {
    F64(Vec<Option<f64>>),
    I64(Vec<Option<i64>>),
    Str(Vec<Option<String>>),
    Bool(Vec<Option<bool>>),
}

/// The contract every source driver implements.
pub trait SourceDriver {
    /// Return the schema (column names + types) without fetching data.
    fn schema(&self) -> Result<Vec<ColumnSchema>, DriverError>;

    /// Fetch the next batch of rows. Returns `Ok(None)` when exhausted.
    fn fetch_batch(&mut self, max_rows: usize) -> Result<Option<RowBatch>, DriverError>;

    /// Cooperative cancellation of in-flight batch fetch.
    fn cancel(&mut self);
}
```

**Why NOT Arrow RecordBatch:**

- `arrow-rs` MSRV is 1.85; Mosaic is pinned at 1.78.
- `arrow-rs` ships quarterly major-version bumps; exposing Arrow types in a public API creates an MSRV treadmill.
- Mosaic's data model (6 dimensions × measures, all `ScalarValue` typed) is vastly simpler than Arrow's columnar format. A `Vec<Option<f64>>` per column is sufficient and orders of magnitude simpler to work with.
- The `SourceDriver` trait is designed to admit a future `AdbcDriver` implementation that internally converts Arrow → `RowBatch` without changing the public API.

### Decision 9: Sub-phase decomposition

**Question:** How is Phase 5 broken into shippable increments after 5A?

**Decision:**

| Sub-phase | Deliverable | Status |
|---|---|---|
| **5A — Tessera Core Engine** | `WriteBatch` in mc-core; recipe format in mc-recipe; 4 source drivers (CSV, SQLite, DuckDB, Postgres) in mc-drivers; Tessera orchestrator in mc-tessera; CLI verbs `mc tessera {apply, dry-run, history, rollback, audit}`; Acme CSV equivalence test; 100K-row perf gate | This ADR (Phase 5A scope) |
| **5B — LLM-Assisted Recipe Authoring** | Plugin skills for import mapping; `/mosaic-import` command; `mc tessera propose` CLI verb; Phase 4B adapter updates; end-to-end fresh-instance proof (best-of-3) | Future ADR after 5A ships |
| **5C — Driver Expansion** | MySQL native, D1 REST, Snowflake/BigQuery via ODBC, cron scheduling, incremental loads, element auto-creation. Demand-driven; each driver is independent. | Future ADR(s) after 5B |
| **5D — Document/OCR Ingestion** | Document ingestion combining open-weight OCR (fast path) + vision-language models (slow path) + LLM-assisted field mapping. Full scope in a future ADR when promoted. | Placeholder only |
| **5E — Grout Proper** | Full secrets layer (vault, rotation, audit log, external secret-manager integrations). Phase 5A ships only the `SecretResolver` trait + `EnvVarSecretResolver`. Full Grout scope in a future ADR when promoted. | Placeholder only |

**No Phase 5F.** Per the "no vague TBD buckets" rule from the ADR-0008 precedent (amendment B). After 5C, any further additions are demand-driven phases scoped when a real customer or proof requires them.

### Decision 10: New crates

**Question:** What new workspace members does Phase 5A introduce?

**Decision:** Three new crates:

| Crate | Purpose | Dependencies |
|---|---|---|
| `mc-recipe` | Recipe format parser, validator, schema types | `serde`, `serde_yaml` (already in workspace via mc-model), `thiserror` |
| `mc-drivers` | `SourceDriver` trait + CSV/SQLite/DuckDB/Postgres/HTTP-JSON drivers | `csv`, `rusqlite` (bundled), `duckdb` (bundled), `postgres`, `ureq`, `thiserror` |
| `mc-tessera` | Tessera orchestrator + transformation layer + audit log + `SecretResolver` trait + `EnvVarSecretResolver` + CLI integration | `mc-core`, `mc-recipe`, `mc-drivers`, `mc-model` (for cube schema loading), `thiserror` |

**Workspace `Cargo.toml` additions:**

```toml
[workspace]
members = [
    "crates/mc-core",
    "crates/mc-fixtures",
    "crates/mc-cli",
    "crates/mc-model",
    "crates/mc-recipe",      # Phase 5A
    "crates/mc-drivers",     # Phase 5A
    "crates/mc-tessera",     # Phase 5A
]
```

**Naming convention:** all new crates follow the `mc-` prefix per CLAUDE.md's binding naming convention (`mc-` = "Mosaic Core" backronym). The "Tessera" brand name is for prose/marketing; the crate name is `mc-tessera`.

### Decision 11: mc-core locked-surfaces amendment

**Question:** What specific changes are permitted to mc-core, and what's the verification gate?

**Decision:** Stream A may add the following to `mc-core`:

**Permitted additions (exhaustive list):**

- `src/batch.rs` — NEW file containing `WriteBatch`, `WritebackContext`, `CommitInfo` types + implementation.
- `src/lib.rs` — ADD `mod batch;` + `pub use batch::{WriteBatch, WritebackContext, CommitInfo};`
- `src/cube.rs` — ADD internal helper methods that `WriteBatch::commit()` calls (e.g., `write_cells_unchecked()` for the validated-batch fast path). These MUST be `pub(crate)`, NOT `pub`.
- `Cargo.toml` — NO new dependencies. The existing 4 runtime deps (`smallvec`, `ahash`, `thiserror`, `once_cell`) are sufficient.
- `benches/tessera_writeback.rs` — NEW benchmark file for `WriteBatch` at 1K/10K/100K/1M scale points.

**Prohibited changes (exhaustive list):**

- No modifications to existing `pub fn write()` signature or behavior.
- No modifications to existing public type signatures (`WritebackRequest`, `WritebackResult`, `WriteIntent`, `Cube`, `CubeBuilder`, etc.).
- No modifications to any existing `#[test]` function.
- No new runtime dependencies.
- No `unsafe` without explicit justification documented in the Stream A completion report.
- No `async`, no `rayon`, no threads.

**Verification gate:** `git diff phase-4b-python-adapters -- crates/mc-core/src/` must show changes ONLY in the permitted files listed above. Any change outside those files is a gate failure.

### Decision 12: Out of scope for Phase 5A

| Out of scope | Phase / disposition |
|---|---|
| **LLM-assisted recipe authoring** | Phase 5B |
| **Cron scheduling / scheduled imports** | Phase 5C |
| **Incremental loads (watermark/cursor)** | Phase 5C |
| **Element auto-creation during import** | Phase 5C (`on_missing_element: create`) |
| **`write_disposition: append` and `merge`** | Phase 5C |
| **MySQL, D1, Snowflake, BigQuery drivers** | Phase 5C |
| **Document/OCR ingestion** | Phase 5D (placeholder) |
| **Full Grout (vault, rotation, external secret managers)** | Phase 5E (placeholder) |
| **Rayon / parallelism in WriteBatch** | Gated behind ADR-0012; NOT Phase 5A scope |
| **Persistent write-ahead logging** | Phase 7+ |
| **Distributed/parallel commits across nodes** | Phase 7+ |
| **Memory-mapped storage backends** | Phase 7+ |
| **Webhook triggers for imports** | Deferred indefinitely |
| **ADBC drivers** | Reconsidered only when arrow-rs ships LTS or MSRV 1.85 approved |
| **Arrow types in public API** | Never (quarterly breaking changes) |
| **Go FFI for any driver** | Never (breaks single-binary distribution) |
| **mc-model changes** | Fully locked |
| **mc-fixtures changes** | Fully locked |
| **Phase 4 plugin structural changes** | Locked; new content (import skill files) is OK, structural changes are not |
| **Toolchain bump** | Not triggered by Phase 5A; the Postgres crypto chain pins avoid it |

### Decision 13: Naming — Tessera + Grout

**Question:** What are the brandable names for Phase 5's components?

**Decision:**

| Brand name | What it is | Crate name | CLI verbs |
|---|---|---|---|
| **Tessera** | Mosaic's data ingestion engine (the TurboIntegrator replacement) | `mc-tessera` | `mc tessera {apply, dry-run, history, rollback, audit}` |
| **Grout** | Mosaic's secrets and security layer (the TurboSecurity replacement) | `mc-grout` (future; Phase 5A trait lives in `mc-tessera`) | `mc grout {init, set, get, rotate, audit}` (future) |

**The Mosaic brand family is now:**

- **Mosaic** — the engine / Large Numbers Model (Phase 1–4)
- **Tessera** — the data ingestion engine (Phase 5; Latin "tessera" = one tile of a mosaic)
- **Grout** — the secrets/security layer (Phase 5E; grout holds mosaic tiles together)

---

## Appendix A: Stream A Interface Contract — WriteBatch API

**This is the binding contract for Stream A (mc-core changes).** The types and method signatures below are frozen once ADR-0010 is accepted. Any change requires a SPEC QUESTION + ADR amendment.

```rust
// === Public types (added to mc-core) ===

#[derive(Debug, Clone)]
pub struct WritebackContext {
    pub source_name: String,
    pub import_id: String,
    pub principal: PrincipalId,
}

pub struct WriteBatch<'cube> { /* private fields */ }

/// Summary of a committed batch.
/// Uses `dirty_count_after` + `newly_dirtied_count` (NOT `invalidated_count`)
/// to avoid the Phase 2D cumulative-vs-marginal ambiguity. See Decision 3 note.
#[derive(Debug)]
pub struct CommitResult {
    pub rows_written: usize,
    pub rows_failed: usize,
    pub revision_before: Revision,
    pub revision_after: Revision,
    pub dirty_count_after: usize,     // total dirty-set size post-commit (cumulative)
    pub newly_dirtied_count: usize,   // cells that went clean → dirty in THIS commit (marginal)
    pub snapshot_id: String,          // pre-commit snapshot ID (for rollback via import_id)
}

impl<'cube> WriteBatch<'cube> {
    /// Create a new batch. Does NOT mutate the cube. No snapshot captured here.
    pub fn new(cube: &'cube mut Cube, context: WritebackContext) -> Self;

    /// Stage a single cell. Does NOT mutate the cube.
    pub fn push(&mut self, coord: CellCoordinate, value: ScalarValue) -> Result<(), EngineError>;

    /// Stage a batch of cells. Does NOT mutate the cube.
    pub fn push_batch(&mut self, cells: &[(CellCoordinate, ScalarValue)]) -> Result<(), EngineError>;

    /// Number of staged writes.
    pub fn staged_count(&self) -> usize;

    /// Validate → snapshot → apply → return result.
    /// On validation failure: no mutation, no snapshot cost.
    /// On mid-apply failure: auto-rollback to the snapshot captured at step 2.
    pub fn commit(self) -> Result<CommitResult, EngineError>;
}

// Drop before commit = no side effects (staging is pure).
// Drop after successful commit = no rollback (commit is final).
// Explicit rollback of a COMMITTED import is via Cube::rollback_import(import_id).
```

**Atomicity contract summary:**

| Phase | Mutates cube? | Snapshot cost? |
|---|---|---|
| `new()` | No | No |
| `push()` / `push_batch()` | No | No |
| `commit()` step 1 (validate) | No | No — validation failure is free |
| `commit()` step 2 (snapshot) | No (captures state) | Yes — one `store.clone()` |
| `commit()` step 3 (apply) | **Yes** | — |
| Mid-apply failure | Auto-rollback to step-2 snapshot | — |
| Drop before commit | No side effects | — |

**Stream A acceptance gates:**

1. All 416 existing tests pass unchanged.
2. `WriteBatch::commit()` at 100K cells completes in ≤ 1 second (Tier 1 target).
3. `WriteBatch::commit()` at 1M cells completes in ≤ 5 seconds (stretch target, Tiers 1+2).
4. Rollback correctness: `commit()` that fails mid-apply leaves cube state unchanged (auto-rollback to step-2 snapshot).
5. Snapshot equivalence: `WriteBatch` commit produces same cube state as N individual `Cube::write()` calls for the same data.
6. `git diff phase-4b-python-adapters -- crates/mc-core/src/` shows changes ONLY in the permitted files (Decision 11).
7. No new mc-core dependencies.
8. Measured per-cell baselines (1K/10K/100K/1M) committed to PERF.md as the FIRST Stream A commit, BEFORE `crates/mc-core/src/batch.rs` exists.

---

## Appendix B: Stream B Interface Contract — Recipe Format

**This is the binding contract for Stream B (mc-recipe crate).** The schema and diagnostic codes below are frozen once ADR-0010 is accepted.

```rust
// === Public types (mc-recipe crate) ===

#[derive(Debug, Clone, Deserialize)]
pub struct Recipe {
    pub version: u32,           // must be 1
    pub name: String,
    pub description: Option<String>,
    pub model: String,          // path to the target Mosaic YAML model
    pub source: SourceConfig,
    pub columns: Vec<ColumnMapping>,
    pub defaults: HashMap<String, String>,  // dim_name → element_name
    pub write_disposition: WriteDisposition,
    pub incremental: bool,
    pub batch: BatchConfig,
    pub on_error: OnError,
    pub on_missing_element: OnMissingElement,
    pub credentials: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SourceConfig {
    pub driver: DriverKind,
    pub path: Option<String>,
    pub query: Option<String>,
    pub table: Option<String>,  // mutual exclusion with query
    pub url: Option<String>,    // for http_json driver
    pub json_path: Option<String>,
    // ... driver-specific fields as needed
}

#[derive(Debug, Clone, Deserialize)]
pub enum DriverKind {
    Csv,
    Sqlite,
    Duckdb,
    Postgres,
    DuckdbPostgres,
    HttpJson,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ColumnMapping {
    pub source: String,
    pub dimension: Option<String>,
    pub measure: Option<String>,
    #[serde(rename = "type")]
    pub data_type: Option<String>,
    pub scale: Option<f64>,
    pub format: Option<String>,
    pub skip: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub enum WriteDisposition { Replace }  // append + merge deferred to 5C

#[derive(Debug, Clone, Deserialize)]
pub enum OnError { Abort, SkipRow, Quarantine }

#[derive(Debug, Clone, Deserialize)]
pub enum OnMissingElement { Error }  // Create deferred to 5C

#[derive(Debug, Clone, Deserialize)]
pub struct BatchConfig {
    pub size: Option<usize>,  // default 50_000
}
```

**Recipe diagnostic codes (MC5xxx):**

| Code | Fires when |
|---|---|
| MC5001 | Recipe YAML parse error (syntax) |
| MC5002 | Unknown driver kind |
| MC5003 | Both `table:` and `query:` specified (mutual exclusion) |
| MC5004 | Column references unknown dimension in target model |
| MC5005 | Column references unknown measure in target model |
| MC5006 | Column type incompatible with target measure type |
| MC5007 | Missing required field (e.g., `source.driver`, `model`, `columns`) |
| MC5008 | Default references unknown dimension |
| MC5009 | Default references unknown element in the named dimension |
| MC5010 | Duplicate column mapping (same source column mapped twice) |
| MC5011 | No dimension/measure mapping and `skip: false` (column goes nowhere) |
| MC5012 | Invalid `version:` (not 1) |
| MC5013 | Credential interpolation failure (`${env.X}` where `X` is unset) |
| MC5014 | Source file not found or not readable (path error, permission denied) |
| MC5015 | Source connection failure (Postgres DSN unreachable, HTTP endpoint down, auth rejected) |
| MC5016 | Dimension appears in both `columns:` mapping and `defaults:` block (mutual exclusion violation) |
| MC5017 | `model:` path escapes the workspace root (path-traversal protection) |
| MC5018 | Column maps to a non-writeable measure (Derived, consolidated, locked, or non-writeable version/scenario — Phase 5A writes to Input measures only) |

**Stream B acceptance gates:**

1. All example recipes parse cleanly.
2. All example recipes validate against a fixture model OR produce expected diagnostic envelopes when intentionally broken.
3. Roundtrip stability: `parse(serialize(parse(recipe))) == parse(recipe)` for all example recipes.
4. `schema_version` stays at `"1.0"` in the diagnostic JSON envelope.

---

## Appendix C: Stream C Interface Contract — SourceDriver Trait

**This is the binding contract for Stream C (mc-drivers crate).** The trait and types below are frozen once ADR-0010 is accepted.

```rust
// === Public types (mc-drivers crate) ===

pub trait SourceDriver {
    fn schema(&self) -> Result<Vec<ColumnSchema>, DriverError>;
    fn fetch_batch(&mut self, max_rows: usize) -> Result<Option<RowBatch>, DriverError>;
    fn cancel(&mut self);
}

pub struct ColumnSchema {
    pub name: String,
    pub data_type: ColumnDataType,
    pub nullable: bool,
}

pub enum ColumnDataType { F64, I64, Str, Bool }

pub struct RowBatch {
    pub columns: Vec<Column>,
    pub row_count: usize,
}

pub struct Column {
    pub name: String,
    pub data: ColumnData,
}

pub enum ColumnData {
    F64(Vec<Option<f64>>),
    I64(Vec<Option<i64>>),
    Str(Vec<Option<String>>),
    Bool(Vec<Option<bool>>),
}

// === Driver constructors ===
pub fn csv_driver(path: &Path) -> Result<impl SourceDriver, DriverError>;
pub fn sqlite_driver(path: &Path, query: &str) -> Result<impl SourceDriver, DriverError>;
pub fn duckdb_driver(path: &Path, query: &str) -> Result<impl SourceDriver, DriverError>;
pub fn postgres_driver(dsn: &str, query: &str) -> Result<impl SourceDriver, DriverError>;
pub fn duckdb_postgres_driver(duckdb_path: &Path, pg_dsn: &str, query: &str) -> Result<impl SourceDriver, DriverError>;
pub fn http_json_driver(url: &str, json_path: Option<&str>) -> Result<impl SourceDriver, DriverError>;
```

**Stream C acceptance gates:**

1. All 6 drivers compile on Rust 1.78 with the pinned versions from Decision 4.
2. All drivers pass per-driver test suites using committed fixture datasets.
3. No driver introduces `async`/`.await` into Mosaic source code.
4. Tokio-path dependency gate test passes (`cargo tree` shows tokio only as transitive of `postgres`/`tokio-postgres`).
5. All drivers honor `cancel()` correctly (cooperative cancellation).

---

## Appendix D: Stream D Interface Contract — Tessera Orchestrator

**This is the binding contract for Stream D (mc-tessera crate).** Depends on Streams A, B, C interfaces being stable.

```rust
// === Public types (mc-tessera crate) ===

pub struct Tessera { /* private */ }

impl Tessera {
    /// Load a recipe, validate it against the target cube, return a prepared import.
    pub fn prepare(recipe_path: &Path) -> Result<PreparedImport, TesseraError>;

    /// Dry-run: validate + plan without writing. Returns the plan + any diagnostics.
    pub fn dry_run(prepared: &PreparedImport) -> Result<DryRunReport, TesseraError>;

    /// Execute: fetch → transform → write_batch → audit.
    pub fn apply(prepared: PreparedImport) -> Result<ImportReport, TesseraError>;

    /// Rollback a previous import by import_id.
    pub fn rollback(model_path: &Path, import_id: &str) -> Result<(), TesseraError>;

    /// List import history for a model.
    pub fn history(model_path: &Path) -> Result<Vec<AuditRecord>, TesseraError>;
}

pub struct PreparedImport {
    pub recipe: Recipe,
    pub cube: Cube,
    pub driver: Box<dyn SourceDriver>,
    pub column_plan: Vec<ResolvedColumnMapping>,
}

pub struct ImportReport {
    pub import_id: String,
    pub rows_written: usize,
    pub rows_failed: usize,
    pub timing: TimingBreakdown,
    pub snapshot_id: String,
    pub audit_path: PathBuf,
}

pub struct TimingBreakdown {
    pub fetch_ms: u64,
    pub transform_ms: u64,
    pub validate_ms: u64,
    pub commit_ms: u64,
    pub total_ms: u64,
}

pub struct AuditRecord {
    pub import_id: String,
    pub recipe_name: String,
    pub source_summary: String,
    pub timestamp: String,
    pub rows_written: usize,
    pub rows_failed: usize,
    pub snapshot_id: String,
}

// === SecretResolver trait (Grout forward-compat) ===

pub trait SecretResolver {
    fn resolve(&self, reference: &str) -> Result<String, SecretError>;
}

pub struct EnvVarSecretResolver;  // resolves ${env.X} from environment
```

**Stream D acceptance gates:**

1. Headline: 100K-row SQLite recipe import completes in ≤ 3 seconds end-to-end.
2. Acme CSV equivalence test: `mc tessera apply acme-import.recipe.yaml` produces byte-identical cube state to `mc_fixtures::write_canonical_inputs()`.
3. Rollback correctness: `mc tessera rollback <import_id>` restores cube state to pre-import snapshot exactly.
4. Audit log: every `apply` produces a valid JSON Lines record at `<model_dir>/.tessera/audit.jsonl`.
5. `mc tessera dry-run <recipe>` exits 0 for valid recipes and non-zero with MC5xxx diagnostics for invalid ones.
6. All 416 existing tests still pass; no regressions in existing benchmarks.

---

## Acceptance amendments

This ADR was Proposed and Accepted on 2026-05-04 with 12 project-owner amendments after parallel reviews from GPT and Claude Desktop. Both reviews converged on the same key gaps (durability model, derived-measure protection, CommitInfo naming, replace semantics, WriteBatch atomicity). The PM verified codebase claims directly before making final calls. Captured here as the audit trail.

| # | Source | Amendment (one-line) | Where it landed |
|---|---|---|---|
| **1** | **GPT (the biggest gap)** | **Add Phase 5A durability/state model — `.tessera/` sidecar under `<model_dir>/` with audit.jsonl, imports/, snapshots/, active-imports.json.** Without a persistence answer, the CLI verbs (`apply`, `history`, `rollback`) were ambiguous. | New Decision 2.5 |
| **2** | **GPT** | **Fix recipe example: remove `Clicks` mapping (Derived measure). Phase 5A writes to Input measures ONLY.** Verified: `Clicks` is `MeasureRole::Derived` in the Acme fixture (line 950 of `mc-fixtures/src/lib.rs`); kernel already rejects via `EngineError::DerivedCellNotWritable`. Tessera catches it at recipe validation. Add MC5018. | Decision 7 example + semantic rule #3 |
| **3** | **GPT** | **Rename `CommitInfo` → `CommitResult`; rename `invalidated_count` → `dirty_count_after` + `newly_dirtied_count`.** Avoids reopening the Phase 2D cumulative-vs-marginal semantic bug. Verified: `WritebackResult.invalidated` means marginal per 5 regression tests in `tests/writeback_invalidated.rs`. | Decision 3 + Appendix A |
| **4** | **GPT** | **Define `write_disposition: replace` as coordinate-level overwrite only.** An import with missing rows does NOT wipe data. Full-slice replace deferred until target-slice semantics exist. | Decision 7 semantic rule #4 |
| **5** | **GPT** | **Tighten WriteBatch atomicity: `new()`/`push()` don't mutate; snapshot captured at `commit()` step 2 (immediately before mutation), not at `new()`.** Saves snapshot-clone cost on batches that are staged but never committed (dry-run flows). | Decision 3 + Appendix A |
| **6** | **GPT** | **Add parallel-stream Cargo governance: PM/integration branch owns root `Cargo.toml` + `Cargo.lock`; streams cannot independently add unapproved deps.** | Decision 2 (new paragraph) |
| **7** | **Desktop** | **Column mappings are 1:1 in Phase 5A.** 1:N (one source column → multiple cube targets) deferred to Phase 5C. Users run multiple recipes against the same source. | Decision 7 semantic rule #1 |
| **8** | **Desktop** | **Defaults vs. columns mutual exclusion.** A dimension cannot appear in both `columns:` and `defaults:`. Add MC5016. | Decision 7 semantic rule #2 |
| **9** | **Desktop** | **Specify `on_error` semantics for `skip_row` and `quarantine`.** Binding behavioral contract before parallel streams build against it. | Decision 7 semantic rule #6 |
| **10** | **Desktop** | **`model:` path resolution relative to recipe file directory.** Matches Phase 3C pattern. Path-escape outside workspace root rejected with MC5017. | Decision 7 semantic rule #5 |
| **11** | **Desktop** | **Add MC5014 (source file not found) + MC5015 (connection failure).** These fire constantly in real use; need typed diagnostics, not generic IO errors. | Appendix B diagnostic table |
| **12** | **Desktop (handoff note)** | **Stream A must commit measured baselines to PERF.md as FIRST commit before any WriteBatch code exists.** Prevents measuring after-the-fact with churned code. | Appendix A gate #8 (handoff note) |

No remaining open questions. Phase 5A stream handoffs draft next.

---

## Alternatives considered

1. **ADBC as the ingestion layer.** Rejected — MSRV 1.85, Go FFI drivers, irrelevant benchmarks. See Decision 4 for full rationale from the May 2026 due-diligence report.

2. **Arrow RecordBatch as the inter-driver data type.** Rejected — MSRV treadmill from arrow-rs quarterly major versions. `RowBatch` with `Vec<Option<T>>` columns is sufficient and decoupled. See Decision 8.

3. **Sequential Phase 5 (one workstream at a time).** Rejected — Phase 5 is 4× larger than any prior phase. Sequential development would take 4× as long. The 4-stream architecture is justified by the clean interface boundaries. See Decision 2.

4. **Skip native Postgres; use DuckDB federation only.** Rejected for Phase 5A (see Decision 5, Path 2). DuckDB-federated Postgres ships as a SEPARATE driver alongside native Postgres; it's a feature, not a fallback.

5. **Bump the Rust toolchain to 1.85 to unlock latest deps.** Rejected for Phase 5A — the pinning strategy (Decision 4) avoids it. A future phase may trigger the bump for unrelated reasons; Phase 5A is NOT that trigger.

6. **Build a Tessera-specific query language (like TM1's TI scripting).** Rejected — the recipe format is declarative YAML, not a programming language. LLMs author declarative formats more reliably than imperative scripts; schema validation catches mapping errors before execution. TI's imperative approach is a historical accident of 1990s design, not a feature.

7. **Use `tokio` explicitly (async source drivers for parallelism).** Rejected — Phase 5A is sync. Rayon for Tier 3 parallelism is a separate ADR-0012 decision. Async would contaminate the entire crate dependency tree and violate the sync-API guarantee.

8. **Build all source drivers in a single crate with `mc-tessera`.** Rejected — separating `mc-drivers` from `mc-tessera` enables Stream C to develop independently of Stream D. The interface boundary (`SourceDriver` trait) is clean and the separation is justified by the parallel development model.

9. **Defer Phase 5 entirely; jump to Phase 6 (UI).** Rejected — UI without data integration means the user manually types every cell. The authoring funnel (Phase 4) + data integration (Phase 5) + UI (Phase 6) is the right order because each phase makes the next one useful.

---

## Cross-links

- [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) — Phase 5 row; updated at this ADR's acceptance.
- [`../CURRENT_STATE.md`](../CURRENT_STATE.md) — Phase status; updated at acceptance.
- [`../strategy/POSITIONING.md`](../strategy/POSITIONING.md) — Mosaic as LNM platform; Tessera is the "data gets in" half of the story.
- [`../process-notes.md`](../process-notes.md) §1 — ADR-first vs handoff-first; Phase 5 is ADR-first.
- [`../process-notes.md`](../process-notes.md) §4 — locked-surfaces rule; Decision 11 is the explicit unlock ADR.
- [`0008-phase-4-llm-authoring-and-plugin-ecosystem.md`](0008-phase-4-llm-authoring-and-plugin-ecosystem.md) — Phase 4 (the plugin that Phase 5B will extend for import authoring).
- [`0009-lnm-substrate-as-product-vision.md`](0009-lnm-substrate-as-product-vision.md) — the three-layer LNM-substrate framing; Tessera is the Layer 2 → Layer 1 data path.
- [`../external-conversations/compass_artifact_wf-0647543e-7e98-4923-ac57-255b2ffb1d86_text_markdown.md`](../external-conversations/compass_artifact_wf-0647543e-7e98-4923-ac57-255b2ffb1d86_text_markdown.md) — the May 2026 due-diligence report underpinning Decisions 4, 5, 6, 8.
- [`../../CLAUDE.md`](../../CLAUDE.md) — project name + naming convention rule; `mc-tessera`, `mc-recipe`, `mc-drivers` follow the `mc-` prefix.
- [`../../crates/mc-core/src/cube.rs`](../../crates/mc-core/src/cube.rs) — the existing `Cube::write()` method that `WriteBatch` builds alongside.

---

## Notes

This ADR is the strategic gate for Phase 5 the way ADR-0008 was for Phase 4. It scopes the Phase 5A umbrella, defines the interface contracts for parallel development, and commits the dependency and performance strategies. Sub-phase ADRs (5B recipe authoring, 5C driver expansion) commit their own implementation contracts when promoted.

If this ADR is amended after Acceptance, the amendment lands in the "Acceptance amendments" section (append-only, same pattern as ADR-0004 through ADR-0008).

**The 7 Postgres crypto-chain pins (Decision 4) are the first time Mosaic has pinned MORE than 3 transitive deps in one phase.** This is a meaningful increase in maintenance surface. When the next Rust toolchain bump occurs (driven by whichever future dep requires it), all 7 pins + the existing 5 (Phase 1B + Phase 3A) must be re-evaluated simultaneously. Document this in the toolchain-bump ADR when it's eventually written.

**The parallel workstream model (Decision 2) is an organizational experiment.** Phase 5A is the first time multiple Claude Code instances develop against the same codebase simultaneously. If the interface-contract + git-worktree + SPEC-QUESTION-for-changes model works, it becomes the template for future multi-stream phases. If it doesn't (interface drift, merge conflicts, coordination overhead > sequential cost), surface the lessons in the Phase 5A completion report and revert to sequential for Phase 5B.

**On the Tier 3 rayon decision (ADR-0012):** this ADR deliberately does NOT commit to `rayon` in Phase 5A. The performance targets (Tiers 1+2, ≤ 5s for 1M cells) are achievable without parallelism. If Tier 1+2 numbers fall short of the target, Tier 3 becomes the follow-up — but with measured evidence, not upfront speculation. ADR-0012 is written when and if Tier 1+2 data justifies it.

**On the "single-binary distribution" aspiration:** the due-diligence report flagged single-binary as an under-recognized moat. Phase 5A's driver choices (rusqlite `bundled`, duckdb `bundled`, postgres native Rust, ureq native Rust, csv native Rust) are all compatible with a single static binary. No system-installed libraries required. The `mc` binary grows from ~5MB to ~30-60MB with bundled SQLite + DuckDB — acceptable for the "zero install steps" distribution story.
