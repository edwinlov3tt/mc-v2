# `bench-data/phase-2d/` — Bitset Tracker + WritebackResult.invalidated correction baseline

> **Status:** populated by `cargo bench -p mc-core --bench <name> -- --save-baseline phase-2d --sample-size 10` against the post-Phase-2D HEAD. Captures every Phase 1B + 2A + 2B + 2C bench row plus the `load_canonical_inputs/100x` row that was abandoned in phase-2c (now runs in 2.13 s under the corrected semantics). This README is the per-baseline metadata; the cross-baseline workflow lives in [`../README.md`](../README.md).

## Captured at

- **Tag:** `phase-2d-bitset-and-invalidated-fix`
- **Commit:** `0678a98` — *phase-2d: bitset DirtyTracker + WritebackResult.invalidated semantic fix*
- **Date:** 2026-05-02
- **Machine:** Apple M4, macOS 26.3 (matches phase-2a / 2b / 2c machine; comparable medians)
- **Toolchain:** Rust 1.78 (unchanged from prior baselines)
- **Cargo.lock pins:** `clap → 4.4.18`, `clap_lex → 0.6.0`, `half → 2.4.1` (unchanged from Phase 1B)
- **Sample size:** `--sample-size 10` (criterion's minimum) — matches phase-2c so `--baseline phase-2c` diffs are apples-to-apples

## Headline finding

> **`load_canonical_inputs/50x`: 230.80 s → 1.06 s (−99.5 %)** — the Phase 2D acceptance gate (≤ 50 s) beat by ~47×. **`load_canonical_inputs/100x`: 2.13 s** (was abandoned at >38 min in phase-2c). Combined-workflow per-mark amortized: ≈ 422 µs → ≈ 2.05 µs at 50× (~200× faster); within-session shape stays flat (3.7 → 2.06 → 2.05 µs at iter 1 / 50 / 100). See [`../../../PERF.md`](../../../PERF.md) §6.15 for the full table + A/B isolation result + spec audit.

## What changed vs phase-2c

Two source changes per the [Phase 2D handoff §A](../../../handoffs/phase-2d-handoff.md):

1. **Bitset-backed `DirtyTracker`.** Internal repr replaced with a Cartesian-product flat bitset behind `Arc<CubeShape>`; public method signatures preserved byte-for-byte. New `pub(crate) fn with_shape(Arc<CubeShape>)` constructor. See [`crates/mc-core/src/dirty.rs`](../../../../crates/mc-core/src/dirty.rs) and [`crates/mc-core/src/cube_shape.rs`](../../../../crates/mc-core/src/cube_shape.rs).
2. **`WritebackResult.invalidated` semantic correction.** The field's *contents* changed from "cumulative dirty set" (Phase 1A misreading of brief line 1938's compact pseudocode shorthand) to "marginal coords transitioned clean → dirty by this single write" (matches the brief's type doc + engine-semantics.md §13 + I-WB-7). The struct, field name, type, and re-export are unchanged. See [`crates/mc-core/src/cube.rs`](../../../../crates/mc-core/src/cube.rs) lines ~892–943 + the spec audit in [`../../../PERF.md`](../../../PERF.md) §6.15.4.

A/B isolation per handoff §A.5 (recorded in PERF.md §6.15.3): the bitset alone moves the gate row by **< 5 % at every measured scale** (within criterion noise). The writeback semantic correction is the load-bearing change. The bitset still ships as the structural foundation that makes the corrected per-write `is_dirty` check O(1).

## Captured / new / unchanged rows

**Captured rows that improved dramatically** (compared to phase-2c):

- `demo_path/load_canonical_inputs` at **1× / 10× / 50×** — −91.1 % / −97.9 % / −99.5 %.
- `leaf_read_write/write_input_leaf` and `write_input_leaf/10x` — −93.8 % / −97.7 %.
- `dirty_propagation/spend_at_anchor` — −93.0 %.

**New rows captured for the first time** (abandoned in phase-2c):

- `demo_path/load_canonical_inputs/100x` — 2.13 s (no phase-2c baseline to diff against).

**Captured rows that stayed within noise** (no Phase 2D effect on the read path):

- `read_input_leaf_warm` — 47.93 ns (was ~50 ns).
- `read_input_leaf_cold` — improved as a free side-effect (−57 %; the per-iter setup is now much faster).
- `read_derived_leaf_warm` / `_cold` — within ±10 %.
- `consolidation_warm` / `_cold` rows — unchanged within noise.
- `snapshot` / `rollback` rows — unchanged within noise.

**Combined workflow** (preflight numbers; criterion side is a noop marker):

- `combined_workflow/50x` per-edit p50 / p95 / p99: 11.1 / 19.9 / 24.0 µs (was ≈ 2.4 ms). Per-mark amortized at iter 1 / 50 / 100: 3.7 / 2.06 / 2.05 µs (was ≈ 422 / 419 / 422 µs). Within-session flat shape preserved.
- `combined_workflow/100x` — still env-gated behind `MC_BENCH_COMBINED_WORKFLOW_100X=1` (preflight at this scale is ~30 min wall-clock per Phase 2C completion report §4.4).

**`raw.csv` files are pruned** from this baseline to keep checked-in size small (phase-2d directory is ~1 MB without raw.csv vs ~50 MB with). Re-run `cargo bench` against the `phase-2d` tag if you need the raw per-iteration timings.

## How to use

For a Phase 2E (or later) optimization wanting a real before/after diff against this baseline:

```bash
# Restore the saved baseline JSON locally so criterion's --baseline flag finds it:
for bench in $(ls docs/reports/bench-data/phase-2d/); do
  [ "$bench" = "README.md" ] && continue
  mkdir -p "crates/mc-core/target/criterion/$bench/phase-2d"
  cp -R "docs/reports/bench-data/phase-2d/$bench/." "crates/mc-core/target/criterion/$bench/phase-2d/"
done

# Apply your source change.
# Run benches with --baseline phase-2d for the diff:
cargo bench -p mc-core --bench <name> -- --baseline phase-2d
```

Save the new baseline if your change is being shipped:

```bash
cargo bench -p mc-core -- --save-baseline phase-2e --sample-size 10
mkdir -p docs/reports/bench-data/phase-2e
# mirror per the workflow at the top of ../README.md, then:
find docs/reports/bench-data/phase-2e -name 'raw.csv' -delete
```

See [`../README.md`](../README.md) for the cross-baseline policy.
