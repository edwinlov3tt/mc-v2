# Phase 5A Stream C — SPEC QUESTION 001

**Filed:** 2026-05-04 (same day as ADR-0010 Acceptance and the Stream C handoff draft).
**Author:** Stream C Claude Code instance, branch `phase-5a/stream-c-source-drivers`.
**Trigger:** Stream C handoff "SPEC QUESTION triggers" #1 — dependency does not build on Rust 1.78 with the pinned version.
**Status:** **Blocking — Stream C implementation paused pending PM decision.**

---

## SPEC QUESTION (one line)

The headline `duckdb = "=1.3.2"` pin in ADR-0010 Decision 4 cannot be built on Rust 1.78 today, because `libduckdb-sys 1.3.2` (and 1.2.x) generates bindgen output using `unsafe extern "C"` block syntax, which requires rustc 1.82+. How should Stream C proceed?

---

## Spec text (literal quotes)

From the handoff:

> **Phase 5A dependency pinning matrix (all verified against Rust 1.78 via scratch test):**
>
> | Crate | Version | Where it lives | Why pinned | Verified on 1.78 |
> | --- | --- | --- | --- | --- |
> | `duckdb` | `=1.3.2` | `mc-drivers` | Last 1.78-credible `duckdb-rs` version; `bundled` feature for zero-install DuckDB | ✓ (per research §11 scratch test) |

From the handoff "SPEC QUESTION triggers" section:

> Open a SPEC QUESTION (per CLAUDE.md section 11) before continuing if:
>
> 1. A dependency does not build on Rust 1.78 with the pinned version. The pinning matrix in Decision 4 was verified via scratch test; if it breaks, something changed upstream. Document the exact error and the `cargo tree` output.

From the research report §11 ("30-Minute Scratch Test"), the only validation given for the duckdb pin:

> ```toml
> duckdb = { version = "=1.3.2", features = ["bundled"] }
> ```
> *(scratch test code path: `DuckConn::open_in_memory()` plus `prepare/query_map`)*

The research report did **not** include a `postgres` driver in the scratch test (Path 2 was a later ADR-0010 decision), and did not record a date when `cargo +1.78 build` of the scratch was last successful — only that "if `cargo +1.78 check` succeeds" the recommendation is materially proven.

---

## Hard evidence

### Evidence 1 — `duckdb 1.3.2` rejects on Rust 1.78 due to MSRV-aware resolver

`cargo check -p mc-drivers` with `duckdb = "=1.3.2"`:

```
error: rustc 1.78.0 is not supported by the following packages:
  arrow@55.2.0 requires rustc 1.81
  arrow-arith@55.2.0 requires rustc 1.81
  arrow-array@55.2.0 requires rustc 1.81
  arrow-buffer@55.2.0 requires rustc 1.81
  arrow-cast@55.2.0 requires rustc 1.81
  arrow-data@55.2.0 requires rustc 1.81
  arrow-ord@55.2.0 requires rustc 1.81
  arrow-row@55.2.0 requires rustc 1.81
  arrow-schema@55.2.0 requires rustc 1.81
  arrow-select@55.2.0 requires rustc 1.81
  arrow-string@55.2.0 requires rustc 1.81
  unicode-segmentation@1.13.2 requires rustc 1.85.0
```

`duckdb-1.3.2`'s manifest hard-requires `arrow ^55` (verified via the crates.io
sparse index). Every published `arrow 55.x` declares `rust-version = "1.81"`.
Cargo 1.78's resolver respects this and refuses to select.

### Evidence 2 — `--ignore-rust-version` does not save us; the actual code uses Rust 1.82+ syntax

`cargo check -p mc-drivers --ignore-rust-version` produces (after a long compile):

```
error: extern block cannot be declared unsafe
    --> .../target/debug/build/libduckdb-sys-.../out/bindgen.rs:2599:1
     |
2599 | unsafe extern "C" {
     | ^^^^^^
... 408 errors total ...
error: could not compile `libduckdb-sys` (lib) due to 408 previous errors
```

`libduckdb-sys` 1.2.x and 1.3.x build with `bindgen ^0.71.1`, which generates
the `unsafe extern "C" { … }` block syntax stabilised in Rust 1.82. Bypassing
the resolver does not bypass this — the actual codegen does not compile on
1.78.

### Evidence 3 — the bindgen jump is the cliff at libduckdb-sys 1.2.0

Per the crates.io sparse index, `libduckdb-sys` bindgen requirements per release:

| `libduckdb-sys` | bindgen req | bindgen output style on 1.78 |
| --- | --- | --- |
| 0.10.x, 1.0.0, 1.1.1 | `^0.69` | safe `extern "C"` blocks → builds on 1.78 |
| 1.2.0 – 1.4.4 | `^0.71.1` | `unsafe extern "C"` blocks → fails on 1.78 |
| 1.10500.x+ | `^0.72.1` | same; fails on 1.78 |

`duckdb 1.1.1` is the **last** `duckdb-rs` release on Rust 1.78 today. `duckdb 1.2.0`
through `1.10502.0` all break.

### Evidence 4 — additional transitive pins beyond Decision 4 are also required

The 6 RustCrypto pins in Decision 4 are accurate but not sufficient. The
following additional transitive pins are needed to even reach the
libduckdb-sys compile step on Rust 1.78:

| Pin | Reason |
| --- | --- |
| `proc-macro-crate = "=3.3.0"` | 3.4.0 pulls `toml_edit 0.23` → `toml_parser 1.1.2` (edition2024 manifest) |
| `idna_adapter = "=1.1.0"` | 1.2.x is edition2024 (URL parsing dep of ureq + postgres) |
| `comfy-table = "=7.1.4"` | 7.2.0+ requires edition2024 (`arrow-cast`'s `prettyprint` feature pulls it; duckdb enables `prettyprint`) |
| `uuid = "=1.20.0"` | 1.21.0+ declares rust-version 1.85; resolver picks 1.23.x by default |
| `unicode-segmentation = "=1.12.0"` | 1.13.0/1.13.1 are yanked; 1.13.2 requires 1.85; 1.12.0 is the last viable release |

These are mechanically necessary regardless of the duckdb decision below.
The handoff anticipates this in the cargo governance section ("if the
resolver picks wrong versions … you must add `[patch]` entries or use
`cargo update -p <crate> --precise <version>` to force the correct pins"),
so they should not require an ADR amendment — but they should be enumerated
in the completion report as Decision 4 pin-matrix additions.

### Evidence 5 — the empirically-working configuration

With `duckdb = "=1.1.1"` + `libduckdb-sys = "=1.1.1"` (forced via
`cargo update -p libduckdb-sys --precise 1.1.1`) plus the 5 additional pins
in Evidence 4, **the workspace compiles** through to source-level errors
in my driver code (rusqlite/duckdb API surface mismatches that are a
straightforward fix). No more dependency-chain breakage.

`duckdb 1.1.1` pulls `arrow ^53`. `arrow 53.x` declares `rust-version = "1.70"`.
`libduckdb-sys 1.1.1` uses `bindgen ^0.69` which emits the pre-1.82 syntax.

---

## The conflict / ambiguity

The handoff freezes the pin at `=1.3.2` with `Verified on 1.78 ✓`, but at the
present moment `cargo +1.78 build` of that pin fails (twice over: resolver
rejection AND, with `--ignore-rust-version`, bindgen output rejection).

The pinning matrix was authored on 2026-05-04 (today). One of three things
is true:

1. The author tested with stale local state (an older `Cargo.lock` already
   selecting `arrow 54.x` / `libduckdb-sys` with bindgen 0.69) and the
   "Verified on 1.78" claim was a false positive against fresh state.
2. crates.io was updated between the verification time and now (within
   today, possibly hours).
3. There is an unspoken additional `[patch]` set the verification used that
   wasn't recorded in Decision 4.

Either way: today, Phase 5A Stream C cannot ship the documented matrix
without picking one of three remediations.

---

## My proposed interpretation (recommended option)

**Option A — Downgrade `duckdb` and `libduckdb-sys` to `=1.1.1`.**

Rationale:
- Restores buildability on the existing `Rust 1.78` toolchain pin (this is
  the central project invariant; ADR-0010 is explicit that "Phase 5A does
  NOT trigger a toolchain bump").
- Smallest footprint deviation: changes one pin row in Decision 4.
- DuckDB's `postgres_scanner` extension has been GA since DuckDB 0.5
  (released October 2022). DuckDB 1.1.1 (released ~September 2024)
  comfortably supports `INSTALL postgres; LOAD postgres; ATTACH …`. The
  Stream C `duckdb_postgres_driver` is therefore not blocked.
- Capability gap vs `1.3.2` for Stream C's reference-implementation
  workload (issue a `SELECT`, drain rows, return Mosaic `RowBatch`):
  effectively zero. The 1.2/1.3 changes are mostly about catalog
  features, optimizer rewrites, and extension surface — none of which
  Stream C exercises.
- Stream D (Tessera orchestrator) has no known dependency on
  duckdb-rs ≥ 1.2.

**Open question for confirmation:** Is the bundled DuckDB binary in
`libduckdb-sys 1.1.1` adequate for the Phase 5A acceptance gates? If
Tessera or `mc tessera apply` exercises a feature that landed only in
DuckDB ≥ 1.2 binary, Option A is unsafe and we need Option B/C.

## Other options considered

**Option B — Bump `rust-toolchain.toml` to `1.82` (or `1.85` to also satisfy
`uuid 1.21+`, `unicode-segmentation 1.13.2`, etc.).**

Pros: unlocks the full ADR-0010 pin matrix verbatim; no driver-side
deviation. Cons: contradicts ADR-0010 Decision 4 explicitly: "Phase 5A
does NOT trigger a toolchain bump." Cascades into Phase 1B/3A pin
re-validation work (PERF.md §9.7 references this). Not a localised
decision; touches every existing crate.

**Option C — Defer the two DuckDB-based drivers (`DuckDbDriver` and
`DuckdbPostgresDriver`) to Phase 5B; ship the other 4 drivers in 5A.**

Pros: zero deviation from documented pins; preserves toolchain. Cons:
contradicts the binding contract — handoff §"Phase 5A Stream C scope" item
4 lists all 6 driver constructors as required, and the final checklist
explicitly demands "DuckDB driver" and "DuckDB-Postgres driver". The
handoff also marks `duckdb_postgres` as "first-class driver, not a
fallback." Deferral is a scope cut, not a workaround.

---

## What I would do without confirmation

**Nothing further.** Per CLAUDE.md §11 ("Do not say 'I'll just guess' or
proceed silently when you're unsure") and the handoff's explicit
SPEC-QUESTION-then-wait instruction (trigger #1), Stream C work is paused
at the following state:

- `crates/mc-drivers/Cargo.toml` carries the experimental pin set from
  Evidence 5 (duckdb=1.1.1 + the 5 pins). A header comment flags this as
  "PROBE — pending PM SPEC QUESTION 001 decision."
- Root `Cargo.toml` carries the workspace-deps additions per the handoff,
  with `duckdb = "=1.1.1"` and a comment flagging the same.
- Driver source files (`csv_driver.rs`, `sqlite_driver.rs`,
  `duckdb_driver.rs`) are written but not yet compiling cleanly (1–4
  source errors against the resolved API; trivial to fix once pin
  decision is made).
- The remaining drivers (`postgres`, `duckdb_postgres`, `http_json`) are
  stubs that compile. Tests not yet written. Dependency-gate test not
  yet written.

When the PM decides on Option A / B / C:
- Option A: I update the pin comment to "Stream-C-approved deviation;
  Decision 4 amendment proposed", finish the 4 source-level fixes, and
  resume the implementation queue.
- Option B: I revert the duckdb pin to `=1.3.2`, drop the 5 extra
  pins (the resolver will pick the new defaults), and update
  `rust-toolchain.toml`. Re-run all existing 416 tests as a regression
  pass.
- Option C: I delete `duckdb_driver.rs` and `duckdb_postgres_driver.rs`,
  remove the duckdb dep, and update Appendix C to list 4 drivers in
  Phase 5A and 2 in Phase 5B.

---

## Reproduction steps

```bash
cd /Users/edwinlovettiii/Projects/mc-v2-stream-c
git status                       # branch phase-5a/stream-c-source-drivers
rustc --version                  # rustc 1.78.0
# To reproduce Evidence 1 (resolver rejection):
git stash                        # stash current probe state if applied
# Set duckdb back to =1.3.2 in workspace + remove the extra pins
cargo check -p mc-drivers        # → "rustc 1.78.0 is not supported by..."
# To reproduce Evidence 2 (bindgen failure):
cargo check -p mc-drivers --ignore-rust-version
# → "error: extern block cannot be declared unsafe"
# To reproduce Evidence 5 (working with =1.1.1):
git stash pop                    # restore probe state
cargo update -p libduckdb-sys --precise 1.1.1
cargo check -p mc-drivers        # → compiles dep tree; only my driver source errors remain
```

---

## Files attached / changed in this branch (not yet committed)

- `Cargo.toml` (root) — workspace member + dep additions; `duckdb` currently `=1.1.1` PROBE.
- `crates/mc-drivers/Cargo.toml` — driver crate manifest with 11 transitive pins (6 documented + 5 probe).
- `crates/mc-drivers/src/lib.rs` — trait + types per Appendix C.
- `crates/mc-drivers/src/csv_driver.rs` — full implementation; compiles.
- `crates/mc-drivers/src/sqlite_driver.rs` — implementation with 1 fix needed (`stmt.columns()` → `stmt.column_names()`).
- `crates/mc-drivers/src/duckdb_driver.rs` — implementation with ~3 fixes needed (DuckType variant names + `rust_decimal` import).
- `crates/mc-drivers/src/postgres_driver.rs` — stub.
- `crates/mc-drivers/src/duckdb_postgres_driver.rs` — stub.
- `crates/mc-drivers/src/http_json_driver.rs` — stub.

No tests written yet. No completion report written yet. Branch not committed.

---

## Awaiting

PM decision on Option A / B / C, plus confirmation of the open question
under Option A (DuckDB 1.1.1 binary capability adequacy for Phase 5A
acceptance gates, particularly Stream D's `tessera_apply/100K_sqlite`
gate which doesn't touch DuckDB anyway, and any future Phase 5B/5C
DuckDB-specific feature use).
