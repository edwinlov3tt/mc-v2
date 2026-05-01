# Phase 1B Handoff — Benchmark Baseline + PERF.md

> **Audience:** the Claude Code instance running in
> `/Users/edwinlovettiii/marketingcubes-v2/` that picks up Phase 1B.
> **You inherit a green Phase 1A.** Your job is **measurement, not
> behavior change.** Read this whole file before touching code.

---

## Where Phase 1A ended

- **Initial commit:** `4aa674a` — *Initial commit: Phase 1 Rust kernel for MarketingCubes V2*
- **Test status:** 203 / 203 passing across all targets. 10/10 determinism gate runs identical.
- **Demo:** `cargo run --release --bin mc -- demo` matches brief §4.6.
- **Gates green:** `cargo build --release --workspace`, `cargo fmt --check --all`, `cargo clippy --workspace --all-targets -- -D warnings`.
- **Toolchain:** Rust 1.78 pinned in [`rust-toolchain.toml`](../../rust-toolchain.toml). **Do not bump without explicit approval.**
- **Outstanding deferral:** brief acceptance criterion 5 (`cargo bench`) — that is the gate you are closing.

For the full Phase 1A audit, read [`phase-1-completion-report.md`](../reports/phase-1-completion-report.md) — especially §3 (deviations), §4.1 (the toolchain blocker), §6 (the deferred criterion you're addressing), and §8 (Phase 2 follow-ups so you know what is *not* yours to do here).

The non-negotiable operating manual is [`CLAUDE.md`](../../CLAUDE.md). Read sections 0, 1, 1.1, 2 (especially 2.6, 2.7, 2.12), 3, 5.1, 5.5, 6, 8, and 12 before writing any code. They override anything below if there is a conflict.

---

## Phase 1B prompt (verbatim — this is your contract)

> We are starting MarketingCubes Phase 1B: Benchmark Baseline + PERF.md.
>
> **Context:**
> Phase 1A is complete. The Rust kernel builds cleanly, passes 203/203 tests, passes the 10-run determinism gate, has a working CLI demo, and has no out-of-scope features. Do not change behavior unless a benchmark exposes a clear bug.
>
> **Goal:**
> Close the deferred benchmark gate from Phase 1A and produce a trustworthy performance baseline before Phase 2 begins.
>
> **Phase 1B scope:**
> 1. Resolve benchmark tooling under the current Rust 1.78 toolchain if possible.
> 2. Add a benchmark harness.
> 3. Benchmark leaf reads/writes.
> 4. Benchmark derived reads.
> 5. Benchmark consolidated reads.
> 6. Benchmark dirty propagation.
> 7. Benchmark the Acme CLI/demo load/read path.
> 8. Produce `docs/PERF.md` with results, environment, commands, observations, and recommended Phase 2 optimizations.
>
> **Hard rules:**
> - Do not add model cells.
> - Do not add DuckDB.
> - Do not add WASM.
> - Do not add PyO3.
> - Do not add async, threads, rayon, tokio, serde, or external storage.
> - Do not introduce CellStore trait yet.
> - Do not rewrite HashMapStore.
> - Do not optimize before first measuring.
> - Do not loosen or remove any existing tests.
> - All existing 203 tests must still pass.
> - If a benchmark dependency requires bumping Rust, stop and report the options before changing rust-toolchain.toml.
>
> **Benchmark tooling instructions:**
> First try to restore Criterion in a Rust 1.78-compatible way by pinning versions if possible.
> If Criterion cannot be made compatible cleanly, implement a std-only benchmark runner using `std::time::Instant` and `std::hint::black_box`.
> Do not bump the Rust toolchain without explicit approval.
>
> **Preferred structure:**
> - If Criterion works:
>   - add `crates/mc-core/benches/leaf_read_write.rs`
>   - add `crates/mc-core/benches/derived_read.rs`
>   - add `crates/mc-core/benches/consolidated_read.rs`
>   - add `crates/mc-core/benches/dirty_propagation.rs`
>   - add `crates/mc-core/benches/demo_path.rs`
> - If Criterion does not work:
>   - add `crates/mc-cli/src/bin/mc-bench.rs` or `crates/mc-bench`
>   - make it runnable with `cargo run --release --bin mc-bench`
>   - output stable plain-text and optionally CSV-like rows
>
> **Benchmarks to implement:**
>
> 1. **Leaf read/write**
>    - Write one input leaf cell.
>    - Read one input leaf cell warm.
>    - Repeat enough iterations to produce stable average/median-ish timing.
>    - Use Acme fixture coordinates.
>
> 2. **Derived read**
>    - Read Clicks, Leads, Customers, Revenue, Gross_Profit at a leaf coordinate.
>    - Include cold read after dirtying an input.
>    - Include warm read after cache/materialization.
>
> 3. **Consolidated read**
>    - Read Q1 × Paid_Media × Florida for:
>      - Spend
>      - CPC
>      - Revenue
>      - Gross_Profit
>    - Include `child_count` in the benchmark label where useful.
>    - Confirm results still match golden values before timing.
>
> 4. **Dirty propagation**
>    - Write Spend at one leaf coordinate after dependencies are materialized.
>    - Measure dirty mark/closure time.
>    - Report dirty set delta size.
>    - Include required-present and required-absent sanity checks before timing.
>
> 5. **Demo path**
>    - Build Acme cube.
>    - Write canonical inputs.
>    - Materialize dependencies if needed.
>    - Run the same reads as `mc demo`.
>    - Report total elapsed time.
>
> **PERF.md requirements:**
> Create `docs/PERF.md` with:
>
> 1. Commit hash
> 2. Toolchain version
> 3. Machine/environment summary
> 4. Benchmark command(s)
> 5. Whether Criterion was used or std-only runner was used
> 6. Raw benchmark table
> 7. Short interpretation of each benchmark
> 8. Known hot spots
> 9. Recommendations for Phase 2 optimization
> 10. Explicit statement that no behavior changed unless a behavior change was required and documented
>
> **Validation gate before reporting done:**
> Run:
> - `cargo fmt --check --all`
> - `cargo clippy --workspace --all-targets -- -D warnings`
> - `cargo build --release --workspace`
> - `cargo test --workspace`
> - `cargo run --release --bin mc -- demo`
> - benchmark command
>
> **Completion report format:**
> ```
> DONE: Phase 1B Benchmark Baseline
>
> Build:    [command] ✓/✗
> Format:   [command] ✓/✗
> Lint:     [command] ✓/✗
> Tests:    cargo test --workspace [N]/[N]
> Demo:     target/release/mc demo ✓/✗
> Bench:    [command] ✓/✗
>
> Benchmark tooling:
> - Criterion compatible? yes/no
> - If no, why?
> - Tooling chosen:
>
> Files changed:
> - list files
>
> Results summary:
> - leaf read/write:
> - derived read:
> - consolidated read:
> - dirty propagation:
> - demo path:
>
> Deviations:
> - list any deviations from Phase 1B instructions
> ```
>
> Do not start Phase 2 features.

---

## Context the prompt above does NOT spell out (Phase 1A landmarks you will need)

### A. The toolchain blocker — why criterion was deferred

From [`phase-1-completion-report.md`](../reports/phase-1-completion-report.md) §4.1:

> On Rust 1.78 (pinned), criterion's transitive dependency `clap_lex 1.1.0` requires `edition2024`, which only stabilized in 1.85.

Workspace declarations for `criterion = "0.5"`, `proptest = "1"`, and `insta = "1"` are still in the **root** [`Cargo.toml`](../../Cargo.toml) (per brief §2.5). They are not pulled into `mc-core/Cargo.toml`. CLAUDE.md §1.1 spells out the rule: **don't re-add to `mc-core` dev-deps until the block resolves.**

**First thing to try (do not skip — the prompt mandates this attempt):** can criterion be pinned to an older version whose dep tree fits Rust 1.78? Try `criterion = "0.4"` or `criterion = "0.3"` with `default-features = false`. Run `cargo tree -e features --target ... --no-default-features` (no actual flag combo here is sacred — explore) to see if you can sidestep `clap_lex 1.1.0`. If even that pulls in an edition2024 transitive, **stop and report the options** before bumping the toolchain. CLAUDE.md §11 has the SPEC QUESTION format.

If criterion truly cannot fit, implement a std-only runner per the prompt — that is an explicitly authorized fallback.

### B. The fixture surface area you will use

[`crates/mc-fixtures/src/lib.rs`](../../crates/mc-fixtures/src/lib.rs) exposes:

```rust
pub fn build_acme_cube() -> Result<(Cube, AcmeRefs), EngineError>
pub fn write_canonical_inputs(cube: &mut Cube, refs: &AcmeRefs) -> Result<usize, EngineError>
pub fn materialize_all_dependencies(cube: &mut Cube, refs: &AcmeRefs) -> Result<usize, EngineError>
pub fn coord(cube_id, refs, scenario, version, time, channel, market, measure) -> CellCoordinate
pub fn canonical_inputs_for(time_idx, channel_idx, market_idx) -> CanonicalInputs
```

`AcmeRefs` is a `pub struct` carrying every named element/dimension/rule ID; the test files [`acme_demo.rs`](../../crates/mc-core/tests/acme_demo.rs), [`consolidation.rs`](../../crates/mc-core/tests/consolidation.rs), and the CLI demo [`crates/mc-cli/src/main.rs`](../../crates/mc-cli/src/main.rs) are good models for how to use it.

The canonical anchor cell (Mar/Paid_Search/Tampa) golden values:

```
Spend = 11_500.00       Clicks       = 7_666.67
CPC   = 1.50            Leads        = 153.33
CVR   = 0.020           Customers    = 15.33
...                     Revenue      = 3_066.67
                        Gross_Profit = 2_146.67
```

Consolidated golden values (Q1/Paid_Search/Tampa Spend = 33_000.00, Mar/Paid_Search/Florida Spend = 35_100.00, Q1/Paid_Media/Florida Spend = 329_400.00, Q1/Paid_Search/Florida CPC ≈ 1.5202381) are documented in brief §4.5.1 and asserted in `tests/acme_demo.rs`. **Use these as the "results match golden values before timing" sanity check the prompt requires for the consolidated benchmark.**

### C. Caching behavior to be aware of when designing benchmarks

Phase 1A added two caching layers — your "warm read" measurements need to account for both:

- **Derived-leaf cache** in [`cube.rs::read_derived_leaf`](../../crates/mc-core/src/cube.rs): on cache hit (`!dirty.is_dirty(coord) && stored.revision == self.revision && !request_trace`), returns a stored `CellValue` immediately. A trace request bypasses the cache.
- **Consolidated cache** in [`cube.rs::read_consolidated`](../../crates/mc-core/src/cube.rs): same pattern — gated on `Provenance::Consolidation`, store hit, dirty bit. Added in Phase 1A specifically so `t_consolidation_caches_value_within_revision` could measure ≥10× speedup on second read.

Implication for your benchmarks:

- **Cold derived read** = first read after a dirty mark (write to upstream input invalidates via `mark_closure` + `compute_dirty_ancestors`). Measure that explicitly.
- **Warm derived read** = re-read at the same revision, no intervening write. Measure that explicitly.
- **Cold consolidated read** = first read at this revision (after write or after `cube` build).
- **Warm consolidated read** = re-read at the same revision.

The dirty-propagation benchmark inherits the §10.1 framing: assertions are *delta* assertions, not absolute. See [`tests/acme_demo.rs::t_acme_dirty_set_size_within_bound_after_one_spend_write`](../../crates/mc-core/tests/acme_demo.rs) for the model — capture `cube.dirty().len()` (or the iter snapshot) before and after the write you're timing. Same for required-present / required-absent sanity checks.

### D. Lazy dependency graph — what materialization buys you

`DependencyGraph` is built lazily on read (per CLAUDE.md §2.1). Tests in [`tests/dependency.rs`](../../crates/mc-core/tests/dependency.rs) confirm:

- Empty graph immediately after `build_acme_cube()`.
- A single `cube.read(...)` on a derived cell materializes the rule chain at that coord (≈5 forward edges per leaf, 2 deps per rule).
- `materialize_all_dependencies(cube, refs)` reads every (12 × 5 × 7 × 5) = 2,100 leaf-derived cells once, taking the dep graph to ≈4,200 forward edges.

Implication for the dirty-propagation benchmark: **call `materialize_all_dependencies` first** so the dep graph is populated. Otherwise `mark_closure` walks an empty reverse-edge index and the timing reflects nothing real.

For the demo-path benchmark the prompt explicitly says "Materialize dependencies if needed" — read this as: include both timings (with and without materialization) if the difference is interesting; otherwise pick one and explain in PERF.md.

### E. Phase 1A "Phase 2 follow-ups" that touch performance

From [`phase-1-completion-report.md`](../reports/phase-1-completion-report.md) §8, two follow-ups are particularly relevant to your interpretation work in PERF.md §7-9:

- (#9) `cube.rs::read_consolidated` clones each dim's default hierarchy on every consolidated read. Tagged in source as "Phase 2 optimization (deferred per §0.A bench gate)". **This is exactly the kind of hot-spot your benchmark gate is meant to expose.**
- (#3) `Snapshot` is a deep clone of `HashMapStore` per spec. Snapshot-take cost is O(N) over store size today. Acceptable for Acme (~25K cells) but worth measuring if your demo-path benchmark touches it.

Don't fix either. Document them in PERF.md §8 (known hot spots) and §9 (Phase 2 recommendations) with the timings you observed.

### F. Phase 1A brief §11 ceilings (reference for PERF.md interpretation)

The brief in §11 lists Phase 1A ceilings (loose) and Phase 1B targets (tighter). Read those before writing PERF.md interpretation. The brief specifically says: *"Phase 1A targets are 20× looser than 1B for a reason. Phase 1 ships when 1A passes, period."* So your interpretation is **calibration**, not pass/fail. Phase 1A correctness is already shipped; Phase 1B is establishing the baseline so Phase 2 can be measured against it.

If a benchmark exceeds even the looser 1A ceiling, treat it as a finding for §8/§9 of PERF.md — do not change behavior unless the prompt's "clear bug" bar is met (and document if it is).

---

## Pointers to existing files you will most likely touch

| Why you might touch it | File | Phase 1B action |
|---|---|---|
| Add criterion back to `mc-core` dev-deps **only if pinned version works** | [`crates/mc-core/Cargo.toml`](../../crates/mc-core/Cargo.toml) | conditional on §A above; if std-only, leave alone |
| Add the `[[bench]]` entries criterion needs | [`crates/mc-core/Cargo.toml`](../../crates/mc-core/Cargo.toml) | only if criterion path |
| Bench files | `crates/mc-core/benches/*.rs` (criterion path) **or** `crates/mc-cli/src/bin/mc-bench.rs` (std-only path) | new files |
| Document the toolchain story / decisions | [`docs/PERF.md`](../PERF.md) | new file |
| Update `phase-1-completion-report.md` §6 (deferral row) once acceptance criterion 5 is satisfied | [`docs/phase-1-completion-report.md`](../reports/phase-1-completion-report.md) | small edit when bench gate passes |

Files you should **NOT** touch unless a benchmark exposes a clear bug (per the prompt's hard rules):

- Anything in `crates/mc-core/src/` — production behavior is locked.
- `crates/mc-core/tests/*.rs` — tests are contracts; don't loosen.
- `crates/mc-fixtures/src/lib.rs` — fixture is sealed for §10 contracts.
- [`docs/engine-semantics.md`](../engine-semantics.md), [`docs/phase-1-rust-kernel-build-brief.md`](../phase-1-rust-kernel-build-brief.md) — locked input documents.

If you need a new public helper on `mc-fixtures` (e.g. a benchmark-only "warm everything" pass), prefer adding it next to `materialize_all_dependencies` — that's the precedent for benchmark-shaped helpers.

---

## Reproducible commands you can rely on

These all exit 0 today on the Phase 1A commit. They are the ground state your work must preserve.

```bash
cd /Users/edwinlovettiii/marketingcubes-v2

# Sourcing cargo (only needed if your shell didn't set up rustup paths)
source $HOME/.cargo/env

# Phase 1A gate
cargo build --release --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace               # 203 / 0
cargo run --release --bin mc -- demo
```

The CLI demo's expected shape is in brief §4.6. The current run prints the §4.6 numbers verbatim modulo two cosmetics noted in [`phase-1-completion-report.md`](../reports/phase-1-completion-report.md) §1; do not "fix" those without a separate task.

---

## Final checklist before you call Phase 1B done

- [ ] Criterion attempt documented in PERF.md §5 (worked / didn't work + why).
- [ ] All 5 benchmark categories implemented.
- [ ] All sanity assertions (golden-value match for consolidated, required-present/required-absent for dirty propagation) ran green before any timing was recorded.
- [ ] All 203 tests still pass on `cargo test --workspace`.
- [ ] `cargo fmt --check --all` clean.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] `cargo run --release --bin mc -- demo` still matches §4.6.
- [ ] `docs/PERF.md` exists and answers all 10 of the prompt's PERF.md requirements.
- [ ] Completion report posted in chat in the exact format the prompt specifies.
- [ ] No out-of-scope additions (re-read the hard rules list in the prompt above).
- [ ] **You did not start Phase 2 features.**

If you are uncertain at any point, the resolution order from CLAUDE.md §0 is:
1. Phase 1B prompt above.
2. [`phase-1-completion-report.md`](../reports/phase-1-completion-report.md).
3. [`engine-semantics.md`](../engine-semantics.md) and [`phase-1-rust-kernel-build-brief.md`](../phase-1-rust-kernel-build-brief.md).
4. [`../CLAUDE.md`](../../CLAUDE.md).
5. Anything else.

If those still don't resolve it: stop, write a SPEC QUESTION per CLAUDE.md §11, and wait. Don't guess.
