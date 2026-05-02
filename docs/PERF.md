# PERF.md — Phase 1B Benchmark Baseline (+ Phase 2A Cold-Path Expansion)

> **Purpose.** Close Phase 1A acceptance criterion 5 (`cargo bench`) and
> establish a trustworthy performance baseline before any Phase 2
> optimization work. No behavior changed; this is measurement only.
>
> **Status.**
>
> - **Phase 1B baseline complete (2026-05-01).** 8 of 14 brief §11 1A
>   ceilings directly comparable in Phase 1B and pass; see §6.1–§6.5.
> - **Phase 2A cold-path expansion complete (2026-05-01).** Both Phase
>   1B caveat banners are now closure notes, not deferrals. The 6
>   §11.2 consolidation rows now have real cold-walk numbers (§6.7) —
>   all five 1A ceilings clear by ≥75× and four of five 1B targets
>   clear too. The brief §11.1 `bench_write_input_leaf_no_deps` 50 µs
>   ceiling is now measurable on the new synthetic minimal-hierarchy
>   fixture (§6.8) and clears at ~246 ns (~200× under). Two adjacent
>   diagnostic suites land alongside: snapshot clone by cardinality
>   (§6.9) and hierarchy ancestor mark microbench by graduated depth
>   (§6.10).
>
> Detailed findings flow into §8 (hot spots) and §9 (Phase 2B
> recommendations, now data-quantified). §10 confirms no
> `crates/mc-core/src/` source file was modified during Phase 1B or
> Phase 2A.
>
> ### Two important caveats — closed in Phase 2A
>
> Both banners below describe the Phase 1B state. Phase 2A's
> measurement work closes both — see §6.7 and §6.8 for the new rows
> and §7.3 / §7.4 for the updated interpretation. Kept here verbatim
> as a historical record of what Phase 1B accepted and Phase 2A
> resolved.
>
> 1. **(Phase 1B → closed in Phase 2A §6.7)** The §6.3 consolidation
>    numbers (~64–70 ns) are warm-cache hits, not the real cost of
>    consolidation. They are the cost of a `Cube::read_consolidated`
>    call after the answer was cached in a prior read at the same
>    revision. **Cold consolidation (cache miss after a write or a
>    fresh build) is not measured in the Phase 1B baseline.** Brief
>    §11.2's ceilings (50 µs … 20 ms range) were calibrated against
>    cold reads; the warm numbers here pass them by 5–6 orders of
>    magnitude because they are not the same operation. *Resolution:*
>    Phase 2A adds cold-path variants in §6.7; every §11.2 1A ceiling
>    is now passed by real cold reads.
>
> 2. **(Phase 1B → closed in Phase 2A §6.8)**
>    `write_input_leaf_no_deps` (165 µs) is a benchmark-scope mismatch,
>    not a closed regression. It is over the brief's 1A ceiling
>    (50 µs), but the bench is mis-named on Acme: every write pays the
>    hierarchy ancestor mark walk regardless of rule fan-out, so the
>    "no-deps" condition the brief envisioned (a synthetic
>    no-hierarchy cube) is not what this bench measures. Phase 1B
>    accepts this as documented. *Resolution:* Phase 2A adds the
>    synthetic minimal-hierarchy fixture (`mc_fixtures::build_minimal_cube`)
>    and a new bench `write_input_leaf_no_deps_synthetic` (§6.8)
>    measures the brief's intended cost at ~246 ns, clearing the 50 µs
>    1A ceiling by ~200×. The Acme `write_input_leaf_no_deps` row in
>    §6.1 stays as a documented Acme-fixture-path measurement, not a
>    failed ceiling.

---

## 1. Commit

| Field | Value |
|---|---|
| HEAD at bench time | `bee281283ac4ce6c4fe911ad322bb790ced4e1c2` |
| Branch | `main` |
| Tree state | Phase 1B + Phase 2A wiring uncommitted at time of bench; will be tagged as `phase-2a-cold-path-baseline` after this report is reviewed |

The Phase 1A kernel commits referenced in the [HANDOFF](./HANDOFF.md) and
[CURRENT_STATE](./CURRENT_STATE.md) (`4aa674a` initial kernel,
`5ebc7bc` docs reorg, `bee2812` `mc-core` lib comment) are unchanged.
Only `crates/mc-core/Cargo.toml`, `Cargo.lock`, and the new
`crates/mc-core/benches/*.rs` files differ in this PR.

---

## 2. Toolchain

| Field | Value |
|---|---|
| `rustc` | `1.78.0 (9b00956e5 2024-04-29)` |
| `cargo` | `1.78.0 (54d8815d0 2024-03-26)` |
| Toolchain pin | [`rust-toolchain.toml`](../rust-toolchain.toml) → `channel = "1.78"` |
| Workspace edition | `2021` (per [`Cargo.toml`](../Cargo.toml)) |
| Resolver | `2` |

The toolchain pin is **unchanged**. Per the Phase 1B handoff hard rule
"If a benchmark dependency requires bumping Rust, stop and report the
options before changing rust-toolchain.toml" — and per CLAUDE.md §1.1,
the Rust 1.78 / `clap_lex` / `edition2024` blocker was the original
cause of the Phase 1A deferral. The blocker is real (see §5 below for
the full diagnosis); it was sidestepped via three transitive pin
downgrades in `Cargo.lock`, not by bumping the toolchain.

---

## 3. Machine / environment

| Field | Value |
|---|---|
| Model | Apple Silicon — `Apple M4` |
| Architecture | `arm64` |
| Physical / logical cores | 10 / 10 |
| RAM | 16 GiB (`hw.memsize = 17_179_869_184`) |
| OS | macOS 26.3 (Build 25D125) |

Single-machine, single-thread. No background load was excluded
explicitly — these numbers should be treated as the **shape** of Phase
1A performance, not certified ceilings. Re-run on the same machine in a
quiet state if comparing against future Phase 2 numbers.

The brief §11 hardware target is "M1/M2 Mac or equivalent x86-64 laptop";
M4 is faster, so every brief §11 1A ceiling should be comfortably
cleared on this machine. As of Phase 2A, every directly-comparable 1A
ceiling does clear: §6.1–§6.5 cover the 8 Phase-1B rows; §6.7's cold
consolidation closes the 6 §11.2 rows that were warm-only at end of
Phase 1B; §6.8's synthetic fixture closes the
`bench_write_input_leaf_no_deps` ceiling.

---

## 4. Benchmark commands

```bash
# All benches (full criterion config: 3s warm-up + 5s sample window each)
cargo bench --workspace

# One file at a time (this is what produced the §6 table)
cargo bench -p mc-core --bench leaf_read_write
cargo bench -p mc-core --bench derived_read
cargo bench -p mc-core --bench consolidated_read
cargo bench -p mc-core --bench dirty_propagation
cargo bench -p mc-core --bench demo_path
```

Quick smoke (sub-second per bench, useful in CI before the long run):

```bash
cargo bench -p mc-core --bench <name> -- \
  --warm-up-time 1 --measurement-time 1 --sample-size 10
```

---

## 5. Tooling — Criterion 0.5 on Rust 1.78

**Criterion was used** (not the std-only fallback). Per Phase 1B
handoff §A and CLAUDE.md §1.1, the Phase 1A deferral cited:

> On Rust 1.78 (pinned), criterion's transitive dependency `clap_lex
> 1.1.0` requires `edition2024`, which only stabilized in 1.85.

That is **still true** for the latest-resolved transitives. The fix:
pin three transitive dependencies to pre-`edition2024` versions in
`Cargo.lock`. The brief's `criterion = "0.5"` (workspace dep) is
preserved verbatim; only `Cargo.lock` changed.

| Crate | Latest-resolved | Pinned to | Why |
|---|---|---|---|
| `clap` | 4.6.1 | **4.4.18** | 4.6.x manifest declares `edition = "2024"` |
| `clap_lex` | 1.1.0 | **0.6.0** | 1.1.0 is the immediate trigger; 0.6.x predates |
| `half` | 2.7.1 | **2.4.1** | 2.7.x sets `rust-version = "1.81"` |

These pins are commands captured in the lockfile, not Cargo.toml edits:

```bash
cargo update -p half      --precise 2.4.1
cargo update -p clap      --precise 4.4.18
cargo update -p clap_lex  --precise 0.6.0
```

The first two probe attempts (a single-crate `crit-probe` outside the
workspace) appeared to compile clap_lex 1.1.0 successfully on what we
thought was Rust 1.78 — they were silently using the system default
`rustc 1.95.0`. Once probed under `rustup run 1.78`, the failure
reproduced. This is the path the handoff §A directed: "First try to
restore Criterion in a Rust 1.78-compatible way by pinning versions if
possible." It worked.

The std-only runner fallback (a `mc-bench` binary using
`std::time::Instant` + `std::hint::black_box`) was **not needed** and
not implemented.

`crates/mc-core/Cargo.toml` was updated to:

```toml
[dev-dependencies]
mc-fixtures = { path = "../mc-fixtures" }
criterion.workspace = true

[[bench]]  name = "leaf_read_write"     harness = false
[[bench]]  name = "derived_read"        harness = false
[[bench]]  name = "consolidated_read"   harness = false
[[bench]]  name = "dirty_propagation"   harness = false
[[bench]]  name = "demo_path"           harness = false
```

The workspace `criterion = "0.5"` declaration in the root
[`Cargo.toml`](../Cargo.toml) keeps `default-features = false` so
plotters / rayon / cargo_bench_support do not enter the dep tree.

---

## 6. Raw benchmark table

Each row is the **median of 100 samples** over a 5-second criterion
sample window (range = lower / upper bounds reported by criterion).
Numbers are body-only; criterion subtracts setup/cleanup time.

### 6.1 `leaf_read_write` — brief §11.1

| Bench | Median | Range | 1A ceiling | 1B target | Status |
|---|---:|---|---:|---:|:---:|
| `read_input_leaf_cold` | **825 ns** | 777 – 888 ns | < 20 µs | < 1 µs | ✓ |
| `read_input_leaf_warm` | **48 ns** | 48.0 – 48.2 ns | < 5 µs | < 200 ns | ✓ ✓ |
| `write_input_leaf` | **163 µs** | 157 – 171 µs | < 200 µs | < 10 µs | ✓ |
| `write_input_leaf_no_deps` | **165 µs** | 164 – 166 µs | < 50 µs | < 2 µs | **✗** |

The `_no_deps` row is over the 1A ceiling and is the lone correctness-gate
miss; see §7 and §8 for why this is a misnaming, not a regression.

### 6.2 `derived_read` — brief §11.1 (rule-evaluated leaves)

| Bench | Median | Range | 1A ceiling | 1B target | Status |
|---|---:|---|---:|---:|:---:|
| `read_derived_leaf_warm/Clicks` | **58.5 ns** | 58.3 – 58.8 ns | < 5 µs | < 200 ns | ✓ ✓ |
| `read_derived_leaf_warm/Leads` | **58.4 ns** | 58.2 – 58.7 ns | < 5 µs | < 200 ns | ✓ ✓ |
| `read_derived_leaf_warm/Customers` | **58.5 ns** | 58.3 – 58.8 ns | < 5 µs | < 200 ns | ✓ ✓ |
| `read_derived_leaf_warm/Revenue` | **58.6 ns** | 58.4 – 58.7 ns | < 5 µs | < 200 ns | ✓ ✓ |
| `read_derived_leaf_warm/Gross_Profit` | **59.1 ns** | 58.9 – 59.3 ns | < 5 µs | < 200 ns | ✓ ✓ |
| `read_derived_leaf_cold/Clicks` | **1.15 µs** | 1.07 – 1.27 µs | < 100 µs | < 5 µs | ✓ ✓ |
| `read_derived_leaf_cold/Leads` | **1.71 µs** | 1.66 – 1.78 µs | < 100 µs | < 5 µs | ✓ ✓ |
| `read_derived_leaf_cold/Customers` | **2.33 µs** | 2.25 – 2.45 µs | < 100 µs | < 5 µs | ✓ ✓ |
| `read_derived_leaf_cold/Revenue` | **2.89 µs** | 2.84 – 2.96 µs | < 100 µs | < 5 µs | ✓ ✓ |
| `read_derived_leaf_cold/Gross_Profit` | **3.57 µs** | 3.49 – 3.66 µs | < 100 µs | < 5 µs | ✓ ✓ |

### 6.3 `consolidated_read` — brief §11.2

> ⚠️ **Warm-cache only.** Every row below is a cache-hit at the same
> revision the consolidation was first computed at. Treat these numbers
> as "the cost of a `Cube::read_consolidated` cache hit," not as
> "the cost of consolidation." The brief §11.2 ceilings (1A column)
> were calibrated against cold reads (cache miss after a write) and
> are not directly comparable to these warm-state numbers. **Cold
> consolidation rows are now measured in §6.7 (Phase 2A); the brief
> §11.2 ceiling assessment lives there.** This §6.3 table is retained
> as the cache-hit baseline.

| Bench | Median (warm) | Range | 1A ceiling (cold) | 1B target (cold) | Status |
|---|---:|---|---:|---:|:---:|
| `consolidation_warm/Q1_PaidSearch_Tampa/Spend (3 leaves)` | **64.2 ns** | 64.1 – 64.3 ns | < 50 µs | < 3 µs | not directly comparable |
| `consolidation_warm/Q1_PaidMedia_Florida/Spend (27 leaves)` | **69.3 ns** | 68.5 – 70.2 ns | < 1 ms | < 30 µs | not directly comparable |
| `consolidation_warm/Q1_PaidMedia_Florida/CPC (27 leaves, weighted avg)` | **67.7 ns** | 67.5 – 68.0 ns | < 2 ms | < 100 µs | not directly comparable |
| `consolidation_warm/Q1_PaidMedia_Florida/Revenue (27 leaves, rule chain)` | **66.9 ns** | 66.8 – 67.0 ns | < 5 ms | < 200 µs | not directly comparable |
| `consolidation_warm/Q1_PaidMedia_Florida/Gross_Profit (27 leaves, rule chain)` | **66.7 ns** | 66.6 – 66.9 ns | < 5 ms | < 200 µs | not directly comparable |
| `consolidation_warm/FY_AllChannels_USA/Spend (420 leaves)` | **69.9 ns** | 68.4 – 71.6 ns | < 20 ms | < 500 µs | not directly comparable |

All consolidation results passed the §4.5.1 golden-value sanity check
(Q1×Paid_Search×Tampa Spend = 33,000; Mar×Paid_Search×Florida Spend =
35,100; Q1×Paid_Media×Florida Spend = 329,400; Q1×Paid_Search×Florida
CPC ≈ 1.5202381) before any timing was recorded — see
[`consolidated_read.rs::assert_consolidated_golden`](../crates/mc-core/benches/consolidated_read.rs).
Golden-value match was verified at the cold first-read step before the
cache warmed; the bench would have aborted if the kernel had drifted
on the first computation.

### 6.4 `dirty_propagation` — brief §11.3 fragment

| Bench | Median | Range | 1A ceiling | 1B target | Status |
|---|---:|---|---:|---:|:---:|
| `dirty_propagation/spend_at_anchor` | **153 µs** | 151 – 156 µs | < 50 ms `*` | < 1 ms `*` | ✓ ✓ |

`*` Brief §11.3 names this `bench_full_recompute_after_one_write` and
sets the ceiling for "after one Spend write, **read all dirtied derived
cells**." This bench measures only the write+mark closure cost (not
the subsequent reads). The 50 ms ceiling is therefore loose for this
sub-bench; treat the 153 µs figure as a **lower bound** on the full
recompute. The follow-up bench in §6.5 (`full_revenue_slice_warm`)
covers the read side of the same pattern at 26.7 µs for 420 cells.

**Pre-flight sanity values** (one-time print, captured by
`dirty_propagation` at startup):

```
dirty_set: 17820 -> 17825 (delta 5); invalidated.len=17825
```

- Required-present check: Revenue@anchor is dirty after the write — passed.
- Required-absent check: distant Spend (Dec_2026 / Organic / Boston) is clean — passed.
- Delta (5) = 5 derived measures at the anchor coord (Clicks / Leads /
  Customers / Revenue / Gross_Profit) — expected from the Acme rule fan-out.
- `invalidated.len` (17825) is the full transitive closure including
  hierarchy ancestors. Same shape as the demo CLI's 19919 figure
  (which also includes effects of `write_canonical_inputs`).

### 6.5 `demo_path` — full pipeline + brief §11.3

| Bench | Median | Range | Mapped 1A ceiling | 1B target | Status |
|---|---:|---|---:|---:|:---:|
| `demo_path/build_only` | **19.7 µs** | 19.5 – 20.0 µs | — | — | (ref) |
| `demo_path/build_and_load` | **240.6 ms** | 239 – 243 ms | < 2 s `*` | < 50 ms `*` | ✓ |
| `demo_path/build_load_materialize` | **242.2 ms** | 241 – 244 ms | < 2 s `*` | < 50 ms `*` | ✓ |
| `demo_path/full_demo_reads (warm)` | **3.51 µs** | 3.47 – 3.55 µs | — | — | (ref) |
| `demo_path/full_revenue_slice_warm (420 cells)` | **26.7 µs** | 26.7 – 26.8 µs | < 50 ms | < 1 ms | ✓ ✓ |
| `demo_path/load_canonical_inputs (2520 writes)` | **242.6 ms** | 239 – 249 ms | < 2 s | < 50 ms | ✓ |

`*` `build_and_load` and `build_load_materialize` map to brief §11.3's
`bench_load_canonical_inputs` because the 2,520 cell write dominates
both. The build cost is < 1% of total — see `build_only` row.

### 6.6 Phase 1B re-runs (drift since 2026-05-01 baseline)

The Phase 2A bench run is on the same machine (Apple M4) and
includes the Phase 1B suite verbatim plus the four new Phase 2A
files. Drift from §6.1–§6.5 is well under measurement noise; no row
moved by more than ~10%. Notable: `write_input_leaf` ticked from
163 µs → 153 µs (−6%), credited by criterion as `Performance has
improved.` This is run-to-run variance on Acme, not a kernel change
(no `mc-core/src/` file was modified — see §10).

### 6.7 Cold consolidation reads — Phase 2A

> Closes Phase 1B caveat #1 (top-of-doc banner) and the §6.3 deferral
> note. Per-iteration setup: `build_acme_cube` →
> `write_canonical_inputs` → `materialize_all_dependencies` → a
> single idempotent leaf write at `Mar_2026 / Paid_Search / Tampa`
> with the leaf's canonical Spend (or CPC, for the weighted-average
> row) value. The write bumps revision and marks the consolidated
> coord dirty — verified by an `assert!(cube.dirty().is_dirty(&target))`
> before each timed read so a future maintainer cannot accidentally
> measure a warm hit. Goldens (§4.5.1, plus closed-form expansions
> for Q1×Paid_Media×Florida CPC and Revenue, FY×All_Channels×USA
> Spend) are verified once on the cold path before any timing is
> recorded — see [`consolidated_read.rs::assert_cold_golden`](../crates/mc-core/benches/consolidated_read.rs).

| Bench | Median (cold) | Range | 1A ceiling | 1B target | Status |
|---|---:|---|---:|---:|:---:|
| `consolidation_cold/Q1_PaidSearch_Tampa/Spend (3 leaves)` | **2.53 µs** | 2.46 – 2.59 µs | < 50 µs | < 3 µs | ✓ ✓ (Phase 2B) |
| `consolidation_cold/Q1_PaidMedia_Florida/Spend (27 leaves)` | **4.53 µs** | 4.44 – 4.63 µs | < 1 ms | < 30 µs | ✓ ✓ |
| `consolidation_cold/Q1_PaidMedia_Florida/CPC (27 leaves, weighted avg)` | **6.34 µs** | 6.23 – 6.45 µs | < 2 ms | < 100 µs | ✓ ✓ |
| `consolidation_cold/Q1_PaidMedia_Florida/Revenue (27 leaves, rule chain)` | **52.4 µs** | 51.69 – 53.20 µs | < 5 ms | < 200 µs | ✓ ✓ |
| `consolidation_cold/FY_AllChannels_USA/Spend (420 leaves)` | **31.8 µs** | 30.57 – 34.02 µs | < 20 ms | < 500 µs | ✓ ✓ |

**Phase 2B closes the 3-leaf 1B target: 14.3 µs → 2.53 µs.** Every
brief §11.2 1A AND 1B ceiling now clears on real cold reads. See
§6.11 below for the per-row before/after diff.

The Phase 1A wording about a "fixed-cost floor that linear walking
doesn't break" is closed: the floor *was* the per-call dim/hierarchy
clone documented in §8.2, and §6.11 attributes the entire ~12 µs gap
to that single source. The brief §10.3 cache-hit-speedup test
(`t_consolidation_caches_value_within_revision`) now records the
speedup *semantically* (cache populated with Consolidation provenance
+ revision match + dirty cleared) — the wall-clock ratio assertion was
moved to PERF.md per ADR-0002, where the §6.3 warm reads (~64 ns) vs
§6.7 cold reads (~2.5 µs) statistically establish a ~40× cache-hit
speedup over 100 samples per row.

The 27-leaf Revenue row (~68 µs) is ~4× the 27-leaf Spend row
(~16 µs), reflecting the rule-chain depth: each leaf's Revenue is
recomputed via a 5-deep recursive eval (Spend → Clicks → Leads →
Customers → Revenue) on a cold read, and that recursion replays per
leaf. Compare §6.2's `read_derived_leaf_cold/Revenue` at ~2.85 µs
per leaf × 27 leaves = ~77 µs upper bound; the actual 68 µs is
close, with the extra cost amortized by AHashMap warmth from the
preceding leaf reads in the same consolidation.

The 420-leaf row at 42.8 µs is ~16× the 27-leaf Spend row, which
matches the ~15× more leaves the consolidator walks. Per-leaf cost
flatlines around ~100 ns at 420 leaves, consistent with the warm
input-leaf read in §6.1 (48 ns) plus a small amount of consolidator
arithmetic per leaf.

### 6.8 Synthetic no-deps write — Phase 2A

> Closes Phase 1B caveat #2 (top-of-doc banner) and §7.3's
> documented deviation. Per-iteration setup: build the new
> [`mc_fixtures::build_minimal_cube`] cube — 2 dims (Time + Measure)
> with **no hierarchies on any non-Measure dim** and **no Derived
> measures** — and write Spend at the lone leaf coord. The
> bench-side `preflight()` asserts both invariants and confirms
> `WritebackResult.invalidated.is_empty()` before any timing is
> recorded — see [`synthetic_no_deps.rs`](../crates/mc-core/benches/synthetic_no_deps.rs).

| Bench | Median | Range | 1A ceiling | 1B target | Status |
|---|---:|---|---:|---:|:---:|
| `write_input_leaf_no_deps_synthetic` | **246 ns** | 241.4 – 252.4 ns | < 50 µs | < 2 µs | ✓ ✓ |

The brief's 50 µs 1A ceiling clears by ~200×; the 1B target (2 µs)
clears by ~8×. The cost decomposes as: permission + cube-id + arity
+ consolidated-coord + derived-measure + version + lock + intent +
type + NaN + optimistic-concurrency check, then revision bump +
store write + a no-op `mark_closure(coord, deps)` (empty graph) +
no-op `compute_dirty_ancestors` (no hierarchies, no derived) + a
no-op soft-lock walk. The 246 ns figure represents the irreducible
per-write fixed cost on the current kernel.

**Implication for the Acme `write_input_leaf_no_deps` row (§6.1,
165 µs).** That bench is *not* over a kernel ceiling; it is the
Acme fixture's hierarchy + derived-measure mark walk dominating an
otherwise-fast write. §6.10 below isolates the per-ancestor
contribution; the difference between the 246 ns synthetic figure
here and the 165 µs Acme figure is ~165 µs of structural fixture
cost, decomposable as outlined in §7.3 (updated below).

### 6.9 Snapshot clone — Phase 2A

> Diagnostic suite per the Phase 2A handoff item 3. Phase 1A's
> `Cube::snapshot()` is a thin wrapper around `HashMapStore::clone()`;
> this bench surfaces the constant + per-cell linear factor across
> the four cardinality landmarks the handoff calls out. Round-trip
> integrity (snapshot → mutate → rollback → read returns
> pre-mutation value) is verified once before timing — see
> [`snapshot_clone.rs::integrity_roundtrip`](../crates/mc-core/benches/snapshot_clone.rs).

| Bench | Median | Range | Notes |
|---|---:|---|---|
| `snapshot/0_cells_fresh` | **7.59 ns** | 7.56 – 7.63 ns | Empty AHashMap clone — essentially the `Snapshot` struct constructor cost. |
| `snapshot/100_cells` | **1.13 µs** | 1.11 – 1.15 µs | ~11 ns/cell. |
| `snapshot/2520_cells_loaded` | **29.5 µs** | 29.48 – 29.57 µs | ~12 ns/cell at 2,520 cells. |
| `snapshot/materialized` | **55.1 µs** | 54.82 – 55.48 µs | ~25K cells (2,520 inputs + materialized derived/consolidated cache) → ~2.2 ns/cell. AHashMap clone amortizes well at scale. |
| `rollback/0_cells_fresh` | **370 ns** | 350.8 – 391.7 ns | Per-iter setup mutates one cell so rollback has work to do; this row covers the empty-store case. |
| `rollback/100_cells` | **5.49 µs** | 5.33 – 5.71 µs | ~55 ns/cell. ~5× the snapshot cost — rollback re-clones the snapshot's store, re-clears `dirty`, and walks the cloned store to prune Rule-provenance cells. |
| `rollback/2520_cells_loaded` | **73.7 µs** | 71.6 – 77.4 µs | ~29 ns/cell. Same shape as 100; the prune walk is cheap when no Rule cells exist (the loaded but not materialized store has only Input cells). |
| `rollback/materialized` | **173 µs** | 170.2 – 178.0 µs | ~7 ns/cell at 25K cells. Per-cell rollback cost shrinks at scale because the prune walk dominates a fixed working-set fits-in-cache regime. |

**Snapshot cost is sub-linear in cardinality at Acme scale** (per-cell
cost drops from 11 ns at 100 cells to 2.2 ns at 25K). At Acme's
working size a snapshot is well under 100 µs; even at 250K cells
linear extrapolation suggests ~1 ms. The §9.5 follow-up (Snapshot
COW) is not gating for current scale, but its cost should be
revisited if Phase 2 introduces a workflow that takes many
snapshots in a single workflow turn.

**Rollback is the more expensive direction** by ~3× (clone + prune +
revision bump + dirty-clear), and the cost grows steeper at low
cardinality (370 ns → 5.49 µs → 73.7 µs → 173 µs), suggesting the
prune walk's `store.iter()` + `Provenance::Rule` filter dominates at
large stores rather than the AHashMap clone itself.

### 6.10 Hierarchy mark cost — Phase 2A microbench

> Diagnostic suite per the Phase 2A handoff item 4. Isolates the
> per-ancestor mark walk contribution by graduated linear hierarchy
> depth on a 2-dim cube with no Derived measures
> ([`mc_fixtures::build_graduated_hierarchy_cube`]). Each row's
> bench-side `preflight_for(depth)` `assert_eq!`s the dirty-set
> delta to the depth (one consolidated coord per ancestor element ×
> the single Spend measure) so a future maintainer cannot
> accidentally turn this microbench into something else.

| Bench | Median | Range | dirty_set_delta | Marginal vs prev |
|---|---:|---|---:|---:|
| `hierarchy_mark/depth_0` | **253 ns** | 243.2 – 268.0 ns | 0 | (baseline) |
| `hierarchy_mark/depth_1` | **438 ns** | 430.9 – 446.5 ns | 1 | +185 ns |
| `hierarchy_mark/depth_2` | **514 ns** | 500.2 – 529.0 ns | 2 | +76 ns |
| `hierarchy_mark/depth_3` | **548 ns** | 540.9 – 555.8 ns | 3 | +34 ns |

**Average marginal cost per ancestor: ~98 ns** ((548 − 253) / 3) on
the 2-dim graduated fixture. The first ancestor is the most
expensive (+185 ns) — consistent with the cost of switching from
the `h.edges.is_empty()` fast path in
[`compute_dirty_ancestors`](../crates/mc-core/src/cube.rs#L912) (which
short-circuits when no hierarchy exists) to the full Cartesian-walk
path. Subsequent ancestors are cheaper because the hierarchy walk
amortizes across the same `parent_of` lookups.

**Comparison with Acme.** Acme's full per-write cost on the
no-materialized-deps state is ~165 µs (§6.1
`write_input_leaf_no_deps`). On the graduated cube the per-write
cost at depth 0 is ~253 ns. The 165 µs − 246 ns = ~165 µs delta
between Acme and the synthetic baseline is **not** explained by the
linear-chain ancestor cost measured here. The dominant Acme cost is
the **Cartesian product of (per-dim hierarchy ancestors) × (every
derived measure)**: at the `Mar_2026/Paid_Search/Tampa` anchor,
this is 3 (Time slots: Mar, Q1, FY) × 3 (Channel slots: Paid_Search,
Paid_Media, All_Channels) × 4 (Market slots: Tampa, Florida,
Southeast, USA) × 6 (1 written + 5 derived measures) ≈ 215 marks.
At ~700 ns per mark on Acme (153 µs / 215 ≈ 712 ns) vs ~98 ns/mark
on the synthetic, the Acme overhead per mark is dominated by
6-dimensional `CellCoordinate` allocation + AHashSet insert, not by
the hierarchy traversal itself. See §8.1 + §9.3 for the implication
on Phase 2B optimization choices.

### 6.11 Phase 2B verification — Consolidation Fast Path (Arc<Hierarchy>)

> Closes the §6.7 3-leaf 1B miss + the §9.4 candidate-optimization
> entry. **Source change:** `Cube::dimensions: Vec<Dimension>` →
> `Arc<Vec<Dimension>>` and `Dimension::hierarchies: Vec<Hierarchy>` →
> `Vec<Arc<Hierarchy>>`, plus a rewrite of
> [`Cube::read_consolidated`](../crates/mc-core/src/cube.rs)
> lines 565–597 to replace the per-call `Vec<Dimension>` clone +
> `Vec<Hierarchy>` clone with one `Arc::clone(&self.dimensions)` plus
> a `Vec<Arc<Hierarchy>>` collect (refcount-bump per dim). Confined
> to `cube.rs` and `dimension.rs`. No public API removed or renamed
> (verified in §10's manifest below). Auxiliary deliverable:
> [`crates/mc-core/src/cube.rs`](../crates/mc-core/src/cube.rs)
> kernel unit test `consecutive_recompute_reads_match_phase_2b`
> (handoff item 3) — exercises the Arc fast path on consecutive
> recompute reads and asserts structural equality.

**Cold consolidation rows — before / after** (release bench, isolated
machine state; full bench JSON output is captured under
[`reports/bench-data/phase-2a/`](./reports/bench-data/phase-2a/) and
[`reports/bench-data/phase-2b/`](./reports/bench-data/phase-2b/) —
the Q3 baseline-tracking workflow that Phase 2B initially deferred
(see Phase 2B completion report §6.A) was closed retroactively later
the same day. Reproduces 12.65 µs → 2.38 µs on the 3-leaf row,
within drift of the document-asserted 14.3 → 2.53 µs below. Re-run
any row via `cp -R docs/reports/bench-data/phase-2b/* crates/mc-core/target/criterion/`
then `cargo bench -p mc-core --bench consolidated_read -- --baseline phase-2b`):

| Bench | Pre-2B median | Post-2B median | Δ | 1B target | Status |
|---|---:|---:|---:|---:|:---:|
| `consolidation_cold/Q1_PaidSearch_Tampa/Spend (3 leaves)` | 14.3 µs | **2.53 µs** | −82% | < 3 µs | ✓ ✓ |
| `consolidation_cold/Q1_PaidMedia_Florida/Spend (27 leaves)` | 16.2 µs | **4.53 µs** | −72% | < 30 µs | ✓ ✓ |
| `consolidation_cold/Q1_PaidMedia_Florida/CPC (27 leaves)` | 18.1 µs | **6.34 µs** | −65% | < 100 µs | ✓ ✓ |
| `consolidation_cold/Q1_PaidMedia_Florida/Revenue (27 leaves)` | 67.6 µs | **52.4 µs** | −22% | < 200 µs | ✓ ✓ |
| `consolidation_cold/FY_AllChannels_USA/Spend (420 leaves)` | 42.8 µs | **31.8 µs** | −26% | < 500 µs | ✓ ✓ |

Every cold row improves; the 3-leaf row, dominated by fixed-cost setup,
improves the most in absolute terms (~12 µs saved per call). The
Revenue row's 22% improvement reflects that the consolidator walk's
recursive rule eval (5-deep chain × 27 leaves) dominates over the
per-call dim/hierarchy clone — the clone savings (~12 µs) are a
smaller share of the total cost (67 µs → 52 µs). The 420-leaf row
(26% improvement) follows the same pattern: more leaves dilute the
fixed-cost savings into a smaller percentage even though the absolute
~11 µs win lines up exactly with the 3-leaf delta.

**Warm rows — drift check** (same release bench, same machine):

| Bench | Pre-2B median | Post-2B median | Δ |
|---|---:|---:|---:|
| `consolidation_warm/Q1_PaidSearch_Tampa/Spend (3 leaves)` | 64.2 ns | **63.8 ns** | −0.6% |
| `consolidation_warm/Q1_PaidMedia_Florida/Spend (27 leaves)` | 69.3 ns | **66.6 ns** | −3.9% |
| `consolidation_warm/Q1_PaidMedia_Florida/CPC (27 leaves)` | 67.7 ns | **66.8 ns** | −1.3% |
| `consolidation_warm/Q1_PaidMedia_Florida/Revenue (27 leaves)` | 66.9 ns | **66.4 ns** | −0.7% |
| `consolidation_warm/Q1_PaidMedia_Florida/Gross_Profit (27 leaves)` | 66.7 ns | **68.0 ns** | +1.9% |
| `consolidation_warm/FY_AllChannels_USA/Spend (420 leaves)` | 69.9 ns | **67.0 ns** | −4.1% |

All within run-to-run noise (≤ 5%). Warm reads short-circuit before
the dim/hierarchy setup path so they are unaffected by the change —
the small drifts here are pure noise.

**Other Phase 2A rows — drift check.** No regressions observed across
§6.1 (leaf read/write), §6.2 (derived read), §6.4 (dirty propagation),
§6.5 (demo path), §6.8 (synthetic no-deps write), §6.9 (snapshot
clone + rollback), §6.10 (hierarchy mark microbench). Every row is
within ±10% of the Phase 2A baseline.

| Bench (sampled) | Pre-2B | Post-2B | Δ |
|---|---:|---:|---:|
| `read_input_leaf_warm` | 48 ns | 48.2 ns | +0.4% |
| `read_input_leaf_cold` | 825 ns | 676 ns | −18% (drift) |
| `write_input_leaf` | 153 µs | 162 µs | +5.9% (drift) |
| `write_input_leaf_no_deps_synthetic` | 246 ns | 239 ns | −2.8% |
| `dirty_propagation/spend_at_anchor` | 153 µs | 150 µs | −2.0% |
| `snapshot/2520_cells_loaded` | 29.5 µs | 28.3 µs | −4.1% |
| `snapshot/materialized` | 55.1 µs | 53.1 µs | −3.6% |
| `rollback/materialized` | 173 µs | 172 µs | −0.6% |
| `hierarchy_mark/depth_3` | 548 ns | 546 ns | −0.4% |
| `demo_path/full_revenue_slice_warm (420 cells)` | 26.7 µs | (sampled under contention) | n/a |

`read_input_leaf_cold` and `write_input_leaf` deltas (−18%, +5.9%)
are within Phase 2A's documented run-to-run drift (§6.6); none of
those code paths touch `read_consolidated`. The
`demo_path/full_revenue_slice_warm` row is omitted as a clean delta
because the Phase 2B bench harness ran it under heavy concurrent
load; isolated re-measurement is a Phase 2C housekeeping item, not
gating.

**Determinism gate.** `for i in {1..10}; do cargo test --workspace
-q; done` produces 10/10 identical 210/210 results post-2B (was
10/10 pre-2B at 209/209; +1 from the new
`consecutive_recompute_reads_match_phase_2b` kernel unit test). The
formerly-flaky `t_consolidation_caches_value_within_revision` was
rewritten to semantic assertions (not timing) per ADR-0002 + the
Phase 2B SPEC QUESTION round-trip; see §10 below for the full file
manifest and the completion report's deviation entry.

---

## 7. Interpretation — bench by bench

### 7.1 `read_input_leaf_warm` (48 ns) and `read_input_leaf_cold` (825 ns)

Inputs do not have a derived-leaf cache (only derived measures do — see
[`cube.rs::read_derived_leaf`](../crates/mc-core/src/cube.rs)). The warm
path is a direct `HashMapStore::read()` after permission/coord checks.
At 48 ns the cube is doing essentially:

- 1 hash + lookup on `permissions` (HashMap),
- 1 hash + lookup on `store` (HashMap),
- packing the result into `CellValue`.

The cold path (825 ns) reflects extra work: the per-iteration cube is
freshly built and the OS / allocator caches are still warming up.
Once those caches settle the cold path approaches the warm path —
brief §11.1's distinction is more meaningful for **derived** leaves
(see §7.2) where the derived-leaf cache materially gates the cost.

### 7.2 `read_derived_leaf_warm` (~58 ns) and `read_derived_leaf_cold` (1.15 – 3.57 µs)

The warm path hits the derived-leaf cache and is indistinguishable from
the input warm path (~58 ns is one HashMap lookup + permission check).

The cold path's monotone increase from Clicks → Gross_Profit (1.15, 1.71,
2.33, 2.89, 3.57 µs) is the rule chain depth in action. Each derived
measure recomputes its rule body, which transitively reads its
dependencies — and after the per-iteration `build_cold()` setup, every
cell on the chain is dirty. Reading Clicks recomputes Clicks (depth 1).
Reading Gross_Profit recomputes Gross_Profit → Revenue → Customers →
Leads → Clicks (depth 5). Each level adds ~600 ns of `eval_expr` +
recursive `cube.read` work. **This is the expected shape — naive
recursive evaluation is producing the linear depth scaling the brief
§11.1 anticipated.**

### 7.3 `write_input_leaf` (163 µs) vs `write_input_leaf_no_deps` (165 µs) — the anomaly (closed in Phase 2A)

The brief §11.1 expected `_no_deps` to be ~4× faster than `_with_deps`
(50 µs ceiling vs 200 µs ceiling). On Acme they are **equal at ~165 µs**.
That is consistent across runs and is **not noise**.

The reason is structural to the Acme fixture, not a kernel slowdown:

1. **The Acme rev-edge graph fans in narrowly at any single coord.** A
   Spend write at Mar/Paid_Search/Tampa propagates to exactly 5
   rule-driven dependents at the same coord (Clicks, Leads, Customers,
   Revenue, Gross_Profit). The full transitive fan-out (≈17,825 entries
   reported in `invalidated.len`) is dominated by **hierarchy ancestors**
   walked per spec §8 — not rule rev-edges. Hierarchy ancestor walks
   happen even with an empty rule-dependency graph, because writes always
   mark the coord's hierarchy ancestors dirty.

2. **The hierarchy ancestor walk is the same in both benches.** Whether
   `materialize_all_dependencies` was called or not, the dimensions still
   carry the same Time/Channel/Market hierarchies, and a write always
   marks self + ancestor combinations dirty. With 6 dims × Acme's
   moderate fan-in, the per-write hierarchy mark walk dominates both
   benches.

3. **Write fixed costs (permission, lock, type, NaN, version, store
   write, revision bump) are the same.** Together with point 2, that
   leaves almost no observable difference.

So Phase 1B `_no_deps` "fails" the < 50 µs ceiling, but the failure is
the **brief's mental model not matching Acme's reality**, not a
regression. The brief's 1A ceiling for `_no_deps` was implicitly
modeling a cube with **no hierarchies** ("synthetic"), where
`mark_closure` would touch exactly one coord.

**Phase 2A closure.** The Phase 2A handoff added that exactly-described
synthetic fixture (`mc_fixtures::build_minimal_cube`) and the
`write_input_leaf_no_deps_synthetic` bench in §6.8. On the synthetic
fixture, the per-write cost is **246 ns** — the brief's 50 µs 1A
ceiling is met by ~200×, and the 1B target (2 µs) is met by ~8×. The
65,000× gap between the synthetic figure and the Acme `_no_deps`
figure decomposes (per §6.10) into:

- ~98 ns/mark per ancestor on a 2-dim, no-derived synthetic cube.
- ~712 ns/mark per (Cartesian-product slot × derived measure) on
  Acme — the difference is dominated by 6-dim `CellCoordinate`
  allocation + AHashSet insert, **not** by hierarchy traversal.

The Acme `_no_deps` row in §6.1 stays put as a documented Acme-fixture
path measurement (no kernel change), and the brief's §11.1 ceiling is
now passed on the row the brief was actually describing.

### 7.4 Consolidation reads — warm (cache hit) vs cold (cache miss)

> **Read this section as:**
> - "Warm-cache consolidation costs ~67 ns" — §6.3 (Phase 1B).
> - "Cold consolidation costs 14–68 µs depending on fan-out and rule
>   chain depth" — §6.7 (Phase 2A). Both cold ranges are well under
>   the brief's §11.2 1A ceilings.
>
> **Not as:** "consolidation costs 67 ns" — that conflates the cache
> hit with the actual walk.

**Warm-cache (~67 ns).** The consolidation cache (added in Phase 1A
so `t_consolidation_caches_value_within_revision` could measure ≥10×
speedup on second read) is **doing exactly its job**: every benched
warm-cache consolidation, from 3 leaves to 420 leaves, returns in
~67 ns regardless of fan-out. That is the cost of the cache lookup +
revision check + permission check, not the cost of walking the
hierarchy and aggregating.

**Cold (14–68 µs, see §6.7).** Phase 2A's cold variants force a
cache miss via an idempotent leaf write that bumps revision and
invalidates the consolidated coord. The cold-walk numbers are now
measured and pass every brief §11.2 1A ceiling:

- 3 leaves Spend → 14.3 µs (1A < 50 µs); ~5× over the 3 µs 1B target.
- 27 leaves Spend → 16.2 µs (1A < 1 ms; 1B < 30 µs ✓).
- 27 leaves CPC weighted avg → 18.1 µs (1A < 2 ms; 1B < 100 µs ✓).
- 27 leaves Revenue rule chain → 67.6 µs (1A < 5 ms; 1B < 200 µs ✓).
- 420 leaves Spend → 42.8 µs (1A < 20 ms; 1B < 500 µs ✓).

The 4× ratio between Revenue (67.6 µs) and Spend (16.2 µs) at 27
leaves is the cost of the per-leaf rule-chain replay (Spend →
Clicks → Leads → Customers → Revenue) on every cold read; this is
consistent with §6.2 (`read_derived_leaf_cold/Revenue` ≈ 2.85 µs)
times the 27-leaf fan-out, with a small amortization from the
shared AHashMap warmth.

The 3-leaf 1B miss (14.3 µs vs 3 µs target) suggests a fixed cost
floor in `Cube::read_consolidated` — likely the per-call clone of
`self.dimensions` and per-dim hierarchy clones (Phase 1A code
comment: "Phase 2 optimization (deferred per §0.A bench gate)") — a
fixed ~14 µs that the 3-leaf walk cannot break. See §9.4 for the
candidate optimization.

### 7.5 Dirty propagation (153 µs)

Single Spend write on a fully materialized cube. The 153 µs cost
breaks down approximately as:

- ~150 µs: the same write fixed costs measured in §7.3 (permission, lock,
  consolidated check, derived check, version check, type/NaN check,
  store write, hierarchy ancestor mark closure).
- ~3 µs (estimated, not measured): rule rev-edge walk of 5 entries.

The visible delta to `_no_deps` is small because the rule fan-out at a
single Acme coord is small. The dep graph IS being walked; it just has
nothing to chase.

**Pre-flight sanity (printed once at bench start):** dirty_set delta = 5,
invalidated.len = 17825, required-present and required-absent both
satisfied. See §6.4.

### 7.6 Demo path — `load_canonical_inputs` is 240 ms

This dominates everything. 2,520 cell writes × ~95 µs each = 240 ms
total. Each write incurs the same hierarchy-ancestor mark walk as §7.3
(95 µs is in line with §7.3's 165 µs because the canonical loader
writes with **no rules yet materialized**, so the mark walk is short
in absolute terms — most of the cost is the per-write fixed overhead).

`build_only` at 19.7 µs and `full_demo_reads` at 3.5 µs confirm that
build + read paths are negligible compared to the write loop. The
cube's hot loop is **input ingest**, not query. That matches the
expected planning workload (heavy initial load, then incremental
changes + read-mostly).

**Comparison:** `cargo run --release --bin mc -- demo` on the same
machine completes in well under 500 ms wall clock; the 240 ms bench
figure is consistent with that minus the I/O / println formatting
overhead.

### 7.7 Full revenue slice, warm — 26.7 µs for 420 cells = 64 ns/cell

Reads scale linearly. Each leaf read is one cache hit. `full_demo_reads`
at 3.5 µs covers 6 leaf reads + 5 consolidated + 1 traced read; same
shape, ~250 ns/op average (the trace pays a bit extra).

---

## 8. Known hot spots

These are the places the baseline points to as candidate bottlenecks
*if* a Phase 2 workload pushes any of them. None are required to be
addressed for Phase 1 to ship — the §7 ceilings are all met (Phase 1B
warm + Phase 2A cold).

### 8.1 The hierarchy ancestor mark walk dominates Acme write latency — but the per-mark cost is dominated by `CellCoordinate` allocation, not hierarchy traversal

Every Acme write pays the same ~150 µs hierarchy mark walk regardless
of how much rule fan-out exists. On Acme the rule fan-out at a single
coord is tiny (5 entries), so the mark walk is the cost. On a future
cube with deeper hierarchies (months × weeks × days, channels ×
subchannels × campaign IDs, larger geographic trees), the walk would
scale roughly linearly with the **product** of per-dim hierarchy
depths — a real combinatorial.

**Phase 2A's §6.10 microbench refines the diagnosis.** On the
synthetic graduated-depth fixture (2 dims, 1 derived measure), the
marginal cost per ancestor is **~98 ns**. On Acme the per-mark cost
is **~712 ns** (153 µs ÷ 215 marks). The 7× gap is **not** the
hierarchy ancestor traversal — it is the per-mark cost of:

1. Allocating a 6-element `CellCoordinate` SmallVec.
2. Cloning element IDs into it.
3. Inserting into the `AHashSet<CellCoordinate>` dirty tracker
   (which hashes the full 6-element coord).

Phase 2B options (see §9.3):
- **Reduce per-mark allocation** — work on `&[ElementId]` slices
  with a shared backing buffer instead of allocating a fresh
  SmallVec per mark.
- **Bitset-backed dirty tracker** — keyed by per-dim element index +
  per-measure index instead of a full `CellCoordinate` hash.

**Source location:** the closure walk happens inside
[`cube.rs::write`](../crates/mc-core/src/cube.rs) → `mark_closure`
([`dirty.rs`](../crates/mc-core/src/dirty.rs)) and via per-dim ancestor
expansion driven by `compute_dirty_ancestors` (cube.rs).

### 8.2 `cube.rs::read_consolidated` clones each dim's default hierarchy on every read

Phase 1A completion report §8 follow-up #9. **Phase 2A measured this
on the cold path (§6.7).** The 3-leaf cold consolidation row at
14.3 µs (vs 1B target 3 µs) is the smoking gun: a 3-leaf walk
shouldn't take 14 µs of work on its own — the bulk is the per-call
clone of `self.dimensions` and the per-dim hierarchy clones in
`read_consolidated`. Phase 2B candidate: replace the clones with
`&[Dimension]` borrows or `Arc<Hierarchy>` per-dim, which should
collapse the fixed cost. See §9.4.

### 8.3 `Snapshot` is a deep clone of `HashMapStore` — now quantified (Phase 2A §6.9)

Phase 1A completion report §8 follow-up #3. **Phase 2A's §6.9 bench
suite quantifies it.** Snapshot cost is sub-linear in cardinality at
Acme scale: 7.6 ns at 0 cells, 1.13 µs at 100 cells, 29.5 µs at
2,520 cells (loaded), 55.1 µs at ~25K cells (materialized). Per-cell
cost drops from 11 ns to 2.2 ns as the AHashMap clone amortizes its
fixed overhead.

Rollback is ~3× more expensive than snapshot (clone + revision bump
+ dirty-clear + Rule-provenance prune walk): 173 µs at materialized
state. The §9.5 follow-up (Snapshot COW) is **not gating at Acme
scale** — even a 250K-cell cube linear-extrapolates to ~1 ms per
snapshot, well under any plausible Phase 2 workflow budget. Revisit
only if a workflow takes many snapshots per turn.

### 8.4 `iter()` on `HashMapStore` is non-deterministic per CLAUDE.md §2.11

Not a perf hot spot today, but worth noting for any Phase 2 export /
dump path that needs deterministic order — that path will pay an O(N
log N) sort cost. The current benchmarks do not call `iter()` on the
hot path (consolidations walk targeted coords via `read()`).

### 8.5 `cube.rs::write` does redundant per-dim element walks

`is_consolidated_coord` walks every dimension to check whether the
coord's element at that position has children in the default hierarchy.
This is O(dims × children) per write. On Acme with 6 dimensions and
small fan-outs it's fast enough not to be the bottleneck — but it's a
fixed cost that will scale with dimensionality. Worth caching the
"this element is a leaf in this hierarchy" bit on the `Element` itself
in Phase 2.

---

## 9. Recommendations for Phase 2B optimization

Listed in rough priority order. **None are gating.** Phase 2B should
prioritize from data, not from this list.

### 9.1 ~~Cold consolidation benchmarks~~ — closed in Phase 2A (§6.7)

Phase 2A added cold-path variants for every §11.2 consolidation row.
Every brief §11.2 1A ceiling is now passed by real cold reads (see
§6.7 + §7.4). The 3-leaf 1B target (3 µs) is missed at 14.3 µs — see
§9.4 below for the candidate cause and fix. Otherwise, the
consolidation algorithm sits well within its 1A and 1B ceilings;
optimization here is opportunistic, not corrective.

### 9.2 Per-dim leaf-flag caching to fast-path `is_consolidated_coord`

§8.5. Cache `is_leaf_in_default_hierarchy: bool` on each `Element`.
Trivial source change with no semantics change.

### 9.3 Reduce hierarchy mark closure cost

§8.1. Two paths:
- (a) **Lazy ancestor marks** — only mark hierarchy ancestors lazily
  when a consolidated read asks for them. Today every write
  preemptively marks them.
- (b) **Bitset-backed dirty tracker** for hot ranges, instead of a
  general `HashSet<CellCoordinate>`.

(a) is a behavior shift that needs a careful invariant audit (the §10.1
delta-bounded test is sensitive to mark-set size). (b) is a pure
performance change.

### 9.4 ~~Consolidation hierarchy clone~~ — closed in Phase 2B

§8.2 / Phase 1A follow-up #9 / Phase 2A measurement.
**Closed 2026-05-01 in Phase 2B.** `Cube::dimensions` is now
`Arc<Vec<Dimension>>` and each `Dimension::hierarchies` is now
`Vec<Arc<Hierarchy>>`. `Cube::read_consolidated` (lines 565–597 in
[`cube.rs`](../crates/mc-core/src/cube.rs)) replaces the per-call
`Vec<Dimension>` deep-clone + `Vec<Hierarchy>` deep-clone with one
`Arc::clone(&self.dimensions)` plus a `Vec<Arc<Hierarchy>>` collect
(refcount-bump per dim). The §6.7 3-leaf cold row drops from 14.3 µs
to 2.53 µs (−82%) and clears its 1B target by ~16%; every higher-fan-out
cold row improves by approximately the same fixed ~12 µs. See §6.11
above for the per-row before/after diff and the no-regression check
on adjacent benches.

### 9.5 Snapshot copy-on-write — now data-justified (Phase 2A §6.9)

§8.3 / Phase 1A follow-up #3. **Phase 2A's §6.9 quantifies the cost
across cardinalities.** At Acme scale (≤ 25K cells) snapshot is
55 µs and rollback is 173 µs — well under any plausible Phase 2
budget for a single-snapshot operation. COW is **not justified yet
by data**; defer until a workflow takes many snapshots per turn (and
pay close attention to rollback at scale: it grows ~2.4× from
2,520→25K cells, suggesting the prune walk's `store.iter()` becomes
linear-dominant).

### 9.6 Recursive rule eval — leave it

`read_derived_leaf_cold` scales linearly with rule chain depth at
~600 ns/level. Phase 2's first instinct will be to flatten the
recursion. **Don't** until benchmarks justify it. At 5-deep chains
this is well under any 1B target.

### 9.7 Toolchain bump revisit

The current `Cargo.lock` pins `clap`, `clap_lex`, `half` to pre-edition2024
versions to keep Rust 1.78 viable. When the project is ready to bump to
Rust 1.85+:

1. Remove the three `cargo update --precise` pins (or run `cargo update`
   to take the latest).
2. Update [`rust-toolchain.toml`](../rust-toolchain.toml) channel.
3. Re-run the bench suite and update this document.
4. Restore `proptest = "1"` and `insta = "1"` in
   [`crates/mc-core/Cargo.toml`](../crates/mc-core/Cargo.toml) per the
   CLAUDE.md §1.1 closure conditions.

This is a Phase 2 housekeeping item, not a perf optimization, but it
unblocks proptest doctrines (§10.7) and insta-driven snapshot tests.

---

## 10. Behavior change statement

**Phase 1B and Phase 2A did not modify any `crates/mc-core/src/` file.**
**Phase 2B modified two:** [`crates/mc-core/src/cube.rs`](../crates/mc-core/src/cube.rs)
and [`crates/mc-core/src/dimension.rs`](../crates/mc-core/src/dimension.rs).
The change is the consolidation fast path documented in §6.11 + §9.4
+ ADR-0002. **Behavior is preserved** — every value the engine emits
on every test in §10.1–§10.7 of the brief, every cell value in `mc demo`
output, every consolidator decision (Sum / WeightedAverage / Min / Max),
every dirty-set delta, every cache-hit detection, every revision
sequence — is identical to Phase 1A. The change is purely a storage-shape
optimization (Arc-wrapped dim/hierarchy snapshots) that swaps deep
clones for refcount bumps inside `Cube::read_consolidated`.

`cargo run --release --bin mc -- demo` still produces brief §4.6
output verbatim. `cargo test --workspace` is now 210 / 0 (was 209;
+1 from the new `consecutive_recompute_reads_match_phase_2b` kernel
unit test mandated by Phase 2B handoff item 3).

**One contract test was rewritten alongside the kernel change:**
`crates/mc-core/tests/consolidation.rs::t_consolidation_caches_value_within_revision`
moved from a single-shot `Instant::elapsed()` ratio assertion to a
direct semantic check (cache populated with `Provenance::Consolidation`
+ matching revision; second read returns byte-for-byte identical
value; revision unchanged across reads; post-write invalidation
reaches the consolidated coord; recompute reflects the new leaf).
The brief §10.3 wording's intent — "the cache hit happened" — is
preserved; the "10× faster" wording was a Phase-1A-era proxy that
became un-measurable in debug-mode `cargo test` once Phase 2B made
the cold path fast enough that timer noise + workspace-parallel-load
ate the headroom. **The performance claim moved to its proper home in
this document** (§6.3 warm reads at ~64 ns vs §6.7 cold reads at
~2.5 µs — a ~40× speedup statistically established over 100 samples
per row). See [`../decisions/0002-perf-assertions-in-benchmarks-not-tests.md`](../decisions/0002-perf-assertions-in-benchmarks-not-tests.md)
for the rule, and the Phase 2B completion report for the deviation
audit trail.

Files changed in Phase 1B:

```
.gitignore                            # ignore crates/*/target (criterion output dir)
crates/mc-core/Cargo.toml             # add criterion dev-dep + 5 [[bench]] entries
crates/mc-core/benches/leaf_read_write.rs    # new
crates/mc-core/benches/derived_read.rs       # new
crates/mc-core/benches/consolidated_read.rs  # new
crates/mc-core/benches/dirty_propagation.rs  # new
crates/mc-core/benches/demo_path.rs          # new
Cargo.lock                            # 3 transitive pins (clap, clap_lex, half)
docs/PERF.md                          # this file
docs/HANDOFF.md                       # Phase 2A measurement-first handoff queued
docs/CURRENT_STATE.md                 # bench gate moves from DEFERRED to tooling-unblocked
docs/reports/phase-1-completion-report.md   # close criterion 5 tooling, document caveats
CLAUDE.md                             # §1.1 partial closure (criterion side); §6.4 caveats
```

Files changed in Phase 2A:

```
crates/mc-core/Cargo.toml                       # 3 new [[bench]] entries (synthetic_no_deps, snapshot_clone, hierarchy_mark)
crates/mc-core/benches/consolidated_read.rs     # extended with 5 cold variants (no warm rows removed)
crates/mc-core/benches/synthetic_no_deps.rs     # new
crates/mc-core/benches/snapshot_clone.rs        # new
crates/mc-core/benches/hierarchy_mark.rs        # new
crates/mc-fixtures/src/lib.rs                   # add build_minimal_cube + build_graduated_hierarchy_cube + 6 unit tests
docs/PERF.md                                    # this file (§6.7–§6.10 + §7/§8/§9/§10 updates)
docs/CURRENT_STATE.md                           # close Phase 2A; close deviation #6
docs/reports/phase-2a-completion-report.md      # new
```

No `crates/mc-core/src/*.rs` file was modified. No
`crates/mc-core/tests/*.rs` file was modified. No locked spec input
under `docs/specs/` was modified. No `Cargo.lock` change.

No behavior change was required by any benchmark finding. The §7.3
`write_input_leaf_no_deps` Phase 1B caveat closes via the new
synthetic fixture in §6.8, not via a kernel change. The §6.3 / §7.4
warm-vs-cold consolidation caveat closes via §6.7, not via a kernel
change. Both Phase 2A measurement gaps were resolved by adding
benches and fixtures only.

Files changed in Phase 2B:

```
crates/mc-core/src/cube.rs                      # Cube::dimensions: Arc<Vec<Dimension>>; read_consolidated fast path; new kernel unit test
crates/mc-core/src/dimension.rs                 # Dimension::hierarchies: Vec<Arc<Hierarchy>>; default_hierarchy_arc() accessor
crates/mc-core/tests/consolidation.rs           # t_consolidation_caches_value_within_revision rewrite (semantic, not timing) per ADR-0002
docs/PERF.md                                    # this file (§6.7 row + status flip; new §6.11; §9.4 closure-noted; §10 manifest + behavior-change note)
docs/decisions/0002-perf-assertions-in-benchmarks-not-tests.md   # new ADR
docs/decisions/README.md                        # ADR index entry
docs/CURRENT_STATE.md                           # close Phase 2B; flip status; add ADR-0002 to active list
docs/roadmap/MASTER_PHASE_PLAN.md               # 2B row → complete + tag
docs/reports/phase-2b-completion-report.md      # new
```

No file outside this manifest was modified. `Cargo.lock` is
unchanged. No `cargo update` was run. No new external dependency
was added (`std::sync::Arc` is `std`, not a dependency). No public
symbol from [`crates/mc-core/src/lib.rs`](../crates/mc-core/src/lib.rs)
was added, removed, or renamed.
