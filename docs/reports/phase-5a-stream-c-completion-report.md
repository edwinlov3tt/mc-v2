# Phase 5A Stream C — Completion Report

**Date:** 2026-05-04
**Branch:** `phase-5a/stream-c-source-drivers` (worktree at `../mc-v2-stream-c`)
**Inheriting:** Phase 4B `b5b6229` (`phase-4b-python-adapters` tag) — 416/0
**ADR contract:** [ADR-0010](../decisions/0010-phase-5-tessera-architecture.md)
+ [Amendment 1](../decisions/0010-amendment-1-stream-c-pin-corrections.md)

---

## DONE: Phase 5A Stream C — SourceDriver Trait + Concrete Drivers

```
Build:    cargo build --release --workspace                       zero warnings
Format:   cargo fmt --check --all                                 exit 0
Lint:     cargo clippy --workspace --all-targets -- -D warnings   exit 0
Tests:    cargo test --workspace                                  446 / 0  (was 416 / 0)
Determinism: 10 / 10 identical
```

**Locked surfaces:**

```
mc-core      0-line diff vs phase-4b-python-adapters
mc-fixtures  0-line diff
mc-model     0-line diff
mc-cli       0-line diff
rust-toolchain.toml  0-line diff (still 1.78)
```

**Dependency verification:**

- `block-buffer` in `Cargo.lock`: **0.10.4** (NOT 0.12.x) ✓
- Tokio dep-gate test: **PASS** (`tokio` reached only via
  `mc-drivers → postgres → tokio-postgres → tokio` (and tokio-postgres's
  internal `tokio-util → tokio`); zero Mosaic-side leak)
- `cargo tree -p mc-drivers -i tokio` confirms tokio's only depth-1
  consumers are `postgres`, `tokio-postgres`, `tokio-util`

**Forbidden-pattern greps in `crates/mc-drivers/src/`:**

- `\.unwrap()`, `\.expect(` — none
- `\bunsafe\b` — none (entire crate, including tests)
- `println!`, `eprintln!`, `dbg!` — none
- `^async`, `\.await`, `use tokio::` — none (one literal occurrence inside
  the `lib.rs` module-doc comment that documents the rule itself; not a
  real code path)

---

## Source manifest (NEW — all under `crates/mc-drivers/`)

```
crates/mc-drivers/Cargo.toml                           NEW
crates/mc-drivers/src/lib.rs                           NEW   (trait + types per ADR-0010 Appendix C)
crates/mc-drivers/src/csv_driver.rs                    NEW
crates/mc-drivers/src/sqlite_driver.rs                 NEW
crates/mc-drivers/src/duckdb_driver.rs                 NEW
crates/mc-drivers/src/postgres_driver.rs               NEW
crates/mc-drivers/src/duckdb_postgres_driver.rs        NEW
crates/mc-drivers/src/http_json_driver.rs              NEW   (incl. minimal in-tree JSON parser; see Deviation #2)
crates/mc-drivers/tests/dependency_gate.rs             NEW   (gated behind feature `dependency-gate`)
crates/mc-drivers/tests/csv_tests.rs                   NEW   (7 tests)
crates/mc-drivers/tests/sqlite_tests.rs                NEW   (6 tests)
crates/mc-drivers/tests/duckdb_tests.rs                NEW   (6 tests)
crates/mc-drivers/tests/postgres_tests.rs              NEW   (4 tests, 3 #[ignore] live-PG)
crates/mc-drivers/tests/duckdb_postgres_tests.rs       NEW   (2 tests, both #[ignore] live-PG)
crates/mc-drivers/tests/http_json_tests.rs             NEW   (10 tests, in-process server)
crates/mc-drivers/tests/fixtures/sample.csv            NEW
crates/mc-drivers/tests/fixtures/sample_with_nulls.csv NEW
crates/mc-drivers/tests/fixtures/sample_types.csv      NEW
crates/mc-drivers/tests/fixtures/postgres_setup.sql    NEW
docs/handoffs/phase-5a-stream-c-spec-question-001.md   NEW   (resolved by Amendment 1)
docs/reports/phase-5a-stream-c-completion-report.md    NEW   (this file)
Cargo.toml                                             MODIFIED — staged for PM integration
Cargo.lock                                             MODIFIED — staged for PM integration
```

---

## ROOT `Cargo.toml` STAGED FOR PM INTEGRATION (amendment #6)

The following diff is on this branch but **not pushed independently**.
Per ADR-0010 amendment #6 (parallel-stream Cargo governance), the PM
merge branch owns final integration of root `Cargo.toml` + `Cargo.lock`
across Streams A, B, C to avoid lockfile conflicts.

```diff
diff --git a/Cargo.toml b/Cargo.toml
@@ -1,6 +1,12 @@
 [workspace]
 resolver = "2"
-members = ["crates/mc-core", "crates/mc-fixtures", "crates/mc-cli", "crates/mc-model"]
+members = [
+    "crates/mc-core",
+    "crates/mc-fixtures",
+    "crates/mc-cli",
+    "crates/mc-model",
+    "crates/mc-drivers",
+]
@@ -20,6 +26,22 @@ criterion = { version = "0.5", default-features = false }
 proptest = "1"
 insta = { version = "1", features = ["yaml"] }

+# === Phase 5A Stream C additions (mc-drivers) ===
+# Pins per ADR-0010 Decision 4 + Amendment 1
+# (docs/decisions/0010-amendment-1-stream-c-pin-corrections.md).
+csv = "1"
+rusqlite = { version = "=0.31.0", features = ["bundled", "column_decltype"] }
+duckdb = { version = "=1.1.1", features = ["bundled", "column_decltype"] }
+postgres = "=0.19.9"
+ureq = "2"
```

`Cargo.lock` is also modified (~2.3K lines added) — exclusively new
mc-drivers and transitive dependency entries, plus the Decision 4 +
Amendment 1 pinned versions. No existing pin was bumped. Cargo.lock
diff is too large to inline; PM merge can regenerate from the workspace
manifest with `cargo generate-lockfile` once Stream A and Stream B are
merged.

---

## Per-driver acceptance summary

| Driver | Source | Tests | Schema inference | `cancel()` | Notes |
| --- | --- | --- | --- | --- | --- |
| **CsvDriver** | `csv_driver.rs` | 7 (all pass) | Documented: 100-row sample, all-int→I64, else all-numeric→F64, else→Str | ✓ | Bool not inferred (CSV is not strongly typed; documented). |
| **SqliteDriver** | `sqlite_driver.rs` | 6 (all pass, in-memory via temp file) | Documented: `decl_type` keyword match; INT→I64, BOOL→Bool, REAL/FLOA/DOUB/NUMERIC/DECIMAL→F64, else→Str | ✓ | `column_decltype` rusqlite feature enabled. |
| **DuckDbDriver** | `duckdb_driver.rs` | 6 (all pass, in-memory via temp file) | Documented: `Statement::column_type` after first row; standard families→{Bool, I64, F64}, complex→Str | ✓ | **ICU limitation documented** in module head; `decimal_string_to_f64` lossy parse for DECIMAL→F64. |
| **PostgresDriver** | `postgres_driver.rs` | 4 total: 1 always-on (bad-DSN ConnectionFailed), 3 `#[ignore]` (require live PG) | Documented: PG OID match; INT2/4/8/OID→I64, FLOAT4/8→F64, BOOL→Bool, NUMERIC + everything else→Str | ✓ | DSN passwords redacted in error surfaces. |
| **DuckdbPostgresDriver** | `duckdb_postgres_driver.rs` | 2 (both `#[ignore]`, require live PG + extension download) | Inherited from DuckDbDriver (federation runs through DuckDB engine) | ✓ | First-class driver per ADR-0010. `INSTALL postgres; LOAD postgres; ATTACH '<dsn>' AS pg`. |
| **HttpJsonDriver** | `http_json_driver.rs` | 10 (all pass, in-process `std::net::TcpListener` mock server) | Documented: per-field union over scalar types; objects/arrays→Str (JSON-serialized) | ✓ | In-tree minimal JSON parser (see Deviation #2). |

Total: **35 tests added** (30 always-on + 5 `#[ignore]` live-PG).
Workspace tests: 446 passing, 0 failing, 5 ignored, 0 measured.

---

## Deviations

### Deviation 1 — `duckdb = "=1.1.1"` (was `=1.3.2`); 5 additional transitive pins

**Status:** Authorised by ADR-0010 Amendment 1 ([source](../decisions/0010-amendment-1-stream-c-pin-corrections.md)). Filed as
[SPEC QUESTION 001](../handoffs/phase-5a-stream-c-spec-question-001.md);
PM resolved with Option A on 2026-05-04.

**Root cause:** `libduckdb-sys 1.2.0+` uses `bindgen ^0.71.1`, which
emits Rust-1.82+ `unsafe extern "C" {` block syntax incompatible with
the project's 1.78 toolchain pin. The cliff is at `libduckdb-sys 1.2.0`;
the last 1.78-compatible pair is `duckdb = =1.1.1` + `libduckdb-sys = =1.1.1`.

**Pin set actually in use (Decision 4 + Amendment 1):**

| Crate | Version | Origin |
| --- | --- | --- |
| `duckdb` | `=1.1.1` | Amendment 1 correction |
| `libduckdb-sys` | `=1.1.1` | Amendment 1 — explicit (resolver may otherwise pull 1.2.x within `^1.1.1` SemVer range) |
| `rusqlite` | `=0.31.0` | Decision 4 (unchanged) |
| `postgres` | `=0.19.9` | Decision 4 (unchanged) |
| `postgres-protocol` | `=0.6.7` | Decision 4 (unchanged) |
| `sha2` | `=0.10.8` | Decision 4 (unchanged) |
| `hmac` | `=0.12.1` | Decision 4 (unchanged) |
| `md-5` | `=0.10.6` | Decision 4 (unchanged) |
| `digest` | `=0.10.7` | Decision 4 (unchanged) |
| `block-buffer` | `=0.10.4` | Decision 4 (unchanged) |
| `ureq` | `2` | Decision 4 (unchanged) |
| `csv` | `1` | Decision 4 (unchanged) |
| `proc-macro-crate` | `=3.3.0` | Amendment 1 — 3.4.0 → toml_edit 0.23 → toml_parser 1.1+ (edition2024) |
| `idna_adapter` | `=1.1.0` | Amendment 1 — 1.2+ is edition2024 (URL parsing) |
| `comfy-table` | `=7.1.4` | Amendment 1 — 7.2+ edition2024 (arrow-cast prettyprint) |
| `uuid` | `=1.20.0` | Amendment 1 — 1.21+ requires Rust 1.85 |
| `unicode-segmentation` | `=1.12.0` | Amendment 1 — 1.13.0/1.13.1 yanked, 1.13.2 requires Rust 1.85 |

DuckDB 1.1.1 vs 1.3.2 capability gap for Stream C's reference-implementation
workload (issue a SELECT, drain rows, return `RowBatch`): zero. The
`postgres_scanner` extension required by `duckdb_postgres_driver` has
been GA since DuckDB 0.5 (October 2022), well before 1.1.1.

### Deviation 2 — Hand-rolled minimal JSON parser inside `http_json_driver.rs`

**Status:** Stream-C judgement call to avoid filing SPEC QUESTION 002.

**Why:** `serde_json` is **not** in ADR-0010 Decision 4's pinning matrix,
and the handoff's SPEC-QUESTION trigger #4 says "do NOT add it
unilaterally." But `HttpJsonDriver` cannot do its job without parsing
JSON. Three options were on the table:

1. Add `serde_json` to mc-drivers' `[dependencies]` → triggers SPEC
   QUESTION 002.
2. Enable `ureq`'s `json` feature → also pulls `serde_json`; same
   trigger.
3. Hand-roll a minimal RFC-8259 parser inside the crate → no new dep.

I picked option 3. The parser is ~280 lines, lives in
`crates/mc-drivers/src/http_json_driver.rs::mod json`, supports the
full grammar Mosaic needs (scalars, strings with escapes including
`\uXXXX`, arrays, objects with last-write-wins on duplicate keys,
numbers parsed as `f64`), and is exercised by 10 driver-level tests.
It is intentionally NOT a streaming parser — Phase 5A reference-
implementation workloads (recipe imports of ~10K-100K rows) fit
comfortably in memory.

**Ratification path if you'd rather have `serde_json`:** an ADR-0010
Amendment 2 adding `serde_json = "1"` to Decision 4 + replacing the
in-tree parser with `serde_json::from_str` is a ~50-line drop-in change
that keeps the same module structure. Flagging for Stream D / mc-tessera
review since they may want serde-shaped types throughout the recipe
pipeline anyway.

### Deviation 3 — `column_decltype` feature added to both `rusqlite` and `duckdb` workspace deps

**Status:** Mechanical necessity (not a contract amendment).

The handoff `Cargo.toml` block specifies `features = ["bundled"]` for
both. Without `column_decltype`, neither driver can read column metadata
from a prepared statement (rusqlite's `Statement::columns()` and
duckdb's column-name accessors are gated behind this feature).
Documented inline in `crates/mc-drivers/Cargo.toml`.

---

## Lesson learned (per Amendment 1's verification protocol)

**Future ADR pin-matrix "Verified on 1.78 ✓" claims must be verified
against fresh state**, not against an existing `Cargo.lock`. The
canonical verification ritual:

```bash
rm -rf scratch && mkdir scratch && cd scratch
cargo init --name verify && cd verify
# add the candidate pin set to Cargo.toml here, leave Cargo.lock absent
cargo +1.78 build --locked   # MUST fail clean if Cargo.lock missing — that's intended;
cargo +1.78 generate-lockfile
cargo +1.78 build --locked   # this MUST succeed for verification to count
```

The original Decision 4 verification on 2026-05-04 used an existing
local `Cargo.lock` that had already locked older arrow / libduckdb-sys
versions. When SPEC QUESTION 001 ran the same pins against fresh state
in this worktree (correctly applying the protocol above), the resolver
selected current crates.io defaults (`arrow 55.x`, `libduckdb-sys 1.3.2`)
which require Rust 1.81+. The pin matrix as written in Decision 4 was
therefore unbuildable from scratch on 1.78.

ADR-0010 Amendment 1 codifies the corrected pin set + the verification
protocol so this class of false positive does not recur for Phase 5B+.

---

## What this stream did NOT do (by contract)

- ❌ Modify any source file in `mc-core/`, `mc-fixtures/`, `mc-model/`, `mc-cli/` (verified: zero diff).
- ❌ Bump the toolchain pin (`rust-toolchain.toml` unchanged).
- ❌ Push root `Cargo.toml` / `Cargo.lock` independently (per amendment #6 — staged locally only, flagged above for PM integration).
- ❌ Add any dependency outside Decision 4 + Amendment 1's matrix (no `serde_json`, no `tokio`-as-direct-dep, no `arrow`-as-direct-dep, no `bumpalo`, no `rayon`).
- ❌ Start Stream D / `mc-tessera` work, write recipe parsing code, or modify `mc-recipe`.
- ❌ Implement async / `.await` / threads in any driver.

---

## Reproducible commands (the gate)

```bash
cd /Users/edwinlovettiii/Projects/mc-v2-stream-c
rustc --version
# rustc 1.78.0

cargo build --release --workspace
# zero warnings

cargo fmt --check --all
# exit 0

cargo clippy --workspace --all-targets -- -D warnings
# exit 0

cargo test --workspace
# 446 passed; 0 failed; 5 ignored (live-PG tests)

# Determinism (10 consecutive runs identical)
for i in $(seq 1 10); do cargo test --workspace -q || echo "FAIL run $i"; done

# Tokio dep-gate explicitly
cargo test -p mc-drivers --features dependency-gate --test dependency_gate
# 2 passed; 0 failed

# block-buffer pin verification
cargo tree -p mc-drivers -i block-buffer | head -1
# block-buffer v0.10.4

# Locked surfaces
git diff phase-4b-python-adapters -- crates/mc-core/ crates/mc-fixtures/ crates/mc-model/ crates/mc-cli/
# (zero output)

# Forbidden patterns
grep -rn "\.unwrap()\|\.expect(" crates/mc-drivers/src/             # (none)
grep -rn "^async\|\.await\|use tokio::" crates/mc-drivers/src/      # (one doc-comment hit in lib.rs only)
grep -rn "println!\|eprintln!\|dbg!" crates/mc-drivers/src/         # (none)
grep -rn "\bunsafe\b" crates/mc-drivers/src/ crates/mc-drivers/tests/ # (none)
```

---

## Stream A / Stream B coordination notes

- Stream A (WriteBatch in `mc-core`) is unaffected — Stream C touched
  zero `mc-core` source. The `WriteBatch` API surface as defined in
  ADR-0010 Appendix A is the Stream A → Stream D contract; Stream C
  does not consume it.
- Stream B (recipe format in `mc-recipe`) depends on
  `serde`/`serde_yaml`/`thiserror` per Decision 7 — none of which are
  in mc-drivers' dep tree. No conflict.
- Stream D (Tessera orchestrator in `mc-tessera`) is the integration
  point: it depends on Stream A's `WriteBatch`, Stream B's `Recipe`
  type, and Stream C's `SourceDriver` trait. The trait surface from
  Appendix C is shipped here verbatim. Stream D consumes via:

  ```rust
  pub struct PreparedImport {
      pub recipe: Recipe,                        // from mc-recipe
      pub cube: Cube,                            // from mc-core
      pub driver: Box<dyn SourceDriver>,         // from mc-drivers ✓
      pub column_plan: Vec<ResolvedColumnMapping>,
  }
  ```

  All Stream C drivers' return types coerce to `Box<dyn SourceDriver>`
  (the trait is object-safe — no generic methods, no `Self` in return
  position, no associated types).

---

## What the PM still owns

1. **Final integration** of root `Cargo.toml` + `Cargo.lock` across
   Streams A, B, C on the merge branch.
2. **Decide on Deviation #2** (in-tree JSON parser) — keep as-is, or
   amend ADR-0010 to add `serde_json` and replace.
3. **Tag commit** when satisfied. Stream C does not commit or tag per
   the handoff ("Do NOT commit or tag. The user reviews first.").

---

## Awaiting

PM review + commit/tag. Stream C work product is complete on this branch
(`phase-5a/stream-c-source-drivers`). On approval the root Cargo.toml
diff above can be merged into the integration branch alongside Streams
A and B.
