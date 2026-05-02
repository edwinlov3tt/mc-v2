# Phase 2D Handoff — SCAFFOLD

> **Status:** SCAFFOLD. Pre-staged while Phase 2C bench work was in flight, so the scope-specific sections can be filled in mechanically once PERF.md §6.14 names a winner. **Move this file to `docs/handoffs/phase-2d-handoff.md`** once the project owner has read PERF.md §6.14 and decided which §9 row Phase 2D targets.
>
> **What's already filled in:** the static framing (what Phase 2C ended with, hard rules, validation gate, files-not-to-touch, completion report format, operating principles). All borrowed/adapted from the proven Phase 2B handoff structure.
>
> **What's `<TODO>`:** the scope section, the implementation options, the acceptance gate (which specific bench row gets the new ceiling), the per-archetype impact paragraph. **Three branch templates** are provided in the scope section for the three most likely §6.14 outcomes — pick the matching branch, fill the row-specific numbers, delete the other two branches.

---

## Where Phase 2C ended

- **Phase 2C commit / tag:** `<TODO: HASH>` — *bench: Phase 2C production-shaped workload benchmarks* — tag `<TODO: TAG>` (likely `phase-2c-workload-baseline`).
- **Test status:** `<TODO: 216>` / 0 passing, 10/10 deterministic.
- **Demo:** `cargo run --release --bin mc -- demo` matches brief §4.6.
- **Gates green:** build / fmt / clippy / test / demo / bench (9 bench files at this point: 5 Phase 1B + 3 Phase 2A + 1 Phase 2C combined-workflow, with scaled variants threaded through).
- **Toolchain:** Rust 1.78 pinned. **Do not bump without explicit approval.**
- **Cargo.lock pins (still load-bearing):** `clap → 4.4.18`, `clap_lex → 0.6.0`, `half → 2.4.1`. Do not run `cargo update`.
- **PERF.md §6.14 headline finding:** `<TODO: paste the §6.14 headline-finding paragraph from PERF.md here verbatim. This is the data Phase 2D's scope is reading from. It MUST be present in this handoff so the receiving instance doesn't have to interpret §6.14 themselves — interpretation is the project owner's call, made before this handoff is filled in.>`
- **ADR-0003** ([`../decisions/0003-workload-sketch.md`](../decisions/0003-workload-sketch.md)) — Accepted — Provisional. Defines the perception-threshold gates Phase 2D's optimization is measured against.

For the full Phase 2C audit read [`../reports/phase-2c-completion-report.md`](../reports/phase-2c-completion-report.md). For the bench baseline this phase diffs against, see [`../reports/bench-data/phase-2c/`](../reports/bench-data/phase-2c/) and its [README](../reports/bench-data/phase-2c/README.md).

The non-negotiable operating manual is [`../../CLAUDE.md`](../../CLAUDE.md). The master roadmap is [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) — Phase 2D is the optimization sub-phase Phase 2C measured for; do not start Phase 2E / Phase 3 work in this phase.

---

## Phase 2D prompt (verbatim — this is your contract)

> We are starting MarketingCubes Phase 2D: `<TODO: short title — e.g. "Hierarchy Mark Closure (Bitset-Backed Dirty Tracker)" if §9.3 / "Per-Dim Leaf-Flag Cache" if §9.2 / etc.>`
>
> **Context.** Phase 2C shipped the production-shaped workload baseline at `bench-data/phase-2c/`. PERF.md §6.14 surfaced `<TODO: 1-line summary of the §6.14 headline finding>`. This phase is the targeted kernel optimization that addresses the highest-value row in §6.14, with the cleanest source surface and the smallest blast radius.
>
> **Goal.** `<TODO: 1-2 sentences. Templates by branch:>`
>
> ### Branch A — §9.3 (super-linear per-edit p99 across session)
>
> `<TODO: "Eliminate the per-mark CellCoordinate-allocation + AHashSet-insert cost in cube.rs::write that PERF.md §6.13.3 measured growing from N ns/mark at iteration 1 to M ns/mark at iteration 100. Likely path: replace the AHashSet<CellCoordinate>-backed dirty tracker with a bitset keyed by per-dim element index, so a mark becomes a constant-time bit-set instead of a hash-and-insert.">`
>
> ### Branch B — §9.2 (flat per-edit p99, fixed cost dominates at scale)
>
> `<TODO: "Eliminate the per-write per-dim element-children scan in cube.rs::is_consolidated_coord that PERF.md §6.12.1 measured as the dominant fixed cost at 100×. Cache is_leaf_in_default_hierarchy: bool on each Element at cube-build time so is_consolidated_coord becomes a O(dims) bit lookup instead of a O(dims × children) walk.">`
>
> ### Branch C — something §6.14 surfaced that wasn't in §9 yet
>
> `<TODO: name the operation, the §6.X row, and the source location. Add a new §9.X candidate description to PERF.md alongside the kernel change.">`
>
> ### Branch D — §6.14 says no Phase 2D needed
>
> `<TODO: if §6.14 shows everything stayed linear and well under perception thresholds, this phase becomes a one-line "Phase 2 exits, proceed to Q2 toolchain bump + Phase 3A." No source change, no handoff scope. Mark Phase 2 complete in the master plan and skip to Phase 3A. Delete this scaffold.">`
>
> **Phase 2D scope:** `<TODO: per branch, 3–5 numbered scope items. Pattern from Phase 2B:>`
>
> 1. `<TODO: source change in cube.rs (and at most one other src/ file).>`
> 2. `<TODO: kernel unit test confirming the change preserves observable behavior (the Phase 2B `consecutive_recompute_reads_match_phase_2b` template applies).>`
> 3. `<TODO: re-run Phase 2C benches against --baseline phase-2c.>`
> 4. `<TODO: PERF.md update — close the relevant §9.X row, append §6.15 verification subsection, etc.>`
>
> **Hard rules:**
>
> - Source change confined to `crates/mc-core/src/<TODO: list specific files — e.g. cube.rs + dirty.rs for §9.3, or element.rs + dimension.rs + cube.rs for §9.2>`. No other source file may change.
> - No new external dependency.
> - No async / threads / rayon / tokio / serde / external storage.
> - The `Cube` public API (the symbols re-exported from `crates/mc-core/src/lib.rs`) MUST NOT change. Internal helper signatures may.
> - All `<TODO: 216>` existing tests must still pass.
> - All Phase 1B / 2A / 2B / 2C benches must still build and run.
> - Do not bump `rust-toolchain.toml` without explicit approval.
> - Do not run `cargo update`.
> - Do not touch `docs/specs/`. The brief and engine-semantics doc are locked.
> - Do not start Phase 2E or Phase 3. The deliverable is one targeted change + the bench data verifying it.
>
> **Acceptance gate (the one thing that determines done):**
> `<TODO: name the specific bench row from PERF.md §6.12 / §6.13 / §6.14 that must move below a specific threshold. Templates:>`
>
> - **Branch A:** `<TODO: "PERF.md §6.13.1 combined_workflow/50× per-edit p99 must drop from N µs (Phase 2C baseline) to M µs (target = ADR-0003 §3 100 ms slice budget at session length 100, divided per-edit).">`
> - **Branch B:** `<TODO: "PERF.md §6.12.1 write_input_leaf at 100× must drop from N µs to M µs (target = matching the 50×→100× linear extrapolation, removing the fixed-cost dominator).">`
> - **Branch C / D:** `<TODO>`
>
> **Validation gate before reporting done:**
> Run, in order:
> - `cargo fmt --check --all` (exit 0)
> - `cargo clippy --workspace --all-targets -- -D warnings` (exit 0)
> - `cargo build --release --workspace` (zero warnings)
> - `cargo test --workspace` (must remain `<TODO: 216>` / 0)
> - `cargo run --release --bin mc -- demo` (must match brief §4.6)
> - `cargo bench --workspace --baseline phase-2c` (acceptance gate row meets target; full table re-recorded in PERF.md)
> - 10 consecutive `cargo test --workspace -q` (still deterministic)
>
> **PERF.md update requirements:**
> - Update the relevant §6.X row(s) with the new median + range.
> - Add a §6.15 "Phase 2D verification" subsection with a before/after diff for every row that improved.
> - Update §9.X (the row Phase 2D closed) from "data-justified" to "closed in Phase 2D (commit `<hash>`)".
> - Update §10's files-changed manifest.
>
> **Completion report format:** `<TODO: same shape as Phase 2B completion report. Pre-fill the structural shell; the implementing instance fills the bench numbers + scope-specific narrative.>`
>
> Do NOT commit or tag. The user reviews first.

---

## Context the prompt above does NOT spell out

`<TODO: 4–6 sections analogous to Phase 2B handoff §A–§H. Templates by branch:>`

### Branch A — §9.3 specifics

#### A. The exact code being optimized

[`crates/mc-core/src/dirty.rs`](../../crates/mc-core/src/dirty.rs) holds `DirtyTracker { set: AHashSet<CellCoordinate> }`. Every mark is a CellCoordinate hash + insert. PERF.md §6.13.3 attribution table shows `<TODO: actual numbers from §6.13.3>`.

#### B. Why the cost is structural

`<TODO: explain CellCoordinate is a SmallVec<[ElementId; 6]>; hashing it walks 6 u64s; AHashSet has its own per-insert overhead; at 215 marks per Acme write this is N µs and the constant doesn't shrink at scale.>`

#### C. Implementation options

`<TODO: 2–3 options with the recommended path. The Phase 2B Option A → Option C pattern of "smallest source change" → "cleanest abstraction" → "most ambitious" applies. For §9.3 likely:`

- **Option A — bitset per dim element index.** `DirtyTracker` carries `Vec<BitVec>` where `bv[dim_idx]` is a per-dimension-element bitset; a CellCoordinate is "marked" if every dim's bit is set. **Smallest change; biggest unknown is whether the bitset's representation supports the existing `is_dirty(coord)` semantics cleanly.**
- **Option B — flat bitset over the cell-product space.** `BitVec` of length = product of dim cardinalities. **Simpler representation; potentially unwieldy memory at high cardinality (50 × 12 × 5 × 7 × 6 = 126K bits at Acme; 12.6 M at 100×).**
- **Option C — sparse representation that adapts.** Use a bitset for hot dims, fall back to AHashSet for cold dims. **Most implementation work; defers a real win.**

`>`

### B / C — §9.2 specifics

`<TODO: parallel structure. The optimization is simpler — cache a bool on each Element at build time and short-circuit is_consolidated_coord. Smaller blast radius than §9.3; smaller projected win.>`

### D. Phase 2C regression guard

The Phase 2C baseline established `cargo bench --baseline phase-2c` as the diff workflow. After your change:

- §6.12 isolated rows at 10× / 50× / 100× should improve in the targeted operation; others should be flat (within ±10% noise).
- §6.13 combined-workflow rows should improve in per-edit percentiles if Branch A; in fixed-cost rows if Branch B.
- §6.14 scaling-shape table should re-classify the targeted row; others should keep their classification.

Any **regression** beyond noise is a stop-the-line signal — investigate before recording.

### E. Phase 2D's own follow-ups

`<TODO: what Phase 2E or Phase 3A should know. If Branch A succeeds and per-edit p99 drops to flat across session, snapshot the new §6.14 finding and ask whether another §9 row remains. If Branch B succeeds, same exercise. If neither: §6.14 + §9 likely point at Phase 3A as the next phase.>`

---

## Pointers to existing files you will most likely touch

| Why | File | Action |
|---|---|---|
| The optimization site | `<TODO: cube.rs / dirty.rs / element.rs / dimension.rs depending on branch>` | per-branch source change |
| Add a kernel unit test for the new behavior | `crates/mc-core/src/<file>.rs` `mod tests` | append one test, model after `consecutive_recompute_reads_match_phase_2b` |
| Update bench numbers + verification subsection | [`../PERF.md`](../PERF.md) | append §6.15 + close §9.X + update §10 |
| Phase 2D completion report | `docs/reports/phase-2d-completion-report.md` | new file (use [`../templates/phase-completion-report.md`](../templates/phase-completion-report.md)) |
| Save phase-2d criterion baseline | [`../reports/bench-data/phase-2d/`](../reports/bench-data/) | new dir, follow workflow in [`../reports/bench-data/README.md`](../reports/bench-data/README.md) |
| Status flip in master plan + state | [`../CURRENT_STATE.md`](../CURRENT_STATE.md), [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) | flip Phase 2D from `proposed` → `complete` |

**Do not touch:**

- `crates/mc-core/src/` — any file other than the 1–2 the chosen branch authorizes.
- `crates/mc-core/tests/` — the contract test suite is locked.
- `crates/mc-core/benches/` — extending PERF.md does not require new bench code.
- `crates/mc-fixtures/src/lib.rs` — the public fixtures are a shared contract.
- `docs/specs/` — locked.
- `rust-toolchain.toml` — pinned.
- ADR-0003 — Accepted; amendments go in `0003-amendment-N.md`.

---

## Reproducible commands you can rely on

```bash
cd /Users/edwinlovettiii/Projects/mc-v2

# Pre-2D gate — must remain green throughout
cargo build --release --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                 # <TODO: 216> / 0
cargo run --release --bin mc -- demo

# Restore phase-2c baseline locally so --baseline phase-2c works:
for bench in $(ls docs/reports/bench-data/phase-2c/); do
  [ "$bench" = "README.md" ] && continue
  mkdir -p "crates/mc-core/target/criterion/$bench"
  cp -R "docs/reports/bench-data/phase-2c/$bench/." "crates/mc-core/target/criterion/$bench/"
done

# Pre-2D bench check (sanity — every row should match the phase-2c baseline):
cargo bench -p mc-core -- --baseline phase-2c

# Quick smoke during 2D development:
cargo bench -p mc-core --bench <name> -- \
  --warm-up-time 1 --measurement-time 1 --sample-size 10

# Full diff against phase-2c baseline (gating for §6.15 numbers):
cargo bench -p mc-core --bench <name> -- --baseline phase-2c

# Save the post-2D baseline once at end of phase:
cargo bench -p mc-core --bench <name> -- --save-baseline phase-2d
```

---

## Final checklist before you call Phase 2D done

- [ ] Single chosen branch implemented (Branch A / B / C from §"Phase 2D scope" above), with the choice documented in the completion report.
- [ ] Source change confined to the branch-authorized files.
- [ ] No public symbol from `crates/mc-core/src/lib.rs` removed or renamed.
- [ ] No new external dependency.
- [ ] All `<TODO: 216>` tests still pass.
- [ ] 10 consecutive `cargo test --workspace -q` runs identical.
- [ ] `cargo run --release --bin mc -- demo` still matches §4.6.
- [ ] **Acceptance gate met:** `<TODO: specific bench row meets specific threshold>`.
- [ ] No Phase 2C bench row regressed beyond run-to-run noise (~10%).
- [ ] PERF.md updated; §6.15 verification subsection added; §9.X closure-noted; §10 manifest updated.
- [ ] Completion report at `docs/reports/phase-2d-completion-report.md`.
- [ ] CURRENT_STATE.md and MASTER_PHASE_PLAN.md updated to flip Phase 2D from `proposed` → `complete`.
- [ ] **You did NOT commit, tag, or push.** The user does that after reading the review.
- [ ] **You did NOT start Phase 2E / 3 / any later phase.**

If you are uncertain at any point, the resolution order is:

1. The Phase 2D prompt above (the project-owner-filled version).
2. PERF.md §6.12 / §6.13 / §6.14 (the data justifying this phase).
3. ADR-0003 (Accepted — Provisional; sunset 2026-11-01).
4. Phase 2C completion report.
5. Earlier completion reports (1A / 1B / 2A / 2B).
6. `docs/specs/engine-semantics.md`, `docs/specs/phase-1-rust-kernel-build-brief.md`.
7. `CLAUDE.md`.
8. `docs/roadmap/MASTER_PHASE_PLAN.md`.
9. Anything else.

If those don't resolve it: stop, write a SPEC QUESTION per CLAUDE.md §11, and wait. Don't guess.

---

## Operating principles (unchanged from Phase 2B / 2C)

**Measure before you optimize.** Phase 2D exists because Phase 2C measured what 2D should optimize. Do not change the kernel without the §6.14 row that justifies it being cited in your scope.

**Source-locked between phases.** This phase is the rare one that touches the kernel — surgically, with a unit test, no surrounding cleanup. Phase 2E (if any) is back to source-locked.

**A bench is a contract, not a draft.** Phase 2D's verification rows must be reproducible by anyone with the repo. Use the `phase-2c` baseline; save the `phase-2d` baseline; commit the JSON.

**Do not pick the next optimization.** Phase 2D's deliverable is the source change + its verification. If §9 still has rows after this phase, the next sub-phase is its own pick — driven by the post-2D bench data, not by the §9 list.

---

*This scaffold should be moved to `docs/handoffs/phase-2d-handoff.md` once the project owner has filled in the branch + scope-specific TODOs based on PERF.md §6.14. Until then it lives in `docs/reports/` as a holding doc.*
