# 1M Cell Write Path Diagnostic

Date: 2026-05-06  
Branch: `phase-3i/formula-language-completion`  
HEAD: `548eb6b docs: Phase 3I handoff - formula language completion`  
Toolchain: `rustc 1.78.0`, `cargo 1.78.0`  
Hardware: Mac mini `Mac16,10`, Apple M4, 10 cores, 16 GB RAM, macOS 26.3 build 25D125

Important repo-state note: the worktree was dirty before this report was written, including uncommitted changes in `crates/mc-core/src/cube.rs`, `crates/mc-core/src/rule.rs`, `crates/mc-model/*`, and CLI tests. The numbers below characterize the current working tree, not a clean tagged commit.

## 1. Current Numbers

The current "~9 seconds for 1M cells" number maps to **Path A: one million sequential `Cube::write` calls**, not to Tessera CSV ingestion and not to the bulk `WriteBatch` path.

### Path A: per-cell `Cube::write`

Command:

```bash
MC_BENCH_BASELINE_HEAVY=1 cargo bench -p mc-core --bench baseline_writebatch -- per_cell/1M
```

Benchmark: `baseline_writebatch/per_cell/1M` in `crates/mc-core/benches/baseline_writebatch.rs`.

Shape and path:

| Item | Value |
|---|---|
| Fixture | `build_scaled_acme_cube_100x` |
| Dimensions | Scenario, Version, Time, Channel, Market, Measure |
| Cardinalities | Scenario 3, Version 3, Time 17, Channel 8, Market 715 total / 707 leaves, Measure 11 |
| Theoretical coord space | 9,626,760 cells including consolidated elements and all scenarios/versions |
| Store shape | Sparse store, not dense array |
| Distinct write coords | 254,520 input leaf coords: 1 scenario x 1 version x 12 months x 5 channels x 707 market leaves x 6 input measures |
| 1M workload | Repeats that coordinate stream about 3.93 times |
| Setup | Canonical inputs loaded; dependency graph and derived caches materialized before timing |
| Timed operation | 1M calls to `Cube::write(WritebackRequest { intent: Set, ... })` |
| Recompute included? | No post-write reads. Timing includes validation, store write, revision bump, dependency dirtying, hierarchy dirtying, and `WritebackResult.invalidated` construction |
| Tessera included? | No. Pure mc-core API |

Criterion used Flat sampling and collected 10 samples, each with 3 full 1M-write passes. Per-pass wall-clock from `sample.json`:

| Metric | Seconds | Per cell |
|---|---:|---:|
| Min | 10.254 | 10.25 us |
| Median | 10.473 | 10.47 us |
| Max | 10.570 | 10.57 us |
| Criterion mean estimate | 10.430 | 10.43 us |
| 95% CI, mean | 10.360 - 10.497 | 10.36 - 10.50 us |

This is slightly slower than the historical PERF.md §6.16 row (`9.10 s` median, `9.52 s` mean). Because the worktree is dirty, this should be treated as a current diagnostic number, not a regression claim.

### Path B: `WriteBatch::commit`

Command:

```bash
MC_BENCH_TESSERA_HEAVY=1 cargo bench -p mc-core --bench tessera_writeback -- commit/1M
```

Benchmark: `write_batch/commit/1M` in `crates/mc-core/benches/tessera_writeback.rs`.

This uses the same 100x Acme setup and stages 1M cells before timing a single `WriteBatch::commit()` per iteration. It is still pure API benchmarking: it does not include CSV parsing, driver fetch, recipe transform, or Tessera audit sidecars.

| Metric | Seconds | Per cell |
|---|---:|---:|
| Min | 3.557 | 3.56 us |
| Median | 3.595 | 3.60 us |
| Max | 4.923 | 4.92 us |
| Criterion mean estimate | 3.823 | 3.82 us |
| 95% CI, mean | 3.584 - 4.153 | 3.58 - 4.15 us |

Criterion reported 2 high-severe outliers among 10 measurements. The median still clears the Phase 5A 1M target of <= 5 seconds.

### Path C: Tessera end-to-end ingestion

No existing 1M-row end-to-end Tessera benchmark was found. The closest current full-pipeline gate is `crates/mc-tessera/tests/perf_100k_sqlite.rs`, a 100K-row SQLite `Tessera::apply` release test with a 3-second ceiling. That is useful evidence for full ingestion overhead, but it is not a 1M-cell CSV/Tessera workload and was not run for this diagnostic.

## 2. Profile Breakdown

Target profiled: `baseline_writebatch/per_cell/1M` while Criterion was running Path A.

`cargo flamegraph` was not available:

- `cargo flamegraph --version` returned "no such command".
- `cargo install flamegraph` failed because current `flamegraph 0.6.12` requires Rust 1.86.
- `cargo install flamegraph --version 0.6.11` and `--locked` also failed on this pinned Rust 1.78 toolchain due edition-2024 dependencies.

Fallback used:

```bash
sample <bench-pid> 30 -file /tmp/baseline_writebatch_1m.sample.txt
```

Saved artifact: `docs/research/perf-1M-cell-flamegraph.svg`. It is a labelled macOS `sample` top-of-stack SVG fallback, not a true flamegraph.

Profile facts:

| Item | Value |
|---|---|
| Samples | 25,434 thread samples |
| Sample interval | 1 ms |
| Physical footprint | 623.1 MB |
| Peak footprint | 756.2 MB |
| Main benchmark loop | `criterion::bencher::Bencher::iter_custom` had 25,260 samples |
| Main write path | `mc_core::cube::Cube::write` had 18,303 inclusive samples, about 72.0% of all samples |

Top stack samples:

| Rank | Function | Samples | Share |
|---:|---|---:|---:|
| 1 | `Cube::compute_dirty_ancestors` | 4,704 | 18.5% |
| 2 | `_xzm_free` | 3,212 | 12.6% |
| 3 | `DirtyTracker::mark` | 2,678 | 10.5% |
| 4 | `_xzm_xzone_malloc_tiny` | 2,590 | 10.2% |
| 5 | `DirtyTracker::is_dirty` | 2,008 | 7.9% |
| 6 | `_platform_memmove` | 1,539 | 6.1% |
| 7 | `_platform_memset` | 1,262 | 5.0% |
| 8 | `_free` | 937 | 3.7% |
| 9 | `_xzm_xzone_malloc` | 847 | 3.3% |
| 10 | `libsystem_malloc` deduplicated symbol | 774 | 3.0% |

Top inclusive/total-time signals visible in the `sample` call graph:

| Function/path | Samples | Share | Notes |
|---|---:|---:|---|
| `criterion::bencher::Bencher::iter_custom` | 25,260 | 99.3% | Benchmark loop |
| `Cube::write` | 18,303 | 72.0% | Timed write path |
| `Cube::compute_dirty_ancestors` branches under `Cube::write` | at least 16,000 visible | at least 63% | `sample` splits by instruction address; this is the dominant nested operation |
| Allocator/free/memmove/memset frames under dirty ancestor work | thousands of samples | large | Allocation and memory movement are prominent in the per-cell path |
| `DirtyTracker::mark` | 2,678 self | 10.5% | Dirty bitset mark path |
| `DirtyTracker::is_dirty` | 2,008 self | 7.9% | Marginal invalidated check |
| `Hierarchy::ancestors` | 363 self | 1.4% | Ancestor lookup appears, but below allocation and dirty tracker costs |
| `HashMapStore::write` | 300 self | 1.2% | Store write/hash path is not the top stack item |
| `DependencyGraph::dependents_of` | 195 self | 0.8% | Rule dependent lookup |
| `DependencyGraph::closure_of_dependents` | 126 self | 0.5% | Dependency closure |

What this profile actually shows: Path A is dominated by dirty ancestor construction plus allocator/free/memory-movement work around that construction. Hardware counter data was not collected, so cache-miss percentages are not available. Hashing/store-write frames are present but not dominant in this sample.

## 3. Path Identification

The owner-visible "~9 seconds" is **Path A**:

- 1M iterations of `Cube::write`.
- Full dirty propagation per write.
- Per-write `WritebackResult.invalidated` materialization.
- No Tessera.
- No post-write recompute/read.

Path A current median is 10.47 us/cell at 1M. The Phase 2D completion report records `write_input_leaf` at 10.77 us and `write_input_leaf/10x` at 15.69 us after the bitset + marginal-invalidated correction. I did not find a current Phase 2D report row claiming 21 us/cell for the relevant 50K/50x write path; the nearby documented gate is `load_canonical_inputs/50x = 1.06 s` and `write_input_leaf = 10.77 us`.

If one extrapolated 21 us/cell to 1M, it would predict about 21 seconds. That is not what current Path A shows; current Path A is about 10.5 seconds. Historical PERF.md Path A was about 9.1 seconds median. Current Path B is about 3.6 seconds median and already under the 5-second target.

If the performance target is "make the per-cell `Cube::write` path under 5 seconds for 1M writes", that is still open. If the target is "make 1M bulk import commit under 5 seconds", the existing `WriteBatch::commit` path already does that on this machine. If the target is "make 1M full Tessera CSV/recipe/apply under 5 seconds", there is no existing 1M end-to-end benchmark yet to diagnose.

## 4. What's Changed Since Phase 2D

- Phase 2D changed the write path materially. It added the bitset-backed dirty tracker and corrected `WritebackResult.invalidated` to marginal semantics. The Phase 2D report says this moved `write_input_leaf` from 167 us to 10.77 us and `load_canonical_inputs/50x` from 230.80 s to 1.06 s.
- Phase 5A Stream A landed `WriteBatch`. Current code has `crates/mc-core/src/batch.rs`, `Cube::batch_apply_validated`, and `Cube::mark_dirty_ancestors_inline`. PERF.md §6.17 records the original 1M `WriteBatch::commit` pass at 3.83 s median; this diagnostic reproduced a 3.60 s median on the current worktree.
- Phase 5A Stream C added source drivers outside `mc-core`. That can add ingestion overhead in Path C, but it is not in the Path A or Path B benches. The current repo has a 100K SQLite Tessera perf test; no 1M full-ingestion benchmark was found.
- Phase 5C appears present as a handoff, not as a completion report in `docs/reports/`. Its planned driver expansion/scheduling work is outside the mc-core benchmark path.
- Phase 6A changed CLI/model loading behavior. Current `crates/mc-cli/src/loader.rs` replays canonical inputs, active Tessera imports, and post-hoc writes for "current reality" loads. That affects CLI query/trace/what-if observed state. It does not change the `baseline_writebatch` or `tessera_writeback` benchmark path.
- Phase 6A.1 and the current dirty Phase 3I work do touch `mc-core/src/cube.rs` and `rule.rs`, mainly formula/modeling behavior. I did not find evidence that they changed the `WriteBatch::commit` trigger conditions. Because the worktree is dirty, use these numbers as current-state diagnostics, not clean-tag baselines.

## 5. Known Inefficiencies Not Yet Addressed

Evidence-backed items only:

| Item | Evidence | Status |
|---|---|---|
| Path A allocates/materializes dirty ancestors per write | `Cube::write` calls `compute_dirty_ancestors`, which returns a `Vec<CellCoordinate>` used to build `WritebackResult.invalidated`; `sample` shows this path dominates | Open for per-cell writes |
| Path B did not ship prefix grouping | PERF.md §6.17.3 says grouping staged writes by `(scenario, version, time, channel, market)` did not ship; estimated 0.5-1 us/cell possible savings | Open, design/complexity tradeoff |
| Snapshot clone floor for small batches | PERF.md §6.17.4 says `write_batch/commit/1K` misses due snapshot clone of the materialized 100x cube | Open; COW snapshot remains deferred |
| Parallel apply/rayon ADR reference drift | `crates/mc-core/src/batch.rs` says rayon is gated behind "still-unwritten ADR-0012", but `docs/decisions/0012-*` now exists for Phase 3F time-series operations | Open doc/design drift; no current parallel plan |
| Cross-coordinate dependencies not in dep graph | Phase 6A.1 report lists this as P1 performance debt: correctness covered by bulk invalidation, but writes may over-invalidate | Open; ADR required by that report |
| Sweep reloads/parses model repeatedly | Phase 6A.1 report lists sweep reloads/parses YAML 2N times for N sweep points | Open CLI performance debt, not in 1M write benches |

Searches for `TODO: perf` and `FIXME: slow` in `crates/mc-core/src/` did not surface a direct additional marker beyond the documented PERF/report debt above.

## 6. Workload Representativeness

The 1M write target is a scaled synthetic Acme workload. It is Acme-shaped: same six dimensions, same rules, same hierarchy pattern, and market widened to 707 leaf markets. It is not the base Acme fixture, which has 2,520 canonical input cells.

Representativeness:

| Workload | Represented? | Notes |
|---|---|---|
| Base Acme | Partially | Same semantics, but 100x market scale and repeated writes |
| Tide Cleaners | Not verified | No 1M Tide cartridge benchmark was identified in this diagnostic |
| NBA cartridge | Not verified | No 1M NBA cartridge benchmark was identified |
| Forecasted production bulk import scale | Yes, as a synthetic stress gate | ADR-0003 discusses 1M-cell bulk import as the patience-limit scale |
| Specific owner complaint | Partially | The owner target references 1M writes from about 9 seconds to under 5; current evidence says that number is Path A, while Path B is already under 5 |
| Full CSV/Tessera ingestion | Not yet | Existing full pipeline perf evidence is 100K SQLite, not 1M CSV/Tessera |

Diagnostic conclusion: before choosing an optimization research question, decide whether the target path is Path A per-cell `Cube::write`, Path B `WriteBatch::commit`, or Path C full Tessera ingestion. The bottleneck profile gathered here is valid for Path A. It should not be generalized to CSV parsing, recipe transforms, driver fetch, audit sidecars, or CLI model loading without a separate Path C measurement.

## Commands Run

```bash
git status --short
git log --oneline -n 8
git branch --show-current
rustc --version
cargo --version
sw_vers
system_profiler SPHardwareDataType
MC_BENCH_BASELINE_HEAVY=1 cargo bench -p mc-core --bench baseline_writebatch -- per_cell/1M
cargo flamegraph --version
cargo install flamegraph
cargo install flamegraph --version 0.6.11
cargo install flamegraph --version 0.6.11 --locked
pgrep -fl baseline_writebatch
sample <pid> 30 -file /tmp/baseline_writebatch_1m.sample.txt
MC_BENCH_TESSERA_HEAVY=1 cargo bench -p mc-core --bench tessera_writeback -- commit/1M
rg ... docs/PERF.md docs/reports docs/handoffs docs/decisions crates/
```

Files created by this diagnostic:

- `docs/research/perf-1M-cell-diagnostic.md`
- `docs/research/perf-1M-cell-flamegraph.svg`
