# Binary Size, the DuckDB Anchor, and a Capability-Split Path to a ~5MB Mosaic

**Status:** Research note (pre-ADR; explores the design space — a refinement of [ADR-0025](../decisions/0025-kernel-discipline-and-deployment-architecture.md)'s deployment shapes)
**Date:** 2026-05-27
**Author:** Mosaic PM (Claude Opus 4.8, 1M context)
**Prompted by:** comparison to a 7MB Rust terminal (terax-ai) — "can Mosaic be that small and that powerful?"

---

## The measured reality (not estimates)

| Artifact | Size | What it contains |
|---|---|---|
| `mc` (the monolith binary) | **55 MB** (40 MB stripped) | kernel + model + daemon + Tessera + DuckDB bundled |
| `mc-model-schema` | **885 KB** | `mc-core` + `mc-model` only — the kernel + the entire formula/model layer |
| All Rust source | 112,633 LOC, 234 files, 12 crates | — |
| Source tree (no `.git`, no `target/`) | small | — |
| `target/` build cache | ~29 GB | disposable; `cargo build` regenerates |

**The headline finding:** the kernel + model layer already compiles to
**885 KB** — 8× *smaller* than the 7MB terminal we're comparing against.
Mosaic's core is not bloated. The 55 MB `mc` binary is big for exactly
one structural reason, quantified below.

---

## Where the 55 MB comes from

The `mc` binary is a **monolith** — one executable carrying every
capability. Three things dominate its size, in order:

1. **DuckDB, bundled (~35-40 MB).** `mc-drivers` depends on
   `libduckdb-sys` with the `bundled` feature, which compiles the entire
   DuckDB C++ analytical-database engine *into the binary*. This is the
   single largest contributor — likely 60-70% of the 55 MB. DuckDB is
   only needed for **parquet input** (Tessera ingestion + `simulate`'s
   bet-record reader). The kernel, model layer, daemon HTTP, and every
   evaluation command except parquet-reading need none of it.

2. **The async stack (tokio + axum).** Pulled by `mc-daemon` for the
   HTTP/slider workflow. A few MB. Only the daemon needs it; the CLI
   verbs are all synchronous.

3. **Everything else linked together.** Narrative engine, Tessera
   recipes, workspace, diagnostics — all compiled into the one binary
   even when a given invocation uses none of them.

The `opt-level = 3` / `lto = "thin"` / `codegen-units = 1` profile is
already reasonable for speed. `strip = true` alone takes 55 MB → 40 MB
(verified). But stripping is a rounding error next to the DuckDB anchor.

---

## The insight: Mosaic is already small where it counts

The thing that makes Mosaic *powerful* — the evaluation kernel: cube
semantics, the formula language, `predict()`, `nbinom_sf`, the whole
Phase 10 evaluation track (`grade`, `simulate`, the metrics library) —
lives in `mc-core` + `mc-model` and **compiles to under 1 MB**
(`mc-model-schema` proves it at 885 KB).

The power isn't in the megabytes. terax-ai is 7 MB because a terminal
needs a rendering + PTY + input stack. Mosaic's *equivalent core* — the
evaluation engine — is already smaller than that. The 55 MB is not
"Mosaic is heavy"; it's "one binary ships an embedded database and a web
server alongside the engine."

---

## The path to a ~5 MB Mosaic: split by capability

This is a refinement of ADR-0025's deployment-shape thinking. Three
binaries instead of one monolith, split along the dependency seams that
already exist:

### `mc` (lite) — the evaluation engine, ~3-5 MB
- `mc-core` + `mc-model` + `mc-cli`'s evaluation verbs (`query`,
  `whatif`, `sweep`, `grade`, `simulate`, `trace`, `validate`, `test`,
  the formula engine, `predict`, `nbinom_sf`, the metrics library)
- **jsonl I/O only — no DuckDB.** This is exactly why ADR-0035
  Amendment 4 made `simulate`'s curve output jsonl and noted parquet as
  deferred. A jsonl-fed evaluation binary skips the DuckDB anchor
  entirely.
- This is what ~95% of users run. ~3-5 MB, comparable to or smaller
  than the terminal.

### `mc-data` — ingestion + parquet, the DuckDB binary
- `mc-drivers` (DuckDB bundled) + Tessera + parquet readers
- The ~40 MB lives here, isolated, only installed by users who ingest
  parquet/CSV at scale.
- `mc` (lite) reads jsonl; `mc-data` converts parquet → jsonl/canonical
  when needed. The split is the same Python-converts / Mosaic-evaluates
  seam, internalized.

### `mc-server` — the daemon, tokio + axum
- `mc-daemon` for the HTTP/slider/`/whatif`/`/sweep` workflow.
- The async stack lives here, off the CLI's critical path.

The seams already exist as crate boundaries — this is a `[[bin]]` +
feature-flag reorganization, not a rewrite. `mc-core`/`mc-model` are
already DuckDB-free, tokio-free, axum-free (verified: only `mc-drivers`
references duckdb, only `mc-daemon` references the async stack).

---

## Additional levers (smaller wins, independent of the split)

1. **Dynamic-link DuckDB instead of `bundled`.** Drops ~35 MB out of any
   binary that needs DuckDB, at the cost of a runtime
   `libduckdb` dependency. Fine for a server install; a tradeoff for a
   distributable CLI (the `bundled` feature exists precisely so the
   binary is self-contained). Relevant for `mc-data`/`mc-server`, not
   for `mc` (lite) which wouldn't link DuckDB at all.

2. **A `dist` release profile.** `opt-level = "z"` (size over speed),
   `strip = true`, `panic = "abort"`, `lto = "fat"`. Typically 30-50%
   off a Rust binary. The current profile optimizes for speed
   (`opt-level = 3`) — correct for the kernel's hot path, but a `dist`
   profile for distributable builds would trade a little eval speed for
   a lot of size. Measure before committing: the kernel's perf ceilings
   (PERF.md) must still hold under `opt-level = "z"`.

3. **Feature-gate the optional layers.** Narrative engine, recipe
   authoring, workspace/org — `#[cfg(feature = ...)]` so a minimal build
   excludes them. Marginal next to the DuckDB split, but compounds.

---

## The honest recommendation: capture this, don't act on it yet

**Don't optimize the monolith. Split it — but not now.**

For a napkin-stage, single-user project, the 55 MB monolith **costs
nothing today.** You run it locally; disk is free; cold-start doesn't
matter; there's no distribution channel where 55 MB vs 5 MB changes a
decision. Binary size becomes real when:

- **Distributing to users** (download size, install friction)
- **Serverless / edge cold-start** (binary size → cold-start latency)
- **Constrained environments** (embedded, CI cache, container layers)
- **A "try Mosaic in 10 seconds" onboarding** where a 5 MB `brew install`
  beats a 55 MB one

None of those are live. The split is architecturally clean and the seams
already exist, so it'll be cheap *when* it matters — but doing it now is
optimization ahead of need, and it would add build/release complexity
(three binaries, feature matrices, CI changes) to a project whose
current bottleneck is "what does claw-core need next," not "how big is
the binary."

**What to do now:** nothing to the build. This note captures the design
so that when distribution becomes real, the path is already mapped: the
kernel is 885 KB, the anchor is bundled DuckDB, the split follows
existing crate seams, and ADR-0025's deployment shapes are the framing.

---

## When to revisit (the triggers)

Promote this to an ADR when ANY of these becomes true:
- A real distribution channel appears (Homebrew tap, GitHub release
  downloads, `cargo install` as the install path users actually use).
- A serverless/edge deployment is on the table (cold-start matters).
- A "minimal Mosaic" onboarding story is a priority (the 10-second try).
- DuckDB needs upgrading anyway (the pin is Rust-1.78-constrained per
  `mc-drivers/Cargo.toml`) — bundle the split decision into that work.

Until then: the kernel is already terminal-sized. The monolith is a
convenience, not a constraint.

---

## Cross-links
- [ADR-0025](../decisions/0025-kernel-discipline-and-deployment-architecture.md) — deployment shapes; this note refines the binary-packaging dimension
- [ADR-0035](../decisions/0035-phase-10f-model-simulate.md) Amendment 4 — jsonl curve output / parquet deferred; the seam that lets `mc` (lite) skip DuckDB
- `crates/mc-drivers/Cargo.toml` — the `libduckdb-sys bundled` pin (the 40 MB anchor) and its Rust-1.78 constraints
- `Cargo.toml` `[profile.release]` — current speed-optimized profile
- PERF.md — the kernel perf ceilings a `dist` (`opt-level = "z"`) profile must not break

---

## Notes
- The "30 GB repo" that prompted the disk-cleanup question is `target/`
  build cache (~29 GB), NOT source or binary — disposable, regenerated
  by `cargo build`. `cargo test` refills `target/debug` (~20 GB) each
  run; `rm -rf target/debug` reclaims it. Unrelated to binary size; just
  build-cache hygiene.
- Stripped monolith is 40 MB; the 885 KB `mc-model-schema` is the real
  "how small is the engine" answer. Both measured this session, not
  estimated.
