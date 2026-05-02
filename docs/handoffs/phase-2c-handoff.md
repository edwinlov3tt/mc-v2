# Phase 2C Handoff — Production-Shaped Workload Benchmarks

> **Audience:** the Claude Code instance running in
> `/Users/edwinlovettiii/Projects/mc-v2/` that picks up Phase 2C.
> **You inherit a green Phase 2B** (commit `6ea58ab`, tag
> `phase-2b-consolidation-fast-path`) plus the Q3 baseline-tracking
> workflow (commit `9f7420c`) and an Accepted-Provisional ADR-0003
> defining the workload curve to calibrate against.
>
> **Your job is measurement, not optimization.** Phase 2C produces
> the workload-shaped data that lets Phase 2D pick between §9.3
> (bitset-backed dirty tracker), §9.2 (leaf-flag cache), or
> something else the data surfaces. **Do not modify any file under
> `crates/mc-core/src/`.** The §9.3 vs §9.2 priority call lives in
> the next handoff, *after* the data lands.

---

## Where Phase 2B + Q3 ended

- **Phase 2B commit / tag:** `6ea58ab` — *bench: Phase 2B consolidation fast path (Arc<Hierarchy>)* — tag `phase-2b-consolidation-fast-path`.
- **Q3 closure commit:** `9f7420c` — *bench: capture phase-2a + phase-2b criterion baselines (close Q3)*. Both baselines live under [`../reports/bench-data/phase-2a/`](../reports/bench-data/phase-2a/) and [`../reports/bench-data/phase-2b/`](../reports/bench-data/phase-2b/) (1.4 MB JSON, 45 rows × 2 phases × 4 files; no `raw.csv` because criterion is `default-features = false`).
- **Test status:** 210 / 210 passing, 10/10 deterministic.
- **Demo:** `cargo run --release --bin mc -- demo` matches brief §4.6.
- **Gates green:** build / fmt / clippy / test / demo / bench (8 bench files, all rows captured against `--baseline phase-2b`).
- **Toolchain:** Rust 1.78 pinned in [`../../rust-toolchain.toml`](../../rust-toolchain.toml). **Do not bump without explicit approval.**
- **Cargo.lock pins (Phase 1B, still load-bearing):** `clap → 4.4.18`, `clap_lex → 0.6.0`, `half → 2.4.1`. Do not run `cargo update`.
- **ADR-0003** ([`../decisions/0003-workload-sketch.md`](../decisions/0003-workload-sketch.md)) — Accepted — Provisional. Defines the 10× / 50× / 100× Acme curve, the 100 ms spreadsheet-shaped click-instant threshold, the TM1 stacked-sandbox snapshot pattern, and the Phase 2C/2D split (2C measures; 2D picks).

For the full Phase 2B audit read [`../reports/phase-2b-completion-report.md`](../reports/phase-2b-completion-report.md). The non-negotiable operating manual is [`../../CLAUDE.md`](../../CLAUDE.md). The master roadmap is [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) — Phase 2C is a sub-phase of Phase 2; do not start Phase 2D / Phase 3 work in this phase.

---

## Phase 2C prompt (verbatim — this is your contract)

> We are starting MarketingCubes Phase 2C: Production-Shaped Workload Benchmarks.
>
> **Context.** Phase 2B shipped the consolidation fast-path (`Arc<Hierarchy>`) and closed PERF.md §9.4. Q3 (criterion baseline tracking) is in place at `bench-data/phase-2b/`. ADR-0003 (workload sketch) is Accepted — Provisional with a sunset clause, calibrating against a 10× / 50× / 100× Acme curve and a 100 ms spreadsheet-shaped click-instant threshold.
>
> **Goal.** Produce the workload-shaped data that decides whether §9.3 (bitset-backed dirty tracker) is the right Phase 2D pick, or whether §9.2 (per-dim leaf-flag caching) is, or whether something else surfaces. **Measurement only.** Do not modify `crates/mc-core/src/`.
>
> **Phase 2C scope:**
>
> 1. **Calibration fixtures.** Add an internal generic builder to `mc-fixtures` plus three thin public wrappers alongside `build_acme_cube`:
>
>    - **Internal (private to `mc-fixtures` or `pub(crate)`):** a generic `build_scaled_acme_cube(scale: u32)` (or equivalent) that takes a scale factor and produces a cube preserving Acme's dimensionality (6 dims, 11 measures, 5 rules) with element counts per dim scaled by `scale`. Internal so the **scale-1× equivalence test** (see below) can prove the scaled-builder path matches Acme goldens through the same code path the public wrappers go through.
>    - **Public:** thin wrappers `build_scaled_acme_cube_10x`, `build_scaled_acme_cube_50x`, `build_scaled_acme_cube_100x` that delegate to the internal builder with `scale = 10 / 50 / 100`.
>
>    **Cardinality definition:** "populated cells" at each scale means **canonical input cells written by the fixture's bulk-load** (the equivalent of Acme's `write_canonical_inputs` payload at 2,520 cells), *not* total store length after derived/consolidated caching. Targets: 1× = 2,520 / 10× ≈ 25 K / 50× ≈ 125 K / 100× ≈ 250 K input cells.
>
>    Each public wrapper is unit-tested for: dim count = 6, measure count = 11, rule count = 5, populated input-cell count within ±5% of target, hierarchy depth preserved from Acme's shape (Time 3 / Channel 2 / Market 4). **Do not introduce new dimensionality, new measures, or new rule shapes** — the goal is scaled Acme, not synthetic stress.
>
>    **Scale-1× equivalence test (mandatory).** A unit test calls the *internal* `build_scaled_acme_cube(1)` and asserts the resulting cube reproduces Acme's brief §4.5.1 golden values for the anchor cell (Mar/Paid_Search/Tampa: Spend = 11_500, CPC = 1.50, ..., Gross_Profit = 6_440/3) byte-for-byte after `write_canonical_inputs`-equivalent loading. This proves the scaled path is not a parallel reimplementation that drifts from Acme — it's the same code with `scale = 1`.
>
> 2. **Isolated-operation benches at three scales.** Extend (do not rewrite) the existing Phase 1B/2A bench files to include 10× / 50× / 100× variants of:
>    - `bench_write_input_leaf` (single edit at a leaf coord)
>    - `bench_read_input_leaf_warm` and `_cold`
>    - `bench_read_derived_leaf_cold` (Revenue — the rule-chain depth-5 row)
>    - `bench_consolidation_cold` at the 27-leaf and 420-leaf fan-outs
>    - `bench_load_canonical_inputs` (bulk ingest)
>    - `bench_snapshot` and `bench_rollback` at each cardinality
>
>    Every row reports against `--baseline phase-2b` so the diff is captured automatically. Save the post-2C baseline as `--save-baseline phase-2c` and copy `target/criterion/.../phase-2c/` JSON into `docs/reports/bench-data/phase-2c/` per the workflow established in [`../reports/bench-data/README.md`](../reports/bench-data/README.md).
>
> 3. **Combined-workflow bench (the load-bearing addition).** New file `crates/mc-core/benches/combined_workflow.rs`. Default scale: 50× Acme. Stress scale: 100× Acme. The bench simulates one planner session:
>
>    - Build cube + bulk-load canonical inputs.
>    - Materialize all dependencies.
>    - Loop 100 times: write a Spend cell at a varying coord (rotate through Time × Channel × Market combinations), every 5 iterations read a 27-leaf consolidated slice, every 10 iterations take a snapshot.
>    - Hold all snapshots live until the session ends (do not drop intermediate ones — the TM1 stacked-sandbox pattern from ADR-0003 Decision 6).
>
>    **Report:** total wall-clock for the session; per-edit p50, p95, p99; per-slice-read p50/p99; per-snapshot p50/p99; final dirty-set size; final invalidated.len; cumulative allocations (if measurable; if not, document as not measured).
>
>    > **Phase 2D historical-artifact note (2026-05-02).** "final invalidated.len" was measured under the Phase 1A *cumulative* reading of `WritebackResult.invalidated`. Phase 2D corrected the semantics to the marginal per-write reading (see [`PERF.md`](../PERF.md) §6.15 + [`docs/handoffs/phase-2d-handoff.md`](./phase-2d-handoff.md) §A); the field has since been renamed `last_write_invalidated_len` in [`crates/mc-core/benches/combined_workflow.rs`](../../crates/mc-core/benches/combined_workflow.rs) and now reports the marginal per-write transition count (single-digit at Acme scale) rather than the cumulative dirty count (~305 K at 50×).
>
>    The bench's value is *temporal*: a 250 ms edit in isolation can become a 3-second edit at iteration 47 when the dirty set has grown, hierarchies have been cloned repeatedly, and consolidated caches have been partially invalidated 30 times. This bench is the one that surfaces nonlinear-in-session-length costs.
>
> 4. **§6.10-style attribution during the combined workflow.** Capture per-mark cost (`mark walk time / dirty_set delta`) at iteration 1, iteration 50, and iteration 100. If per-mark cost grows across the session, that's the §9.3 evidence. If it's flat, §9.3 is opportunistic and §9.2 may be the better Phase 2D pick.
>
> **Hard rules:**
>
> - Do not modify any file under `crates/mc-core/src/`.
> - Do not modify any file under `crates/mc-core/tests/`.
> - Do not modify any locked spec input under `docs/specs/`.
> - Do not loosen, rewrite, or remove existing Phase 1B / 2A / 2B bench files; only extend them.
> - Do not modify `mc-fixtures` existing public functions (`build_acme_cube`, `write_canonical_inputs`, `materialize_all_dependencies`, `coord`, `canonical_inputs_for`, `build_minimal_cube`, `build_graduated_hierarchy_cube`, helpers). Additions only.
> - The internal generic `build_scaled_acme_cube(scale: u32)` is `pub(crate)` or private — not part of the public mc-fixtures API. Public surface is the three wrappers.
> - Do not bump `rust-toolchain.toml` without explicit approval.
> - Do not run `cargo update`. The Cargo.lock pins are load-bearing.
> - All 210 existing tests must still pass. New `mc-fixtures` unit tests for the scaled builders are expected and welcome (target: +6, one per builder × scale × invariant set, plus the scale-1× equivalence test).
> - All Phase 1B/2A/2B benches must still produce numbers consistent with PERF.md §6 (small drift fine; substantial drift means you've changed something you shouldn't have).
> - **Do not start §9.3 or §9.2 kernel work.** The deliverable is the data; the priority call is Phase 2D's.
>
> **Sanity checks before timing:**
>
> - Each scaled builder asserts its invariants (dim/measure/rule count, populated cells within tolerance) in a one-time pre-flight call before the bench loop.
> - The scale-1× equivalence test (mandatory unit test, not bench preflight) proves the scaled-builder path reproduces Acme golden values for the anchor cell. Fixture work is wrong if this fails — fix the fixture, not the test.
> - Snapshot integrity round-trips (snapshot → mutate → rollback → read) verified once per scale before timing.
> - Stderr emission per scale: `populated_input_cells = N; dirty_set initial = 0; rule_graph forward edges = M`.
>
> **Iteration vs final-report bench discipline:**
>
> Full criterion runs (sample-of-100 × 5s window per row × ~50 rows × 3 scales) are slow. **Smoke commands are allowed for iteration but the final completion report must use the full benchmark suite.**
>
> - **Smoke (per-bench, sub-second):**
>   ```bash
>   cargo bench -p mc-core --bench <name> -- \
>     --warm-up-time 1 --measurement-time 1 --sample-size 10
>   ```
> - **Final (per-bench, full sample):**
>   ```bash
>   cargo bench -p mc-core --bench <name> -- --baseline phase-2b
>   ```
> - **Save the post-2C baseline once at end of phase:**
>   ```bash
>   cargo bench -p mc-core --bench <name> -- --save-baseline phase-2c
>   # then for each bench file, copy target/criterion/.../phase-2c/ JSON
>   # into docs/reports/bench-data/phase-2c/<bench>/<id>/phase-2c/
>   ```
>
> Smoke numbers must not appear in PERF.md §6.12 / §6.13 / §6.14 — those tables come from the full sample-of-100 runs only. The completion report's bench gate is the full suite.
>
> **PERF.md update requirements:**
>
> 1. New section **§6.12 — Workload-Shaped Benchmarks (10× / 50× / 100×)**. Tables for the isolated-operation rows at all three scales (one table per operation, three rows per table). Annotate each row against the §11 1A and 1B ceilings *and* against ADR-0003's perception thresholds (the 100 ms click-instant gate, the 1 s responsive gate).
> 2. New **§6.13 — Combined Workflow**. Table reporting wall-clock + percentile breakdown at 50× and 100×. Plus the §6.10-style attribution at iterations 1 / 50 / 100.
> 3. New **§6.14 — Scaling shape.** Per-operation summary: linear / super-linear / cliff-at-N for each isolated operation across the 10×→100× curve. **This is the load-bearing interpretation section.** Phase 2D's priority call reads from here.
> 4. **§7 interpretation:** one paragraph per scaling-shape finding. If anything is super-linear, name it.
> 5. **§8 known hot spots:** refresh based on the new data. The §6.10 finding may be reaffirmed, refined, or contradicted — say which.
> 6. **§9:** **do not pick a winner.** Update each row's quantification with the new data; leave priority unspecified. Phase 2D's pick reads from §6.14, not §9.
>
> Replace the §6.12 stub created in the Phase 2 closure commit with the real §6.12 table.

---

## Acceptance criteria

- All three scaled builders ship with passing invariant tests + the scale-1× equivalence test that proves the internal builder reproduces Acme's brief §4.5.1 goldens.
- All isolated-operation benches run at all three scales against `--baseline phase-2b` with diffs captured under `bench-data/phase-2c/`.
- Combined-workflow bench runs at 50× and 100× and emits the percentile + attribution data above.
- PERF.md §6.12 / §6.13 / §6.14 / §7 / §8 / §9 updated per requirements above. Numbers come from full criterion runs (sample-of-100), not smoke.
- All 210 existing tests still pass; new fixture tests are net additions.
- `cargo bench -p mc-core --bench <name> -- --baseline phase-2b` produces a clean diff with no Phase 1B/2A/2B regression beyond ±10% noise.
- `cargo run --release --bin mc -- demo` still matches brief §4.6.
- All standard gates (build / fmt / clippy / determinism 10×) green.
- Completion report at `docs/reports/phase-2c-completion-report.md` from the template, including:
  - The §6.14 scaling-shape table reproduced.
  - One paragraph per archetype from ADR-0003 §3 stating whether the workload-shaped data **confirms or refutes** the row's mapping.
  - An explicit "did not pick a Phase 2D winner" confirmation.
  - Smoke vs final invocation log if smoke was used during iteration (so the next reviewer can see what was preliminary vs gating).

---

## Why this phase exists (preamble for the receiving instance)

The §6.10 finding (per-mark cost on Acme is 7× the synthetic, dominated by `CellCoordinate` allocation + `AHashSet` insert) is suggestive evidence for §9.3. Suggestive evidence is not a basis for kernel changes. Three things could be true:

1. The 7× gap is a real per-write tax that grows with cube size — §9.3 is correct and load-bearing.
2. The 7× gap is a fixed per-write structural cost that does *not* grow with cube size — §9.3 helps a little, but §9.2 (which attacks per-write work directly) wins on cost-benefit.
3. The 7× gap shrinks at scale because allocator reuse / cache effects amortize it — neither §9.3 nor §9.2 is the right Phase 2D pick, and something the workload bench surfaces is.

The Phase 2C data tells you which one. Without it, Phase 2D is a guess.

The combined-workflow bench is the one that distinguishes (1) from (2). If per-edit p99 grows superlinearly across the session, the dirty tracker's data structure is on the critical path and §9.3 is right. If p99 is flat, the per-write fixed cost dominates and §9.2 is right. The isolated-operation curves at 10× / 50× / 100× confirm which.

The TM1 stacked-sandbox pattern in the combined-workflow bench (snapshots held live across the session, not dropped) is the test that could reopen §9.5 — the Acme-scale §6.9 numbers extrapolate linearly, but real planning workloads hold 2–4 snapshots simultaneously and that's a regime the current data does not cover.

---

## Files you will most likely touch

| Why | File | Action |
|---|---|---|
| Add internal `build_scaled_acme_cube(scale)` + 3 public wrappers + 6 unit tests | [`crates/mc-fixtures/src/lib.rs`](../../crates/mc-fixtures/src/lib.rs) | additive only; do not modify existing functions |
| Extend isolated benches with 10× / 50× / 100× variants | [`crates/mc-core/benches/leaf_read_write.rs`](../../crates/mc-core/benches/leaf_read_write.rs), [`derived_read.rs`](../../crates/mc-core/benches/derived_read.rs), [`consolidated_read.rs`](../../crates/mc-core/benches/consolidated_read.rs), [`demo_path.rs`](../../crates/mc-core/benches/demo_path.rs), [`snapshot_clone.rs`](../../crates/mc-core/benches/snapshot_clone.rs) | extend; do not rewrite existing rows |
| New combined-workflow bench | `crates/mc-core/benches/combined_workflow.rs` | new file |
| Register `[[bench]]` entry for combined_workflow | [`crates/mc-core/Cargo.toml`](../../crates/mc-core/Cargo.toml) | add `[[bench]]` entry only; no dep changes |
| PERF.md §6.12–§6.14 + §7/§8/§9 updates (replace §6.12 stub) | [`../PERF.md`](../PERF.md) | append + targeted edits |
| Phase 2C completion report | `docs/reports/phase-2c-completion-report.md` | new from [`../templates/phase-completion-report.md`](../templates/phase-completion-report.md) |
| Save phase-2c criterion baseline | [`../reports/bench-data/phase-2c/`](../reports/bench-data/) | new dir, follow workflow in [`../reports/bench-data/README.md`](../reports/bench-data/README.md) |
| CURRENT_STATE Phase 2C closure | [`../CURRENT_STATE.md`](../CURRENT_STATE.md) | flip queued → shipping when done |
| MASTER_PHASE_PLAN Phase 2C row | [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) | flip status, append tag |

**Do not touch:**

- Anything in `crates/mc-core/src/`.
- Anything in `crates/mc-core/tests/`.
- Locked specs in `docs/specs/`.
- `rust-toolchain.toml`.
- Workspace `Cargo.toml` dep lines (only `crates/mc-core/Cargo.toml` may add `[[bench]]` entries).
- ADR-0003 (it's Accepted; amendments go in `0003-amendment-1.md` per its sunset clause). Phase 2C may consume the ADR but does not edit it.
- Existing Phase 1B / 2A / 2B bench rows (extend, don't rewrite).
- Existing `mc-fixtures` public functions.

---

## Reproducible commands you can rely on

These all exit 0 today on the inherited HEAD (commits `6ea58ab` + `9f7420c`).

```bash
cd /Users/edwinlovettiii/Projects/mc-v2

# (only if your shell didn't initialize rustup)
source $HOME/.cargo/env

# Pre-2C gate — must remain green throughout
cargo build --release --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                  # 210 / 0
cargo run --release --bin mc -- demo    # matches brief §4.6

# Restore phase-2b baseline locally so --baseline phase-2b works:
for bench in $(ls docs/reports/bench-data/phase-2b/); do
  mkdir -p "crates/mc-core/target/criterion/$bench"
  cp -R "docs/reports/bench-data/phase-2b/$bench/." "crates/mc-core/target/criterion/$bench/"
done

# Smoke (per-bench, sub-second, for iteration):
cargo bench -p mc-core --bench <name> -- \
  --warm-up-time 1 --measurement-time 1 --sample-size 10

# Diff against phase-2b baseline (full sample, gating for §6.12 numbers):
cargo bench -p mc-core --bench <name> -- --baseline phase-2b

# Save the post-2C baseline once at end of phase, per bench file:
cargo bench -p mc-core --bench <name> -- --save-baseline phase-2c
mkdir -p docs/reports/bench-data/phase-2c
# then copy target/criterion/<bench>/<id>/phase-2c/ subdirs into
# docs/reports/bench-data/phase-2c/<bench>/<id>/ (mirror the README workflow)
```

---

## Final checklist before you call Phase 2C done

- [ ] Internal generic `build_scaled_acme_cube(scale: u32)` lands `pub(crate)` (or private) with three public wrappers (`_10x`, `_50x`, `_100x`).
- [ ] **Scale-1× equivalence test** passes — internal builder at `scale = 1` reproduces Acme's brief §4.5.1 goldens for the anchor cell after `write_canonical_inputs`-equivalent loading.
- [ ] All three scaled builders ship with passing invariant tests (dim/measure/rule count, populated input-cell count within ±5% of target, hierarchy depth preserved).
- [ ] All isolated-operation benches run at 10× / 50× / 100× and have rows in `bench-data/phase-2c/`.
- [ ] Combined-workflow bench runs at 50× and 100×, reports per-edit + per-slice + per-snapshot percentiles, plus iteration-1/50/100 attribution.
- [ ] `bench-data/phase-2c/` populated with criterion JSON (no `raw.csv` — `default-features = false` is doing its job; check anyway).
- [ ] PERF.md §6.12 (replacing the stub) / §6.13 / §6.14 written from full criterion runs (sample-of-100), not smoke.
- [ ] PERF.md §7 / §8 / §9 updated; §9 priority deliberately not picked.
- [ ] All 210 existing tests + new fixture tests pass.
- [ ] All Phase 1B / 2A / 2B benches still produce numbers within ±10% of the `phase-2b` baseline.
- [ ] `cargo run --release --bin mc -- demo` still matches §4.6.
- [ ] Format / clippy / build / determinism 10× gates green.
- [ ] Completion report written from template; includes scaling-shape table, per-archetype confirmation/refutation paragraphs, and explicit no-winner-picked note.
- [ ] CURRENT_STATE.md + MASTER_PHASE_PLAN.md updated to flip Phase 2C from `proposed` → `complete`; tag is `phase-2c-workload-baseline` (or whatever the project owner picks at commit time).
- [ ] **You did NOT modify any file in `crates/mc-core/src/` or `crates/mc-core/tests/`.**
- [ ] **You did NOT pick a Phase 2D winner.** The §9 row priorities stay unspecified.
- [ ] **You did NOT commit, tag, or push.** The user does that after reading the review.

If you are uncertain at any point, the resolution order is:

1. The Phase 2C prompt above.
2. ADR-0003 (Accepted — Provisional).
3. PERF.md §6 / §7 / §8 / §9 (the Phase 2B baseline).
4. Phase 1A / 1B / 2A / 2B completion reports.
5. [`../specs/engine-semantics.md`](../specs/engine-semantics.md), [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md).
6. [`../../CLAUDE.md`](../../CLAUDE.md).
7. [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md).
8. Anything else.

If those don't resolve it: stop, write a SPEC QUESTION per CLAUDE.md §11, and wait. Don't guess.

---

## Operating principles for this handoff (read before starting)

These are the principles the previous instances and the project owner have been operating under. They're not new constraints; they're the reasons the project is in its current state.

**Measure before you optimize, every time.** Phase 2B shipped because §6.7 + §6.10 quantified the §9.4 hot path precisely. Phase 2C exists because §9.3's evidence is *suggestive*, not quantified at workload scale. The instinct to skip 2C and "just do the bitset" is exactly the failure mode the project has avoided so far. Don't break the streak.

**The kernel source is locked between phases.** Every phase's "no `crates/mc-core/src/` modified" claim is a forcing function for the next phase to do its measurement work in fixtures and benches. Phase 2B was the *one* phase that touched the kernel, and it touched two functions, surgically, with a kernel unit test added. That's the bar. Phase 2C is back to the locked-source rule.

**A bench is a contract, not a draft.** Every benched row should be reproducible by anyone with the repo. Goldens asserted before timing. Cardinalities asserted in pre-flight. Stderr-emitted invariants. The "future maintainer cannot accidentally turn this microbench into something else" comment in the existing benches is load-bearing — match its discipline.

**Provisional is not permanent.** ADR-0003's sunset clause is real. If you find yourself reasoning *from* ADR-0003 as if it were settled, stop and check whether the data you're producing should trigger an amendment. The ADR is a working hypothesis, not a constitution.

**Acme is calibration, not production.** Real planning workloads — TM1-shaped or otherwise — are 10–100× the dimension count, 5–10× the hierarchy depth, and 1000–10000× the cell count of Acme. 100× Acme is the *upper bound of calibration*, not the lower bound of production. Optimizations that work at 25K cells and break at 25M are the failure mode you're trying to avoid by measuring across a curve.

**Combined workflows reveal what isolated benches hide.** The single most likely thing Phase 2C will find is a per-edit cost that's flat in isolation but grows across a session because some cache, dirty set, or graph structure accumulates. That finding will reorder §9 in ways no isolated bench can predict. Take the combined-workflow bench seriously — it's the highest-information-density piece of the phase.

**Smoke is for iteration; full sample is for the gate.** Criterion's sample-of-100 / 5s-window discipline is what makes the numbers in PERF.md §6 trustworthy across machines and across project lifetimes. Smoke runs are fine while you're shaping the bench; the moment you record a number in PERF.md or the completion report, it must come from a full run. ADR-0002's principle ("perf assertions belong in benchmarks, not tests") applies recursively: a number worth recording is a number worth running fully.

**Do not pick the next optimization.** Phase 2C's deliverable is the data. Phase 2D's deliverable is the pick. Conflating them is how projects ship optimizations that don't help and skip ones that would.

---

*End of Phase 2C handoff. Phase 2C source work is its own session — do not start it in the same session as the Phase 2 closure work that produced this handoff.*
