# `bench-data/` — Per-tag criterion baselines

This directory holds criterion's `target/criterion/` JSON output captured at named tags so future optimization sub-phases can produce real before/after diffs via `cargo bench -- --baseline <name>` instead of hand-edited PERF.md tables. Per [`../../roadmap/MASTER_PHASE_PLAN.md`](../../roadmap/MASTER_PHASE_PLAN.md) "Phase 2 housekeeping → Q3" and the Phase 2B completion report §6.A.

## Layout

```
docs/reports/bench-data/
├── README.md                     this file
├── phase-2a/                     baseline at the `phase-2a-cold-path-baseline` tag
│   └── <bench>/<id>/
│       ├── benchmark.json        criterion's static metadata for the row
│       ├── estimates.json        median + range + slope estimates
│       ├── sample.json           per-sample iteration counts + times
│       ├── tukey.json            outlier thresholds (Tukey method)
│       └── raw.csv               raw per-iteration timings (large; pruned)
└── phase-2b/                     baseline at the `phase-2b-consolidation-fast-path` tag
    └── <bench>/<id>/             same shape as phase-2a/
```

`raw.csv` files are pruned from the committed snapshot — they're large
(per-iteration timings × thousands of iterations × every bench row),
machine-noisy, and `estimates.json` + `sample.json` are sufficient for
criterion's `--baseline` diff. If a future investigation needs the raw
samples, re-run `cargo bench` against the relevant tag.

## How to use

To run a Phase 2C (or later) optimization with a real before/after diff:

1. Confirm the relevant baseline JSON is in this directory. Each baseline
   should match a tag in `git tag --list "phase-*"`.
2. Symlink or copy the baseline back into `target/criterion/`:

   ```bash
   # From repo root, restore the baseline criterion expects locally:
   for bench in $(ls docs/reports/bench-data/phase-2b/); do
     mkdir -p "target/criterion/$bench"
     cp -R "docs/reports/bench-data/phase-2b/$bench/." "target/criterion/$bench/"
   done
   ```

3. Apply your source change.
4. Run the bench(es) with `--baseline phase-2b` to get the diff:

   ```bash
   cargo bench -p mc-core --bench consolidated_read -- --baseline phase-2b
   ```

   Criterion will print an `Improved.` / `Regressed.` line + percentage
   per row, comparing the new run to the saved baseline.

5. Save the new baseline if your change is being shipped:

   ```bash
   cargo bench -p mc-core -- --save-baseline phase-2c
   mkdir -p docs/reports/bench-data/phase-2c
   cp -R target/criterion/* docs/reports/bench-data/phase-2c/
   # prune raw.csv before committing if size matters:
   find docs/reports/bench-data/phase-2c -name 'raw.csv' -delete
   ```

## What's captured per tag

| Tag | Captured | Purpose |
|---|---|---|
| `phase-2a-cold-path-baseline` | All Phase 1B + Phase 2A bench rows at the pre-Phase-2B kernel | Establishes the cold-path baseline before the Arc fast path. Diffing `phase-2b` against `phase-2a` reproduces PERF.md §6.11's before/after numbers via criterion's statistical bounds rather than document-asserted medians. |
| `phase-2b-consolidation-fast-path` | All Phase 1B + Phase 2A bench rows at the post-Arc-fast-path kernel | The forward baseline. Phase 2C and later optimizations run `--baseline phase-2b` to get an automatic diff against the current shipping kernel. |

## Why this directory exists

PERF.md §6.11 records Phase 2B's before/after as document-form medians.
Without checked-in criterion baselines, those numbers were
hand-recorded — not independently verifiable from the repository.
Phase 2B's completion report §6.A documents this as a deviation
("Q3 slipped"); this directory + the per-tag JSON it holds is the
closure.

The workflow's reusable rule: every optimization sub-phase from
Phase 2C onward must save its post-change baseline here as part of
the phase commit, and must diff against the previous phase's
baseline as part of the validation gate. ADR-0002 codified the
adjacent rule (perf assertions belong in benches, not in `cargo
test`); this directory is what makes the bench-side discipline
actually work over time.
