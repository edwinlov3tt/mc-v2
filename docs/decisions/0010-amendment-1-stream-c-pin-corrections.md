# ADR-0010 Amendment 1: Stream C Pin Corrections (DuckDB downgrade + transitive pin matrix completion)

**Status:** Accepted
**Date:** 2026-05-04
**Filed by:** PM, after Stream C SPEC QUESTION 001
**Amends:** [ADR-0010 Decision 4](./0010-phase-5-tessera-architecture.md) (dependency pinning matrix)

---

## What changed

### 1. DuckDB pin downgraded: `=1.3.2` → `=1.1.1`

**Original ADR-0010 claim:** `duckdb = "=1.3.2"` — "Last 1.78-credible `duckdb-rs` version; Verified on 1.78 ✓"

**Corrected:** `duckdb = "=1.1.1"` + `libduckdb-sys = "=1.1.1"` — the ACTUAL last version buildable on Rust 1.78.

**Root cause of the false "Verified" claim:** The ADR-0010 pin matrix referenced the research report's §11 scratch test, but that test likely ran against a stale `Cargo.lock` that pre-resolved `libduckdb-sys` to a version using `bindgen ^0.69` (pre-1.82 syntax). A fresh resolve of `duckdb = "=1.3.2"` pulls `libduckdb-sys 1.3.2` which uses `bindgen ^0.71.1` — generating `unsafe extern "C" { }` blocks that require Rust 1.82+. Additionally, `duckdb 1.3.2` hard-requires `arrow ^55` which declares `rust-version = "1.81"`, causing a resolver rejection even before the bindgen issue surfaces.

**The cliff:** `libduckdb-sys 1.2.0` (released ~November 2024) jumped bindgen from `^0.69` to `^0.71.1`. Everything from `duckdb-rs 1.2.0` onward is unbuildable on Rust 1.78.

**Functional impact of the downgrade:** Effectively zero for Phase 5A.

- DuckDB's `postgres_scanner` extension has been GA since DuckDB 0.5 (October 2022). DuckDB 1.1.1 (DuckDB engine ~0.10.x vintage) comfortably supports `INSTALL postgres; LOAD postgres; ATTACH …`. The `duckdb_postgres` driver is NOT blocked.
- Stream C's workload (execute queries, drain rows into `RowBatch`) doesn't exercise any DuckDB feature that landed only in 1.2+. The 1.2/1.3 changes are mostly catalog features, optimizer rewrites, and extension surface.
- Stream D's acceptance gate (`tessera_apply/100K_sqlite`) doesn't touch DuckDB anyway.

### 2. Five additional transitive pins added to the matrix

These are mechanically necessary to reach a clean build on Rust 1.78 — they address edition2024 and MSRV declarations in transitive deps that cargo 1.78's resolver rejects:

| Crate | Pinned version | Why |
|---|---|---|
| `proc-macro-crate` | `=3.3.0` | 3.4.0 pulls `toml_edit 0.23` → `toml_parser 1.1.2` (edition2024 manifest) |
| `idna_adapter` | `=1.1.0` | 1.2.x is edition2024 (URL parsing dep of ureq + postgres) |
| `comfy-table` | `=7.1.4` | 7.2.0+ requires edition2024 (`arrow-cast`'s `prettyprint` feature; duckdb enables it) |
| `uuid` | `=1.20.0` | 1.21.0+ declares `rust-version = "1.85"`; resolver picks 1.23.x by default |
| `unicode-segmentation` | `=1.12.0` | 1.13.0/1.13.1 are yanked; 1.13.2 requires 1.85; 1.12.0 is the last viable release |

### 3. Postgres chain re-verified (no issues found)

The 7 Postgres-related pins from the original ADR-0010 Decision 4 were re-verified in a fresh scratch project (no stale `Cargo.lock`) on 2026-05-04:

```
postgres = "=0.19.9"
postgres-protocol = "=0.6.7"
sha2 = "=0.10.8"
hmac = "=0.12.1"
md-5 = "=0.10.6"
digest = "=0.10.7"
block-buffer = "=0.10.4"
```

Result: **builds clean on `cargo +1.78 build --locked`**. Only `block-buffer 0.10.4` in the lockfile (no 0.12.0). `tokio 1.52.2` resolves as expected (transitive of `tokio-postgres 0.7.12`). No additional pins required.

---

## Corrected Decision 4 pinning matrix (full, as-shipped)

| Crate | Version | Where | Category |
|---|---|---|---|
| `rusqlite` | `=0.31.0` | `mc-drivers` dep | Direct (unchanged) |
| `duckdb` | **`=1.1.1`** | `mc-drivers` dep | **Corrected** (was =1.3.2) |
| `libduckdb-sys` | **`=1.1.1`** | transitive pin | **New** (forces bindgen ^0.69) |
| `postgres` | `=0.19.9` | `mc-drivers` dep | Direct (unchanged) |
| `postgres-protocol` | `=0.6.7` | transitive pin | Unchanged |
| `sha2` | `=0.10.8` | transitive pin | Unchanged |
| `hmac` | `=0.12.1` | transitive pin | Unchanged |
| `md-5` | `=0.10.6` | transitive pin | Unchanged |
| `digest` | `=0.10.7` | transitive pin | Unchanged |
| `block-buffer` | `=0.10.4` | transitive pin | Unchanged |
| `ureq` | `2` | `mc-drivers` dep | Direct (unchanged) |
| `csv` | `1` | `mc-drivers` dep | Direct (unchanged) |
| `proc-macro-crate` | **`=3.3.0`** | transitive pin | **New** |
| `idna_adapter` | **`=1.1.0`** | transitive pin | **New** |
| `comfy-table` | **`=7.1.4`** | transitive pin | **New** |
| `uuid` | **`=1.20.0`** | transitive pin | **New** |
| `unicode-segmentation` | **`=1.12.0`** | transitive pin | **New** |

Total pins: 17 (was 11 in original ADR-0010; +1 corrected duckdb + 1 new libduckdb-sys + 5 new transitive).

---

## Lesson learned (carry-forward for future ADRs)

**Future ADR pin-matrix "Verified on 1.78 ✓" claims must be verified against fresh state:**

```bash
# The verification protocol (mandatory for every pin claim):
rm -rf /tmp/mosaic-pin-gate && mkdir -p /tmp/mosaic-pin-gate/src
cd /tmp/mosaic-pin-gate
# Write Cargo.toml with the proposed pins
# Write a minimal src/main.rs that imports the crate
cargo generate-lockfile              # ← fresh resolve, no stale Cargo.lock
cargo +1.78 build --locked           # ← build with the locked resolution
# Only if this exits 0: "Verified on 1.78 ✓"
```

The original ADR-0010 verification for DuckDB likely ran against the existing `Cargo.lock` in the research report's scratch project, which pre-resolved to older transitive deps. A fresh resolve (as any new workspace checkout would produce) selects newer versions that break on 1.78.

---

## Cross-links

- [ADR-0010](./0010-phase-5-tessera-architecture.md) Decision 4 — the original pinning matrix this amends
- [Stream C SPEC QUESTION 001](../../docs/handoffs/phase-5a-stream-c-spec-question-001.md) — the evidence that triggered this amendment (filed from the `phase-5a/stream-c-source-drivers` worktree)
- [CLAUDE.md §1.1](../../CLAUDE.md) — the criterion/proptest/insta deviation precedent (Phase 1B's transitive pins established the pattern)
