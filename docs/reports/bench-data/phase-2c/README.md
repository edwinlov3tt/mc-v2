# `bench-data/phase-2c/` — Production-Shaped Workload Baseline

> **Status:** populated by `cargo bench --save-baseline phase-2c` against the post-Phase-2C HEAD. Captures the Phase 1B + Phase 2A + Phase 2B + Phase 2C bench rows that ran to completion, including the new 10× / 50× / 100× scaled-Acme variants and the combined-workflow bench. This README is the per-baseline metadata; the cross-baseline workflow lives in [`../README.md`](../README.md).

## Captured at

- **Tag:** `phase-2c-workload-baseline`
- **Commit:** `789db15` — *bench: complete Phase 2C workload-shaped benchmark baseline*
- **Backfill commit:** `96cca75` (post-tag README + PERF.md + bench-data backfill; baseline JSON unchanged)
- **Date:** 2026-05-02
- **Machine:** Apple M4, macOS 26.3 (matches phase-2a + phase-2b machine; comparable medians)
- **Toolchain:** Rust 1.78 (unchanged from prior baselines)
- **Cargo.lock pins:** `clap → 4.4.18`, `clap_lex → 0.6.0`, `half → 2.4.1` (unchanged from Phase 1B)
- **Sample size:** `--sample-size 10` (criterion's minimum) for the gate run; phase-2a + phase-2b ran at sample-size 100. Wider confidence intervals on this baseline; the `--baseline-lenient` diff against phase-2b for unchanged 1× rows accommodates this. See PERF.md §6.12 prologue.

## Captured / missing / env-gated rows

**Captured** (in this baseline):

- `consolidated_read::*` at all calibration scales that ran to completion.
- `derived_read::*` at all calibration scales that ran to completion.
- `leaf_read_write::read_input_leaf_warm` / `_cold`, `read_derived_leaf_cold/Revenue` at 1× / 10× / 50× / 100× (read side only).
- `leaf_read_write::write_input_leaf` at 1× / 10× (write side; 50× and 100× write-side rows were not captured — see env-gated below).
- `demo_path::load_canonical_inputs` at 1× (~10 ms) / 10× (~50 s) / 50× (~231 s).
- `dirty_propagation::spend_to_revenue` at 1× / 10× / 50×.
- `combined_workflow/50x` (100-iter session, 10 stacked snapshots, ADR-0003 Decision 6 pattern).
- `snapshot` and `rollback` rows where they ran to completion.

**Abandoned** (not in this baseline; criterion estimated > 38 minutes per single 10-sample row):

- `combined_workflow/100x` — attempted, abandoned mid-criterion-estimation.
- `demo_path::load_canonical_inputs/100x` — attempted, abandoned mid-criterion-estimation.

**Env-gated** (`MC_BENCH_CONSOL_SCALED=1`; not run for the baseline):

- `leaf_read_write::write_input_leaf/50x` and `/100x`.
- Any other 50× / 100× row whose per-iteration setup is dominated by a multi-second bulk-load (the gate avoids replicating that cost on every criterion sample).

This is **a sufficient baseline for Phase 2D scoping** (the §6.14 cliff is fully captured by the 10× → 50× `load_canonical_inputs` jump that *did* run), **not** a comprehensive production-workload benchmark. Phase 2D should opt into the env-gated rows after its source change to confirm the cliff closes at 50× and 100×.

## What's in here

A `target/criterion/` snapshot scoped to the post-Phase-2C kernel:

```
phase-2c/
├── README.md                      this file
├── <bench>/<id>/                  one dir per benched row
│   ├── benchmark.json             criterion's static metadata
│   ├── estimates.json             median + range + slope
│   ├── sample.json                per-sample iteration counts + times
│   └── tukey.json                 outlier thresholds
└── ...
```

No `raw.csv` (criterion is `default-features = false` per Phase 1B's pin policy).

Phase 2C added these row groups not present in `phase-2a/` or `phase-2b/`:

- `<bench>_10x` / `_50x` / `_100x` variants of `write_input_leaf`, `read_input_leaf_warm`, `read_input_leaf_cold`, `read_derived_leaf_cold/Revenue`, `consolidation_cold` (27-leaf Spend, 27-leaf Revenue, 420-leaf Spend), `bench_load_canonical_inputs`, `snapshot`, `rollback`.
- `combined_workflow/50x` and `combined_workflow/100x` — new file, simulates a 100-iteration planner session with stacked snapshots held live across the session per ADR-0003 Decision 6.

## How to use this baseline (Phase 2D and later)

Per the parent [`../README.md`](../README.md), the standard workflow:

```bash
# From repo root, restore the phase-2c baseline locally:
for bench in $(ls docs/reports/bench-data/phase-2c/); do
  [ "$bench" = "README.md" ] && continue
  mkdir -p "crates/mc-core/target/criterion/$bench"
  cp -R "docs/reports/bench-data/phase-2c/$bench/." "crates/mc-core/target/criterion/$bench/"
done

# Apply your Phase 2D source change (or whatever the next sub-phase touches).

# Diff against the saved baseline:
cargo bench -p mc-core --bench <name> -- --baseline phase-2c
```

Criterion will print an `Improved.` / `Regressed.` line + percentage per row, comparing your run to this snapshot.

## Headline finding (load-bearing for Phase 2D)

**Cross-scale `load_canonical_inputs` super-linear cliff between 10× and 50×** (4.33×/write at 10× → 19.7×/write at 50×; total ingest at 50× is 230.84 s = 23× over the ADR-0003 patience-limit gate). The within-session `combined_workflow/50x` per-edit-amortized cost is *flat* across a 100-iteration session (≈ 434 → 430 → 439 µs amortized; computed as `edit_time ÷ dirty_delta` per the bench's `eprintln!` — see PERF.md §6.13.2 unit caveat where the divisor unit is labeled "ns" but with `edit_time ≈ 2.1 ms` and `dirty_delta ≈ 5` the result magnitude is µs, not ns). Within-session flatness is *consistent with* §9.3 once the dirty set is saturated, but does not isolate the AHashSet-insert component on its own. **The load-bearing §9.3 evidence — and the load-bearing input to Phase 2D's pick — is the cross-scale ingest cliff**, not the within-session number. The Phase 2C completion report explicitly does not pick a winner; PERF.md §6.14 lays out the Branch A / B / C comparison and Phase 2D's handoff makes the call.

## Cross-links

- [`../README.md`](../README.md) — workflow + per-tag baseline table.
- [`../phase-2c-completion-report.md`](../phase-2c-completion-report.md) — Phase 2C audit; §3 reproduces these numbers in tabular form.
- [`../../PERF.md`](../../PERF.md) §6.12 / §6.13 / §6.14 — interpretive tables built from this baseline.
- [`../../decisions/0003-workload-sketch.md`](../../decisions/0003-workload-sketch.md) — the ADR this baseline was calibrated against.
- [`../../handoffs/phase-2c-handoff.md`](../../handoffs/phase-2c-handoff.md) — the contract that defined what's measured here.
