# `bench-data/phase-2c/` — Production-Shaped Workload Baseline

> **Status (when this directory ships):** populated by `cargo bench --save-baseline phase-2c` against the post-Phase-2C HEAD. Captures all Phase 1B + Phase 2A + Phase 2B + Phase 2C bench rows including the new 10× / 50× / 100× scaled-Acme variants and the combined-workflow bench. This README is the per-baseline metadata; the cross-baseline workflow lives in [`../README.md`](../README.md).

## Captured at

- **Tag (prospective):** `phase-2c-workload-baseline` (final tag at project owner's discretion at commit time)
- **Commit:** `<TODO: hash on tag>` — uncommitted at the time of this README draft
- **Date:** 2026-05-02
- **Machine:** Apple M4, macOS 26.3 (matches phase-2a + phase-2b machine; comparable medians)
- **Toolchain:** Rust 1.78 (unchanged from prior baselines)
- **Cargo.lock pins:** `clap → 4.4.18`, `clap_lex → 0.6.0`, `half → 2.4.1` (unchanged from Phase 1B)
- **Sample size:** `--sample-size 10` (criterion's minimum) for the gate run; phase-2a + phase-2b ran at sample-size 100. Wider confidence intervals on this baseline; the `--baseline-lenient` diff against phase-2b for unchanged 1× rows accommodates this. See PERF.md §6.12 prologue.

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

**Per-mark cost on the hierarchy ancestor mark walk is FLAT across a 100-iteration session at 50× Acme** (434 → 430 → 439 ns at iters 1 / 50 / 100, ≤ 3% spread across 3 independent session samples). This *constrains* PERF.md §9.3's hypothesis — within a session at 50× scale, the dirty set growing from 0 to 305,039 entries does not measurably change per-mark cost. Cross-scale per-mark cost growth (1× → 10× → 50× → 100×) is what the isolated `write_input_leaf/{10,50,100}x` rows here measure; **Phase 2D's §9.3 vs §9.2 priority call reads from PERF.md §6.12.1 (cross-scale shape)**, not from this within-session finding. The Phase 2C completion report explicitly does not pick a winner.

## Cross-links

- [`../README.md`](../README.md) — workflow + per-tag baseline table.
- [`../phase-2c-completion-report.md`](../phase-2c-completion-report.md) — Phase 2C audit; §3 reproduces these numbers in tabular form.
- [`../../PERF.md`](../../PERF.md) §6.12 / §6.13 / §6.14 — interpretive tables built from this baseline.
- [`../../decisions/0003-workload-sketch.md`](../../decisions/0003-workload-sketch.md) — the ADR this baseline was calibrated against.
- [`../../handoffs/phase-2c-handoff.md`](../../handoffs/phase-2c-handoff.md) — the contract that defined what's measured here.
