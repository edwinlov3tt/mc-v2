# Phase 5A Stream C Handoff — SourceDriver Trait + Concrete Drivers (`mc-drivers`)

> **Audience:** the Claude Code instance running in a git worktree at
> `../mc-v2-stream-c` on branch `phase-5a/stream-c-source-drivers`.
> **You inherit a green Phase 4B** (commit `b5b6229`, tag
> `phase-4b-python-adapters`; 416/0 tests passing, 10/10 deterministic).
>
> **This stream creates `crates/mc-drivers/` — a new crate containing
> the `SourceDriver` trait, 5 supporting types (`RowBatch`, `Column`,
> `ColumnData`, `ColumnSchema`, `ColumnDataType`), and 6 concrete
> driver implementations (CSV, SQLite, DuckDB, Postgres,
> DuckDB-federated-Postgres, HTTP/JSON).** It is the most
> external-dependency-heavy stream in Phase 5A. Every other existing
> crate (`mc-core`, `mc-fixtures`, `mc-model`, `mc-cli`) is fully
> locked — zero source changes.
>
> **Read [ADR-0010](../decisions/0010-phase-5-tessera-architecture.md)
> BEFORE this handoff.** Focus on Decision 4 (dependency pinning
> matrix), Decision 5 (tokio-transitive-dep Path 2), Decision 8
> (`SourceDriver` trait), and Appendix C (Stream C interface contract).
> Those four sections are the binding contract. This handoff is the
> implementation guide that makes the contract buildable.
>
> **Hard rule:** Stream C creates `crates/mc-drivers/` ONLY. The only
> existing file it modifies is the workspace root `Cargo.toml` (to add
> `mc-drivers` to workspace members + new dependency declarations) —
> but per ADR-0010 amendment #6 (parallel-stream Cargo governance),
> this modification is staged locally in your worktree and flagged for
> PM integration. You do NOT push root `Cargo.toml` / `Cargo.lock`
> changes independently. See the Cargo governance section below.

---

## The one paragraph you must internalize before writing code

**Drivers are commodity; the trait design is the IP.** Get the
`SourceDriver` trait right — simple, synchronous, returning
Mosaic-native `RowBatch` types, forward-compatible with a future
`AdbcDriver` without exposing Arrow in the public API — and the six
concrete drivers are straightforward wrappers around well-documented
Rust crates. Do not over-engineer the drivers. They are reference
implementations proving that the trait works, not production-hardened
connectors. The trait shape (Appendix C of ADR-0010) is frozen; spend
your design energy on schema inference determinism and cancel()
correctness, not on driver-internal cleverness. If a driver's
implementation is more than ~200 lines, you are probably doing too
much.

---

## ADR-0010 amendments affecting Stream C

Two acceptance amendments directly affect this stream:

| # | Amendment | How it shows up in Stream C |
|---|---|---|
| **#6** | **Parallel-stream Cargo governance.** PM / integration branch owns root `Cargo.toml` and `Cargo.lock`. Streams stage locally; final integration through the PM merge branch. | Stream C modifies root `Cargo.toml` in its worktree (adds `mc-drivers` to members + new workspace dep declarations) but does NOT push those changes independently. When Stream C is complete, flag the root `Cargo.toml` diff for PM integration. See the dedicated Cargo governance section below. |
| **#11** | **MC5014 (source-file-not-found) + MC5015 (connection-failure).** These diagnostic codes fire from Stream C drivers. | `DriverError` variants must carry enough context for Stream D / `mc-tessera` to emit MC5014 and MC5015 diagnostic envelopes. Include the path (for file drivers) or DSN/URL (for network drivers) in the error. Stream C does NOT emit MC5xxx codes directly — it returns typed `DriverError` variants; Stream D maps them to diagnostic codes. |

---

## Where Phase 4B ended

- **Phase 4B commit / tag:** `b5b6229` — *phase-4b: python reference adapters* — tag `phase-4b-python-adapters`.
- **Test status:** 416 / 0 passing across all targets. 10/10 deterministic.
- **Toolchain:** Rust 1.78. Existing Cargo.lock pins: Phase 1B (`clap` -> 4.4.18, `clap_lex` -> 0.6.0, `half` -> 2.4.1) + Phase 3A (`indexmap` -> 2.7.0, `hashbrown` -> 0.15.5). **Do not bump.** ADR-0010 is explicit: Phase 5A does NOT trigger a toolchain bump; the Postgres crypto-chain pins avoid it.
- **Workspace members (current):** `mc-core`, `mc-fixtures`, `mc-cli`, `mc-model`.
- **`mc-core`, `mc-fixtures`, `mc-model`, `mc-cli` all fully locked.** Zero source changes permitted by Stream C.

---

## Cargo governance (ADR-0010 amendment #6 — read carefully)

Stream C develops in a git worktree at `../mc-v2-stream-c`. To make
`cargo build` work in that worktree, you MUST modify the workspace
root `Cargo.toml` to add `mc-drivers` to members and declare the new
workspace dependencies. **However, per amendment #6, you do NOT push
these root-file changes to the remote independently.**

**What to do:**

1. In your worktree, modify `Cargo.toml` at the workspace root as
   documented in the "Workspace Cargo.toml modification" section below.
2. Run `cargo build`, `cargo test`, etc. normally in your worktree.
3. When Stream C is complete, include the root `Cargo.toml` diff in
   your completion report with a clear note: "ROOT CARGO.TOML STAGED
   FOR PM INTEGRATION — do not merge independently."
4. The PM merge branch handles final integration of root `Cargo.toml`
   + `Cargo.lock` across Streams A, B, C to avoid conflicts.

**Why this matters:** Streams A, B, and C all add new crates to the
workspace. If each stream pushes root `Cargo.toml` independently,
merge conflicts and `Cargo.lock` divergence are guaranteed. The PM
owns the merge.

---

## Workspace Cargo.toml modification (stage locally)

Add `mc-drivers` to the workspace members list and add new dependency
declarations. The diff against the current root `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = [
    "crates/mc-core",
    "crates/mc-fixtures",
    "crates/mc-cli",
    "crates/mc-model",
    "crates/mc-drivers",     # Phase 5A Stream C
]

[workspace.dependencies]
# === Existing (unchanged) ===
smallvec = { version = "1", features = ["const_generics", "union"] }
ahash = "0.8"
thiserror = "1"
once_cell = "1"
criterion = { version = "0.5", default-features = false }
proptest = "1"
insta = { version = "1", features = ["yaml"] }

# === Phase 5A Stream C additions ===
csv = "1"
rusqlite = { version = "=0.31.0", features = ["bundled"] }
duckdb = { version = "=1.3.2", features = ["bundled"] }
postgres = "=0.19.9"
ureq = "2"
```

**Note:** The 7 RustCrypto transitive pins (`postgres-protocol`,
`sha2`, `hmac`, `md-5`, `digest`, `block-buffer`, and
`crypto-common` if needed) are enforced via `Cargo.lock`, NOT via
`[workspace.dependencies]` declarations. They land in `Cargo.lock`
automatically when you pin `postgres = "=0.19.9"` and the resolver
picks the correct transitive versions. If the resolver picks wrong
versions (e.g., `block-buffer 0.12.0` which requires `edition2024`),
you must add `[patch]` entries or use `cargo update -p <crate>
--precise <version>` to force the correct pins. The expected
transitive pin set (from ADR-0010 Decision 4):

| Transitive crate | Required pin | Why |
|---|---|---|
| `postgres-protocol` | `=0.6.7` | Last version using pre-edition2024 RustCrypto chain |
| `sha2` | `=0.10.8` | Pre-edition2024; 0.11.0 pulls digest 0.11.3 -> block-buffer 0.12.0 |
| `hmac` | `=0.12.1` | Pre-edition2024; 0.13.0 pulls digest 0.11.3 |
| `md-5` | `=0.10.6` | Pre-edition2024; 0.11.0 pulls digest 0.11.3 |
| `digest` | `=0.10.7` | Pre-edition2024 |
| `block-buffer` | `=0.10.4` | The actual blocker; 0.12.0 requires edition2024 Cargo feature |

**Verification command after pinning:**
```bash
cargo tree -p postgres -i block-buffer
# Must show block-buffer 0.10.x, NOT 0.12.x
# If 0.12.x appears, the build WILL fail on Rust 1.78.
```

---

## `crates/mc-drivers/Cargo.toml` (paste this directly)

```toml
[package]
name = "mc-drivers"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
description = "Mosaic source drivers — SourceDriver trait + CSV/SQLite/DuckDB/Postgres/HTTP-JSON reference implementations (Phase 5A Stream C)."

[dependencies]
csv = { workspace = true }
rusqlite = { workspace = true }
duckdb = { workspace = true }
postgres = { workspace = true }
ureq = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
# No dev-deps beyond std for Phase 5A Stream C.
# Test fixtures use in-memory databases and inline CSV.

[features]
# The dependency-gate test is gated behind this feature so it doesn't
# slow every `cargo test` invocation. Run explicitly:
#   cargo test -p mc-drivers --features dependency-gate -- dependency_gate
dependency-gate = []
```

---

## Stream C prompt (verbatim — this is your contract)

> We are starting Mosaic Phase 5A Stream C: SourceDriver trait +
> concrete driver implementations in `crates/mc-drivers/`.
>
> **Context.** Phase 5A introduces Tessera, Mosaic's data ingestion
> engine. Stream C is one of four parallel streams. It owns the
> `SourceDriver` trait (the abstraction every data source implements)
> and 6 reference driver implementations. Streams A (WriteBatch in
> mc-core), B (recipe format in mc-recipe), and D (Tessera
> orchestrator in mc-tessera) develop in parallel against frozen
> interface contracts defined in ADR-0010.
>
> **Goal.** Ship a complete, tested `crates/mc-drivers/` crate such
> that:
>
> 1. The `SourceDriver` trait matches Appendix C of ADR-0010 exactly
>    (signature-identical; no additions, no omissions, no renames).
> 2. All 6 drivers compile on Rust 1.78 with the pinned dependency
>    versions from Decision 4.
> 3. All drivers are fully synchronous — no `async`, no `.await`, no
>    `use tokio::*` anywhere in `crates/mc-drivers/`.
> 4. All drivers pass per-driver test suites using committed fixture
>    datasets (in-memory DBs, small CSV files, in-process test
>    servers).
> 5. The tokio dependency-gate test passes (tokio appears in the dep
>    tree only as a transitive of `postgres` / `tokio-postgres`).
> 6. Schema inference is deterministic and documented per driver.
>
> **Phase 5A Stream C scope (binding contract):**
>
> 1. **New crate `crates/mc-drivers/`** with Cargo.toml as specified
>    in the handoff (paste directly; all dep versions + features
>    pre-configured).
>
> 2. **The `SourceDriver` trait** matching Appendix C signatures
>    EXACTLY:
>    ```rust
>    pub trait SourceDriver {
>        fn schema(&self) -> Result<Vec<ColumnSchema>, DriverError>;
>        fn fetch_batch(&mut self, max_rows: usize) -> Result<Option<RowBatch>, DriverError>;
>        fn cancel(&mut self);
>    }
>    ```
>
> 3. **Supporting types** (all public, all `#[derive(Debug)]`):
>    - `RowBatch` — `columns: Vec<Column>`, `row_count: usize`
>    - `Column` — `name: String`, `data: ColumnData`
>    - `ColumnData` — enum: `F64(Vec<Option<f64>>)`,
>      `I64(Vec<Option<i64>>)`, `Str(Vec<Option<String>>)`,
>      `Bool(Vec<Option<bool>>)`
>    - `ColumnSchema` — `name: String`, `data_type: ColumnDataType`,
>      `nullable: bool`
>    - `ColumnDataType` — enum: `F64`, `I64`, `Str`, `Bool`
>    - `DriverError` — `thiserror`-derived error enum with variants
>      carrying enough context for MC5014 / MC5015 diagnostic
>      mapping (include file paths, DSNs, query text in error
>      variants as appropriate)
>
> 4. **6 concrete drivers with constructor functions matching
>    Appendix C:**
>    - `pub fn csv_driver(path: &Path) -> Result<impl SourceDriver, DriverError>`
>    - `pub fn sqlite_driver(path: &Path, query: &str) -> Result<impl SourceDriver, DriverError>`
>    - `pub fn duckdb_driver(path: &Path, query: &str) -> Result<impl SourceDriver, DriverError>`
>    - `pub fn postgres_driver(dsn: &str, query: &str) -> Result<impl SourceDriver, DriverError>`
>    - `pub fn duckdb_postgres_driver(duckdb_path: &Path, pg_dsn: &str, query: &str) -> Result<impl SourceDriver, DriverError>`
>    - `pub fn http_json_driver(url: &str, json_path: Option<&str>) -> Result<impl SourceDriver, DriverError>`
>
> 5. **Per-driver test fixtures:**
>    - CSV: small `.csv` files committed under
>      `crates/mc-drivers/tests/fixtures/`
>    - SQLite: in-memory database created in test setup (NOT a
>      committed .db file — `rusqlite::Connection::open_in_memory()`)
>    - DuckDB: in-memory database created in test setup
>      (`duckdb::Connection::open_in_memory()`)
>    - Postgres: tests gated behind `#[cfg(feature = "postgres-live")]`
>      or `#[ignore]` with a doc comment explaining the test requires
>      a running Postgres instance. Include a
>      `tests/fixtures/postgres_setup.sql` script for manual setup.
>    - DuckDB-federated-Postgres: same gating as Postgres tests.
>    - HTTP/JSON: in-process test server using `std::net::TcpListener`
>      (no external deps; raw HTTP response writing is fine for a test
>      fixture). OR use a committed `.json` fixture file and mock the
>      HTTP layer at the driver level.
>
> 6. **Tokio dependency-gate test** at
>    `crates/mc-drivers/tests/dependency_gate.rs`:
>    - Gated behind `#[cfg(feature = "dependency-gate")]` so it does
>      not slow every `cargo test` invocation.
>    - Runs `cargo tree -p mc-drivers` as a subprocess.
>    - Asserts that `tokio` appears ONLY as a transitive dependency
>      of `postgres` / `tokio-postgres`.
>    - If tokio appears via any other path, the test fails.
>
> 7. **All drivers are SYNC.** No `async` keyword. No `.await`. No
>    `use tokio::*`. The `postgres` crate (not `tokio-postgres`) is
>    the direct dependency; it provides a synchronous API wrapping
>    `tokio-postgres` internally. Stream C source code never sees
>    async.
>
> 8. **All drivers honor `cancel()` correctly.** After `cancel()` is
>    called, the next `fetch_batch()` returns `Ok(None)` (exhausted).
>    This is cooperative cancellation — the driver checks a
>    `cancelled: bool` flag at the top of `fetch_batch()`.
>
> 9. **Schema inference is deterministic and documented per driver:**
>    - CSV: infer from first N rows (configurable; default 100).
>      All-numeric columns -> F64; columns with any non-numeric -> Str.
>      Document the inference rule in a `///` doc comment on
>      `csv_driver`.
>    - SQLite: use `sqlite3_column_type()` / column declaration
>      types from the prepared statement.
>    - DuckDB: use DuckDB's column type metadata from the prepared
>      statement.
>    - Postgres: use `postgres::types::Type` from the row description.
>    - DuckDB-Postgres: same as DuckDB (the federation query runs
>      through DuckDB's engine).
>    - HTTP/JSON: infer from first batch. Nested objects -> Str
>      (JSON-serialized). Arrays -> Str. Scalars -> their natural
>      type (number -> F64, bool -> Bool, string -> Str, null ->
>      nullable Str).
>
> 10. **The DuckDB-federated-Postgres path (`duckdb_postgres`
>     driver) is a first-class driver, not a fallback.** It exists
>     for legitimate cross-database join use cases where DuckDB
>     mediates between a local DuckDB database and a remote Postgres
>     via DuckDB's `postgres_scanner` extension. It is NOT a
>     substitute for the native `postgres` driver.
>
> **Hard rules:**
>
> - No `async` keyword anywhere in `crates/mc-drivers/`.
> - No `.await` anywhere in `crates/mc-drivers/`.
> - No `use tokio::*` anywhere in `crates/mc-drivers/`.
> - `postgres` pinned at `=0.19.9`. The 7 RustCrypto transitive
>   deps pinned in `Cargo.lock` per the handoff's pinning table.
> - `rusqlite` pinned at `=0.31.0` with `features = ["bundled"]`.
> - `duckdb` pinned at `=1.3.2` with `features = ["bundled"]`.
> - `bundled` features for both SQLite and DuckDB — zero
>   system-library dependencies. The single-binary distribution
>   story requires no pre-installed `libsqlite3` or `libduckdb`.
> - `mc-core` untouched. `mc-model` untouched. `mc-fixtures`
>   untouched. `mc-cli` untouched. `git diff phase-4b-python-adapters
>   -- crates/mc-core/ crates/mc-fixtures/ crates/mc-model/ crates/mc-cli/`
>   returns zero lines.
> - No `unsafe` in `crates/mc-drivers/`.
> - No `unwrap()` in `crates/mc-drivers/src/`. Tests may use
>   `expect("static reason")`.
> - No `println!` / `eprintln!` / `dbg!` in `crates/mc-drivers/src/`.
> - Every public type has a `///` doc comment.
> - Every public type has `#[derive(Debug)]`.
> - Root `Cargo.toml` / `Cargo.lock` changes staged locally only;
>   flagged for PM integration per amendment #6.
>
> **SPEC QUESTION triggers:**
>
> Open a SPEC QUESTION (per CLAUDE.md section 11) before continuing if:
>
> 1. A dependency does not build on Rust 1.78 with the pinned
>    version. The pinning matrix in Decision 4 was verified via
>    scratch test; if it breaks, something changed upstream.
>    Document the exact error and the `cargo tree` output.
> 2. DuckDB `bundled` feature produces a binary exceeding 100 MB
>    (indicating unexpected static-link bloat). Document the size.
> 3. The `postgres` crate's sync wrapper requires an async
>    handshake that cannot be completed without explicitly spinning
>    up a tokio runtime in driver code. The `postgres` crate
>    internally manages its own runtime; if that internal runtime
>    fails on 1.78, document the error.
> 4. A driver needs an additional dependency not listed in Decision
>    4's pinning matrix. Do NOT add it unilaterally — open a SPEC
>    QUESTION with the crate name, version, why it is needed, and
>    whether it builds on 1.78.
> 5. The DuckDB `postgres_scanner` extension is not available in
>    the bundled build (it may require a separate extension load).
>    Document what the `duckdb_postgres` driver needs at runtime.
> 6. `csv` crate version 1.x requires a Rust edition or feature
>    not available on 1.78. (Extremely unlikely — `csv` is pure
>    Rust and conservative, but check.)
> 7. The `ureq` crate pulls unexpected transitive deps that
>    conflict with the existing Cargo.lock pins.
>
> **Acceptance gates (from Appendix C):**
>
> 1. All 6 drivers compile on Rust 1.78 with the pinned versions.
> 2. All drivers pass per-driver test suites using committed
>    fixture datasets.
> 3. No driver introduces `async` / `.await` into Mosaic source.
> 4. Tokio-path dependency gate test passes.
> 5. All drivers honor `cancel()` correctly.
> 6. All 416 existing tests still pass (the new crate adds tests;
>    existing tests are unaffected).
> 7. `cargo fmt --check --all` exits 0.
> 8. `cargo clippy --workspace --all-targets -- -D warnings` exits 0.

---

## Context you need (so you do not waste time rediscovering it)

### The ADBC rejection rationale

ADR-0010 Decision 4 rejected ADBC for Phase 5A. The reasons (from the
May 2026 due-diligence report):

1. `arrow-rs` MSRV is 1.85; Mosaic is pinned at 1.78. No compatible
   arrow-rs version exists that receives security backports.
2. Non-SQLite ADBC drivers are Go FFI shims (`adbc_snowflake` wraps a
   Go-built dynamic library; `adbc_clickhouse` has MSRV 1.91).
3. ADBC's "10-100x faster" benchmarks measure warehouse-to-warehouse
   pulls, not 100K-row recipe imports.

**Do not reconsider ADBC.** The `SourceDriver` trait is designed to
admit a future `AdbcDriver` implementation (Arrow -> RowBatch
conversion) without public API changes. That is a Phase 6+ concern.

### Why RowBatch, not Arrow RecordBatch

- `arrow-rs` MSRV 1.85 (incompatible).
- `arrow-rs` ships quarterly major-version bumps; exposing Arrow
  types in a public API creates an MSRV treadmill.
- Mosaic's data model is vastly simpler than Arrow's columnar format.
  `Vec<Option<f64>>` per column is sufficient.
- The trait is forward-compatible: a future ADBC driver converts
  Arrow -> RowBatch internally without changing the public API.

### The toolchain gate result (Postgres crypto chain)

The `postgres = "=0.19.9"` pin was chosen because `postgres 0.19.13`
(latest at time of ADR) pulls `postgres-protocol 0.6.11`, which pulls
RustCrypto 0.11/0.13 chain, which pulls `block-buffer 0.12.0`
requiring `edition2024` — a Cargo feature not available on Rust 1.78.

Pinning to `postgres = "=0.19.9"` + the 6 transitive RustCrypto pins
keeps the entire chain on pre-edition2024 crates. This was verified
in the ADR-0010 toolchain gate (2026-05-04). The 7 pins are:

```
postgres-protocol = 0.6.7
sha2 = 0.10.8
hmac = 0.12.1
md-5 = 0.10.6
digest = 0.10.7
block-buffer = 0.10.4
```

If `cargo build` fails with an `edition2024` error from any of these,
the resolver picked a wrong version. Use `cargo update -p <crate>
--precise <version>` to force the correct pin, then verify with
`cargo tree -p postgres -i block-buffer`.

### DuckDB ICU limitation

The `duckdb = "=1.3.2"` bundled build does NOT include the ICU
extension. This means DuckDB's date/time arithmetic functions that
depend on locale-aware formatting (e.g., `strftime` with locale
specifiers, `monthname()`) may not work. This is a known limitation
of the bundled build at this version.

**Document this limitation, do not try to fix it.** The `duckdb`
driver's doc comment should note: "Bundled DuckDB 1.3.2 does not
include the ICU extension. Locale-dependent date formatting is
unavailable. Use ISO 8601 date formats in queries."

### Single-binary distribution aspiration

All five driver dependencies are compatible with static compilation
into a single binary:

- `rusqlite` `bundled` — statically links `libsqlite3`.
- `duckdb` `bundled` — statically links `libduckdb`.
- `postgres` — pure Rust (internally wraps `tokio-postgres` which is
  pure Rust).
- `ureq` — pure Rust HTTP client.
- `csv` — pure Rust.

No system-installed libraries required. The `mc` binary grows from
~5 MB to ~30-60 MB with bundled SQLite + DuckDB. This is acceptable
for the "zero install steps" distribution story.

---

## Suggested module structure

```
crates/mc-drivers/
    Cargo.toml
    src/
        lib.rs              # pub mod + re-exports; SourceDriver trait; RowBatch/Column/ColumnData/ColumnSchema/ColumnDataType; DriverError
        csv_driver.rs       # CsvDriver struct + csv_driver() constructor
        sqlite_driver.rs    # SqliteDriver struct + sqlite_driver() constructor
        duckdb_driver.rs    # DuckDbDriver struct + duckdb_driver() constructor
        postgres_driver.rs  # PostgresDriver struct + postgres_driver() constructor
        duckdb_postgres_driver.rs  # DuckdbPostgresDriver struct + duckdb_postgres_driver() constructor
        http_json_driver.rs # HttpJsonDriver struct + http_json_driver() constructor
    tests/
        dependency_gate.rs  # tokio-path assertion; gated behind "dependency-gate" feature
        csv_tests.rs        # CSV driver tests using fixtures/
        sqlite_tests.rs     # SQLite driver tests (in-memory)
        duckdb_tests.rs     # DuckDB driver tests (in-memory)
        postgres_tests.rs   # Postgres tests (gated behind feature or #[ignore])
        duckdb_postgres_tests.rs  # DuckDB-Postgres tests (gated)
        http_json_tests.rs  # HTTP/JSON tests (in-process server or mock)
        fixtures/
            sample.csv
            sample_with_nulls.csv
            sample_types.csv
            postgres_setup.sql
```

You may reorganize the test layout if integration tests in `tests/`
are more natural than unit tests in `src/`. The public API surface
(trait + types + constructors) is the contract; internal module
boundaries are your call.

---

## Reproducible commands

```bash
# Working directory: the stream-c worktree
cd /Users/edwinlovettiii/Projects/mc-v2-stream-c

source $HOME/.cargo/env

# After creating crates/mc-drivers/ and modifying root Cargo.toml:

# Build gate
cargo build --release --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings

# Test gate (all 416 existing + new mc-drivers tests)
cargo test --workspace

# Verify no async contamination in mc-drivers source
grep -rn "async\|\.await\|use tokio" crates/mc-drivers/src/
# expected: zero matches

# Verify no unwrap in mc-drivers/src/
grep -rn "\.unwrap()\|\.expect(" crates/mc-drivers/src/
# expected: zero matches

# Verify no println/dbg in mc-drivers/src/
grep -rn "println!\|eprintln!\|dbg!" crates/mc-drivers/src/
# expected: zero matches

# Verify locked surfaces
git diff phase-4b-python-adapters -- crates/mc-core/ crates/mc-fixtures/ crates/mc-model/ crates/mc-cli/
# expected: zero output

# Verify the Postgres crypto chain pins
cargo tree -p mc-drivers -i block-buffer 2>/dev/null
# must show block-buffer 0.10.x, NOT 0.12.x

# Run the tokio dependency-gate test explicitly
cargo test -p mc-drivers --features dependency-gate -- dependency_gate

# Determinism gate
for i in $(seq 1 10); do cargo test --workspace -q || echo "FAIL run $i"; done

# Verify toolchain is still 1.78
rustc --version
# expected: rustc 1.78.x
```

---

## Final checklist before you call Stream C done

- [ ] `crates/mc-drivers/` exists with Cargo.toml matching the handoff specification.
- [ ] `SourceDriver` trait matches Appendix C of ADR-0010 signature-for-signature.
- [ ] `RowBatch`, `Column`, `ColumnData`, `ColumnSchema`, `ColumnDataType` types match Appendix C.
- [ ] `DriverError` is `thiserror`-derived with variants carrying path/DSN/URL context for MC5014/MC5015 mapping.
- [ ] All 6 constructor functions exist with signatures matching Appendix C.
- [ ] All 6 drivers compile on Rust 1.78.
- [ ] CSV driver: tested with committed fixture CSV files; schema inference documented.
- [ ] SQLite driver: tested with in-memory database; schema inference documented.
- [ ] DuckDB driver: tested with in-memory database; ICU limitation documented; schema inference documented.
- [ ] Postgres driver: test exists (gated behind feature flag or `#[ignore]`); schema inference documented.
- [ ] DuckDB-Postgres driver: test exists (gated); documented as first-class driver, not fallback.
- [ ] HTTP/JSON driver: tested with in-process fixture or mock; schema inference documented.
- [ ] All drivers honor `cancel()` correctly (test per driver).
- [ ] Tokio dependency-gate test passes (`cargo test -p mc-drivers --features dependency-gate`).
- [ ] No `async` / `.await` / `use tokio::*` anywhere in `crates/mc-drivers/src/`.
- [ ] No `unwrap()` / `expect()` in `crates/mc-drivers/src/`. Tests may use `expect("static reason")`.
- [ ] No `println!` / `eprintln!` / `dbg!` in `crates/mc-drivers/src/`.
- [ ] No `unsafe` in `crates/mc-drivers/`.
- [ ] Every public type has `///` doc comment + `#[derive(Debug)]`.
- [ ] `cargo fmt --check --all` exits 0.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` exits 0.
- [ ] `cargo build --release --workspace` zero warnings.
- [ ] `cargo test --workspace` passes (416 existing + Stream C additions).
- [ ] 10 consecutive `cargo test --workspace -q` runs identical.
- [ ] `block-buffer` in `Cargo.lock` is 0.10.x, NOT 0.12.x.
- [ ] Locked surfaces: `git diff phase-4b-python-adapters -- crates/mc-core/ crates/mc-fixtures/ crates/mc-model/ crates/mc-cli/` returns zero lines.
- [ ] Toolchain: `rust-toolchain.toml` unchanged. Rust 1.78.
- [ ] Root `Cargo.toml` diff documented in completion report with "STAGED FOR PM INTEGRATION" note.
- [ ] **You did NOT push root Cargo.toml / Cargo.lock changes independently** (amendment #6).
- [ ] **You did NOT start Stream D, modify mc-tessera, or write recipe parsing code.**
- [ ] **You did NOT add any dependency not listed in Decision 4's pinning matrix.**
- [ ] Completion report at `docs/reports/phase-5a-stream-c-completion-report.md`.

---

## Completion report format

```
DONE: Phase 5A Stream C — SourceDriver Trait + Concrete Drivers

Build:    cargo build --release --workspace                       [zero warnings]
Format:   cargo fmt --check --all                                 [exit 0]
Lint:     cargo clippy --workspace --all-targets -- -D warnings   [exit 0]
Tests:    cargo test --workspace                                  [N] / 0 (was 416 / 0)
Determinism: 10 / 10 identical

Locked surfaces:
  mc-core      0-line diff vs phase-4b-python-adapters
  mc-fixtures  0-line diff
  mc-model     0-line diff
  mc-cli       0-line diff

Dependency verification:
  block-buffer in Cargo.lock: 0.10.4 (NOT 0.12.x)
  tokio dep-gate test: PASS (tokio only via postgres -> tokio-postgres)

ROOT CARGO.TOML STAGED FOR PM INTEGRATION:
  <include full diff here>

Source manifest:
  crates/mc-drivers/Cargo.toml                    (NEW)
  crates/mc-drivers/src/lib.rs                    (NEW)
  crates/mc-drivers/src/csv_driver.rs             (NEW)
  crates/mc-drivers/src/sqlite_driver.rs          (NEW)
  crates/mc-drivers/src/duckdb_driver.rs          (NEW)
  crates/mc-drivers/src/postgres_driver.rs        (NEW)
  crates/mc-drivers/src/duckdb_postgres_driver.rs (NEW)
  crates/mc-drivers/src/http_json_driver.rs       (NEW)
  crates/mc-drivers/tests/dependency_gate.rs      (NEW)
  crates/mc-drivers/tests/csv_tests.rs            (NEW)
  crates/mc-drivers/tests/sqlite_tests.rs         (NEW)
  crates/mc-drivers/tests/duckdb_tests.rs         (NEW)
  crates/mc-drivers/tests/postgres_tests.rs       (NEW)
  crates/mc-drivers/tests/duckdb_postgres_tests.rs (NEW)
  crates/mc-drivers/tests/http_json_tests.rs      (NEW)
  crates/mc-drivers/tests/fixtures/               (NEW — CSV fixtures)
  Cargo.toml                                      (MODIFIED — staged for PM integration)
  Cargo.lock                                      (MODIFIED — staged for PM integration)

Deviations:
  - <list any; ideally empty>
```

Do NOT commit or tag. The user reviews first.

---

## Resolution order when uncertain

1. **ADR-0010** — Decisions 4, 5, 8; Appendix C. The binding contract.
2. This handoff document.
3. The due-diligence report at `docs/external-conversations/compass_artifact_wf-0647543e-7e98-4923-ac57-255b2ffb1d86_text_markdown.md` (section 11 scratch test for exact dep configurations).
4. `CLAUDE.md` — operating manual (especially sections 2.3, 2.14, 3.1, 3.2, 6, 11).
5. The existing workspace `Cargo.toml` and `Cargo.lock` for pin precedent.
6. Anything else.

If those do not resolve it: stop, write a SPEC QUESTION per CLAUDE.md section 11, and wait. Do not guess.

---

*Phase 5A Stream C handoff drafted 2026-05-04, immediately after ADR-0010 was Accepted with 12 amendments. Stream C is the most dependency-heavy of the four parallel streams; the pinning matrix and Cargo governance rules exist specifically to contain that complexity. The trait is the IP; the drivers are commodity. Get the trait right.*
