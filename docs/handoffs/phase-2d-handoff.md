# Phase 2D Handoff — Bitset-Backed Dirty Tracker (§9.3)

> **Audience:** the Claude Code instance running in
> `/Users/edwinlovettiii/Projects/mc-v2/` that picks up Phase 2D.
> **You inherit a green Phase 2C** (commit `789db15`, tag
> `phase-2c-workload-baseline`, backfill at `96cca75`).
>
> **Branch picked from PERF.md §6.14: Branch A — §9.3.** The
> `load_canonical_inputs` super-linear cliff between 10× (4.33×/write)
> and 50× (19.7×/write) is the load-bearing finding, attributable to
> the `AHashSet<CellCoordinate>` dirty tracker's per-mark hash-and-insert
> cost growing nonlinearly as the set saturates. Phase 2D replaces it
> with a Cartesian-product flat bitset keyed by linearized coordinate
> index, giving O(1) mark/check independent of set size.
>
> **Hard rule:** This phase touches `crates/mc-core/src/`. The change
> is surgical (one or two files, ~150–250 lines) and gated behind a
> kernel unit test that proves observational equivalence with the
> existing tracker. The Phase 2B template (Arc<Hierarchy>) is the
> precedent: small, source-locked, justified by data, accompanied by
> a kernel unit test.

---

## Where Phase 2C ended

- **Phase 2C commit / tag:** `789db15` — *bench: complete Phase 2C workload-shaped benchmark baseline* — tag `phase-2c-workload-baseline`. Backfill commit at `96cca75`.
- **Test status:** 216 / 0 passing across all targets. 10/10 deterministic.
- **Demo:** `cargo run --release --bin mc -- demo` matches brief §4.6.
- **Gates green:** build / fmt / clippy / test / demo / bench.
- **Toolchain:** Rust 1.78 pinned in [`../../rust-toolchain.toml`](../../rust-toolchain.toml). **Do not bump without explicit approval.**
- **Cargo.lock pins (still load-bearing):** `clap → 4.4.18`, `clap_lex → 0.6.0`, `half → 2.4.1`. Do not run `cargo update`.
- **PERF.md §6.14 headline finding (verbatim):**
  > `load_canonical_inputs` shows a super-linear cliff between 10× (4.33×/write) and 50× (19.7×/write). Total ingest at 50× is **23× over the ADR-0003 patience-limit gate** (231 s vs 10 s). 100× was abandoned mid-run after criterion estimated > 38 minutes for a single 10-sample row. **This is the single data point Phase 2D's pick should anchor on.**
  >
  > **Why the cliff is §9.3 evidence, not §9.2.** §9.2 attacks per-write fixed cost (permission / type / lock / NaN / version / store-write / revision-bump). Those costs scale O(1) with cube size — fixing them drops every per-write cost by a constant amount but doesn't bend the curve. §9.3 attacks the per-mark hash-and-insert cost on the `AHashSet<CellCoordinate>` dirty tracker. As the dirty set grows (during bulk ingest, dirty grows from 0 to **305,039 entries (measured)** at 50× — earlier handoff drafts projected ~750 K / ~1.5 M for 50× / 100× from a populated-cell estimate that ignored mark-merging across measures; the actual measured 50× steady state is ~305 K), each subsequent insert costs more — AHashSet rehashes, cache locality drops, hash-collision probability climbs. This compounds nonlinearly. **A bitset-backed dirty tracker keyed by linearized coordinate index (per-dim strides over per-dim element-index maps) would make every insert O(1) and independent of set size, exactly the thing the cliff data names.**
  >
  > **Why combined-workflow flatness doesn't contradict.** The combined-workflow per-edit total cost is flat **within a session** at 50× (≈ 422 → 419 → 422 µs amortized at iter 1 / 50 / 100, computed as `edit_time ÷ dirty_delta`; see PERF.md §6.13.2 unit caveat — the bench's `eprintln!` labels the divisor unit "ns" but with `edit_time ≈ 2.1 ms` and `dirty_delta ≈ 5`, the result magnitude is µs, not ns). That's because the dirty set was *already* fully populated from the bulk-load that preceded the session — `final dirty_set = 305,039` at session start (after bulk-load), and stays in the same range across the session. The *cliff* is in the bulk-load itself, where the dirty set grows from 0; once it's saturated, per-edit total cost stabilizes. **Caveat:** within-session flatness is consistent with §9.3 but does *not* isolate the AHashSet-insert component — it amortizes total per-edit work (permission / type / lock / NaN / store-write / revision-bump + ancestor-mark walk + dependency rev-edge walk + AHashSet insert) over `dirty_delta` marks per edit. The load-bearing §9.3 evidence is the cross-scale ingest cliff itself (§6.12.7), independent of this within-session number.

- **ADR-0003** ([`../decisions/0003-workload-sketch.md`](../decisions/0003-workload-sketch.md)) — Accepted — Provisional. Defines the perception-threshold gates this optimization is measured against. The relevant gate for Phase 2D's acceptance is the ADR-0003 "patience limit" (≤ 10 s for bulk imports of typical-size datasets).

For the full Phase 2C audit read [`../reports/phase-2c-completion-report.md`](../reports/phase-2c-completion-report.md). For the bench baseline this phase diffs against, see [`../reports/bench-data/phase-2c/`](../reports/bench-data/phase-2c/) and its [README](../reports/bench-data/phase-2c/README.md).

---

## Phase 2D amendment — `WritebackResult.invalidated` semantic correction (2026-05-02)

> **Authoritative — read this BEFORE the verbatim prompt below.** This amendment was raised by the implementing Claude Code instance via SPEC QUESTION on 2026-05-02 and is approved by the project owner with PM/spec-maintainer review. Where this amendment conflicts with the verbatim prompt, **the amendment wins**. The verbatim prompt is preserved unchanged for audit-trail purposes.

### A.1 What the implementer found

After implementing the Cartesian-product flat bitset per the original Phase 2D scope, the 50× `load_canonical_inputs` bench moved by **+4%** (240 s vs 230.8 s baseline — within criterion noise). The bitset was correct (kernel equivalence test + 222/222 integration tests pass) but the *diagnosis* was wrong: replacing the `AHashSet`'s hash-and-insert with a bitset doesn't address the actual bottleneck.

The actual bottleneck is at [`crates/mc-core/src/cube.rs`](../../crates/mc-core/src/cube.rs) (Phase 1A's `Cube::write` writeback path), specifically the line that materializes `WritebackResult.invalidated`:

```rust
let invalidated: Vec<CellCoordinate> = self.dirty.iter().cloned().collect();
```

This collects the **entire current dirty set** on every write — `O(|cumulative dirty|)` per write. Across an N-write bulk-load, that's `O(N · cumulative_dirty)` ≈ `O(N²)` because the cumulative dirty set grows monotonically with N until the whole cube is saturated. **That's the §6.14 cliff.** The bitset alone can't fix it because the bitset isn't the hot path during bulk load — *materializing the full set on every write is*.

### A.2 The spec ambiguity that produced the Phase 1A bug

`WritebackResult.invalidated` has three authoritative spec sites; one is ambiguous:

| Source | Says | Reading |
|---|---|---|
| Brief [`docs/specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) §3.18, type doc on `WritebackResult.invalidated` (line 1214–1216) | "Coordinates marked dirty by **this write** — both rule dependents and hierarchy ancestors. Order is unspecified; equality is by set content." | **Marginal** |
| Brief writeback algorithm step 12 (line 1259) | "Return `WritebackResult` with the invalidated set." | Marginal-leaning (the just-computed set) |
| Brief compact pseudocode (line 1938) | `Return WritebackResult { invalidated: <full dirty set> }.` | **Ambiguous prose** — Phase 1A read it as `cube.dirty` (cumulative); reads equally as "the full set computed in steps 3–6 above" (marginal) |
| Semantics doc [`docs/specs/engine-semantics.md`](../specs/engine-semantics.md) §13.2 inline comment (line 1011) | "cells dirtied **by this write**" | **Marginal** |
| Semantics doc §13.4 worked example (line 1052–1054) | Enumerates only the 5 derived measures + the same 5 at consolidated ancestors of the single Spend coord (~10 coords, NOT the prior 17,820 cumulative dirty) | **Marginal** |
| I-WB-7 (semantics doc line 1034) | "returns the list of invalidated coordinates so callers can pre-warm caches if they care" | **Marginal** (cache pre-warming the cumulative dirty set on every write is incoherent) |

Per **CLAUDE.md §0** hierarchy of authority:
- "Brief wins for what to implement (types, signatures)" → the brief's *type doc* wins over its own *pseudocode shorthand* within the brief.
- "Semantics wins for what a concept means" → `invalidated` means "by this write."

**Verdict:** five of six sources are unambiguous on the marginal reading; one (the compact pseudocode) is genuinely ambiguous and Phase 1A picked the wrong gloss. The cumulative reading was a Phase 1A misimplementation. **The marginal reading is the intended semantics.**

### A.3 What this amendment authorizes

The Phase 2D scope is **expanded** to include the writeback-path semantic correction in `cube.rs`. Specifically:

- `Cube::write` (cube.rs around lines 892–943, the `WritebackResult.invalidated` construction) is **in scope** for Phase 2D edits, in addition to the original `dirty.rs` + `cube.rs` (struct field + builder plumbing) + optional `cube_shape.rs`. **Frame this in the completion report as "Phase 1A semantic bugfix surfaced by Phase 2D's bench gate," not as scope creep.**
- The semantic change is: `invalidated` now contains exactly the coords that transitioned **clean → dirty during this single `write()` call** — including the freshly-written input coord if it transitioned, all rule dependents (closure under `deps.reverse_edges`) that newly transitioned, and all hierarchy ancestors that newly transitioned. Excludes any coord that was *already* dirty before this write.
- The implementation captures marginal coords by checking `dirty.is_dirty(&c)` *before* each `mark` call and pushing only the clean→dirty transitions into `invalidated`. The bitset's O(1) `is_dirty` is what makes this O(per-write fan-out) instead of O(per-write fan-out × |cumulative dirty|).
- **Public API shape is unchanged.** `WritebackResult.invalidated: Vec<CellCoordinate>`; same field name, same type, same struct, same re-export. Only the *contents* change.

### A.4 Why this is not scope creep

Three reasons the project owner approves this as in-scope for Phase 2D rather than as a separate Phase 2D.5:

1. **The bitset alone does not close the gate.** The handoff's acceptance gate (`load_canonical_inputs/50x ≤ 50 s`) cannot be hit without this fix; the bitset moves the row by 4%. Either Phase 2D ships both, or Phase 2D ships neither and we open Phase 2D.5 for the writeback fix immediately. Bundling avoids a rollback path and keeps the §6.14 finding closed in one phase.
2. **The fix is required by the bitset to be efficient.** The marginal-only construction relies on the bitset's O(1) `is_dirty` check (per-coord, `D` hash lookups + bit test). Under the AHashSet representation, marginal-only would still help (avoids the cumulative collection) but every per-write `is_dirty` would be a hash lookup that gets slower as the set grows — partial fix only. The bitset and the semantic correction reinforce each other.
3. **The diagnosis is the contribution.** Phase 2D's *real* deliverable, beyond the source change, is correctly attributing the §6.14 cliff to the write-result construction rather than to the dirty tracker's hash cost. Splitting the diagnosis from the fix would obscure that.

### A.5 Required A/B isolation

Before the Phase 2D completion report is finalized, the implementer MUST report the **isolated effect** of the two changes. Concretely, run the `load_canonical_inputs/10x` and `/50x` rows under three configurations:

1. **Baseline** (`phase-2c` tag): cumulative `invalidated` + AHashSet tracker. (Already in `bench-data/phase-2c/`.)
2. **Bitset only**: bitset tracker + cumulative `invalidated` (revert just the writeback fix). Report wall-clock.
3. **Bitset + writeback fix** (the shipping Phase 2D state): both changes applied. Report wall-clock.

Document configuration 2's numbers in the Phase 2D completion report's §"Source attribution" section. The phrasing should be either:

- "The bitset alone moved 50× ingest by N% (still M× over the gate). The writeback-result semantic fix was the load-bearing change for the gate." (this is what the implementer's preliminary numbers suggest), OR
- "Both changes are required to clear the gate; reverting either keeps 50× over budget." (if the A/B numbers say so), OR
- Whatever the data actually shows.

**Do not over-credit the bitset if the writeback fix is the load-bearing piece.** The PERF.md §6.15 verification table must distinguish the two.

If the A/B shows the bitset is *not* a meaningful contributor at any calibration scale (i.e., configuration 2 ≈ configuration 3), the project owner will consider whether to keep the bitset or revert it for code-volume / complexity reasons in a follow-up review. Phase 2D ships with the bitset by default unless the data argues otherwise.

### A.6 Required correctness tests

Add these tests in addition to the kernel `bitset_tracker_observationally_equivalent_to_ahashset` test from item 4 of the original prompt. Place them in `crates/mc-core/tests/writeback.rs` (or a new `tests/writeback_invalidated.rs` if it reads cleaner):

**Test A — Fresh write on a clean cube reports the marginal closure.** Build Acme. Call `clear_all` to ensure dirty is empty. Write a single Spend cell. Assert `result.invalidated` length matches the brief §10.1 fan-out (≤ 215). Assert membership: contains Clicks/Leads/Customers/Revenue/Gross_Profit at the same leaf coord plus their hierarchy ancestors. Assert `cube.dirty().len()` matches the same number (no other prior dirt to add).

**Test B — Repeated identical write returns empty `invalidated` for already-dirty dependents.** Build Acme. Write Spend at coord C. Capture `invalidated_first`. Without intervening reads, write Spend at the same coord C with a different value. Assert `invalidated_second.is_empty()` (or contains only the input coord C if it transitions clean→dirty by your impl convention — match whatever Test A established). The point: dependents that were *already* dirty from the first write must not appear in the second `invalidated`. The full `cube.dirty` is unchanged-or-larger; `WritebackResult.invalidated` for the second write is much smaller.

**Test C — After recompute, transitions clean → dirty are reported again.** Build Acme. Write Spend at C; capture `invalidated_first`. Read Revenue at C (forces recompute → Revenue@C becomes clean again). Write Spend at C again. Assert `invalidated_second` includes Revenue@C (it transitioned clean→dirty). The membership check is the load-bearing assertion: `invalidated` is a marginal *transition* set, not a cumulative set.

**Test D — Bulk-ingest preserves the §10.1 per-write bound.** Run `write_canonical_inputs(&mut cube, &refs)` (the 2,520-write bulk loader). For each individual write, assert `result.invalidated.len() ≤ 215` (or whatever §10.1's documented per-write bound is). Assert `cube.dirty().len()` after the full ingest grows monotonically (the cumulative dirty *should* grow as expected). The point: marginal `invalidated` stays bounded per-write even when the cumulative dirty grows large. This is the test that, had it existed, would have caught the Phase 1A bug originally.

**Test E — `mc-cli demo` printed dirty count is the marginal one.** This is more of a smoke test than a kernel unit test — assert `cargo run --release --bin mc -- demo` prints `"{N} dependent cells dirtied. (bounded per brief §8)"` with `N` matching the §10.1 marginal bound (small two-digit number), NOT the cumulative count (~17,820 + small). The exact number is implementation-dependent ("brief §8 says exact N depends on impl; bounded"); the assertion is on order of magnitude — `N < 215`. Implementing this as a `#[test]` is optional; if it's clearer as a manual check noted in the completion report, do that instead.

### A.7 Bench-preflight wording fix

Three benches print preflight diagnostics that conflated `dirty_set` and `invalidated.len`. Update their `eprintln!` strings + comments to make the distinction explicit. Specifically:

- [`crates/mc-core/benches/dirty_propagation.rs`](../../crates/mc-core/benches/dirty_propagation.rs) line 105–107: the line should now show `dirty_set: A -> B (delta D); invalidated.len=D` (delta should equal `invalidated.len` under the corrected semantics — that's the validation).
- [`crates/mc-core/benches/hierarchy_mark.rs`](../../crates/mc-core/benches/hierarchy_mark.rs) line 73–74: same fix; `dirty_set_delta` should equal `invalidated.len`.
- [`crates/mc-core/benches/combined_workflow.rs`](../../crates/mc-core/benches/combined_workflow.rs) lines 173, 360 + the `final invalidated.len` field name: rename to `last_write_invalidated_len` or similar; the `final dirty_set` field stays (cumulative is meaningful for the cube state, just not for `WritebackResult.invalidated`). Update the surrounding `eprintln!` to clarify the two columns: cumulative cube dirty vs marginal-per-write.

These are bench-side comment / label changes — no behavior change. They prevent future maintainers from re-confusing the two quantities.

### A.8 Documentation update requirements

PERF.md has multiple sites that *rationalized* the bug instead of catching it. The completion report (and PERF.md §6.15) must annotate these as historical artifacts of the Phase 1A bug. Specifically:

- PERF.md §6.4 (line ~288–297): the `dirty_set: 17820 -> 17825 (delta 5); invalidated.len=17825` block + the explanatory note "`invalidated.len` (17825) is the full transitive closure including hierarchy ancestors. Same shape as the demo CLI's 19919 figure" — append a note: *"This was the Phase 1A cumulative reading of `WritebackResult.invalidated`; corrected in Phase 2D — see §6.15. Under the corrected semantics, `invalidated.len` equals the per-write delta (5 in this case)."*
- PERF.md §6.13 (line ~806): the column header "Final invalidated.len (last-iter write)" — append parenthetical: *"under Phase 2D's corrected marginal semantics; the phase-2c row's much-larger value reflected the Phase 1A cumulative bug."*
- PERF.md §6.14 attribution narrative: rewrite to clearly attribute the cliff to the cumulative-collection bug and the bitset's role as enabling-but-not-load-bearing (or load-bearing if the A/B says so).
- `mc-cli/src/main.rs:189` printed line `"{N} dependent cells dirtied. (bounded per brief §8)"` — wording stays; the new (smaller) `N` is more consistent with "bounded per brief §8" than the old (cumulative) `N` was. No code change beyond what the semantic correction already produces.
- `docs/handoffs/phase-2c-handoff.md:72` and `docs/reports/phase-2c-completion-report.md:129,314` "Final invalidated.len" report fields: leave the historical wording, append a footnote pointing at this amendment + PERF.md §6.15.

### A.9 Updated acceptance gate framing

The original acceptance gate (`load_canonical_inputs/50x ≤ 50 s`) stays. The implementer's preliminary A/B suggests the gate will be **beat by ~47×** (1.06 s) under the bitset + writeback-fix combination, with the writeback fix carrying most of the win. Record the headroom in §6.15; do not relax the gate.

The secondary gate (combined-workflow per-edit-amortized cost stays within ±10% of ~422 µs) **may shift meaningfully** under the corrected semantics, because the within-session cost no longer includes the cumulative-collection penalty. Expect *improvement* on combined_workflow as a free side-effect; if the new median is significantly *better* than the phase-2c baseline, document it as a side-effect rather than a regression.

### A.10 Updated completion-report contents

In addition to the original completion-report format below, the Phase 2D completion report MUST include a section titled "**§N. WritebackResult.invalidated semantic correction**" (or similar) that contains:

1. The spec ambiguity table from §A.2 above (or a link to it).
2. The chosen interpretation: *"`invalidated` contains coordinates that transitioned clean → dirty during this single `write()` call. Excludes coords already dirty before the call."*
3. Why the cumulative reading was wrong: O(|cumulative dirty|) per write × N writes → O(N²) over a bulk load; misleading `invalidated.len` output across all benches; incoherent under I-WB-7.
4. Behavior impact summary: same `WritebackResult` field type; different field contents; `mc-cli` printed dirty count drops from ~19,919 to a small number (matches the brief §8 "bounded" wording); preflight bench output now shows `dirty_set_delta == invalidated.len`.
5. The full bench impact table at 1× / 10× / 50× / 100× scales (`load_canonical_inputs`, `write_input_leaf` / `_10x` / etc., `dirty_propagation`).
6. The A/B isolation result from §A.5 above.
7. Test coverage added per §A.6 (A through D as kernel tests; E as smoke check).
8. Confirmation of the standard validation gate (build / fmt / clippy / test 222 ≥ baseline / demo / bench).

### A.11 SPEC QUESTION triggers (additions on top of the original)

Open a new SPEC QUESTION before continuing if any of these surface:

- An external caller (outside `crates/`) genuinely needs cumulative `invalidated` semantics. *(Audit complete: none found. Internal callers — three benches, two fixture/bench assertions, one CLI line — all behave correctly under marginal semantics. The two `is_empty()` assertions hold trivially because they're on no-deps cubes.)*
- A §10.1 contract test fails under the marginal reading in a way that cannot be explained as "the test was hard-coded to the Phase 1A cumulative bug and its bound number was set against that bug." If the §10.1 bound number changes meaningfully, that's a SPEC QUESTION (the bound is load-bearing per CLAUDE.md §2.6).
- The A/B isolation in §A.5 shows the bitset *regresses* any row meaningfully. (A regression means the bitset isn't pulling its weight and the writeback fix should ship alone — but that's a project-owner decision, not the implementer's.)

---

## Phase 2D prompt (verbatim — this is your contract)

> We are starting MarketingCubes Phase 2D: Bitset-Backed Dirty Tracker.
>
> **Context.** PERF.md §6.14's `load_canonical_inputs` cliff (4.33×/write at 10× → 19.7×/write at 50×) attributes super-linear ingest cost to `AHashSet<CellCoordinate>` rehash + cache locality + hash-collision probability as the dirty set grows from 0 to 1.5M+ entries. Replace the AHashSet with a Cartesian-product flat bitset keyed by linearized coordinate index. Per-mark cost becomes O(1), independent of set size.
>
> **Goal.** Drop `load_canonical_inputs/50x` from 230.84 s (Phase 2C baseline) to ≤ 50 s — a ≥ 4.6× improvement that brings 50× ingest within ~5× of ADR-0003's 10 s patience limit. This is the load-bearing acceptance gate; secondary improvements on `write_input_leaf/10x` and any other write-shaped row are diagnostic, not gating.
>
> **Phase 2D scope:**
>
> 1. **Add a precomputed `CubeShape`** to the `Cube` struct, holding per-dimension element-index maps and per-dimension strides for linearizing a `CellCoordinate` to a `usize` index. Computed once at `CubeBuilder::build` from the existing dim/element data; immutable for the cube's lifetime. Held behind `Arc` for cheap propagation to the dirty tracker. Source: `crates/mc-core/src/cube.rs` (struct field) + a new private helper `cube_shape::CubeShape` (likely a new module `crates/mc-core/src/cube_shape.rs` or a struct inside `cube.rs`).
>
> 2. **Replace the `DirtyTracker` internal representation** in `crates/mc-core/src/dirty.rs`:
>
>    - Before: `pub struct DirtyTracker { set: AHashSet<CellCoordinate> }`
>    - After: `pub struct DirtyTracker { bits: Vec<u64>, shape: Option<Arc<CubeShape>>, len: usize }` (or equivalent — the exact field set is yours, but the **public API surface (every existing pub fn signature) stays identical** and `new()` remains callable without a shape).
>
>    The existing public methods (`new`, `mark`, `mark_closure`, `is_dirty`, `clear`, `clear_all`, `len`, `is_empty`, `iter`, `snapshot_sorted`) keep their signatures **byte-for-byte**. Internal implementations:
>    - `new() -> Self`: **stays available** (back-compat: `lib.rs` re-exports `DirtyTracker`; the existing tests + downstream code that constructs a tracker without a cube still compile). The shape is `None`; the bitset is empty. In this no-shape mode, the implementer may either (a) fall back to the legacy AHashSet representation behind a small enum, or (b) keep the bitset path and lazily initialize the shape on the first `mark` (less recommended). Option (a) is the cleaner pick because it isolates the new fast path to where Phase 2D actually proves a win — the cube's tracker constructed via `with_shape`. Either way, `new()` must continue to work for any test or caller that builds a tracker with no cube context.
>    - `with_shape(shape: Arc<CubeShape>) -> Self`: **additive new constructor.** This is the production path. Allocates `bits = vec![0u64; (cardinality + 63) / 64]`. `CubeBuilder::build` calls this, not `new()`.
>    - `mark(coord)`: linearize coord → set bit at index. Increment `len` if the bit was previously zero. (No-shape mode: insert into the AHashSet fallback.)
>    - `is_dirty(coord)`: linearize coord → test bit at index. O(1). (No-shape mode: AHashSet contains.)
>    - `clear(coord)`: linearize → clear bit. Decrement `len` if the bit was previously one.
>    - `clear_all()`: zero the bitset. `len = 0`.
>    - `iter()`: walk set bits, materializing each as a `CellCoordinate` via the inverse-linearize from `CubeShape`. **Allocation cost is paid only on `iter()`, not on `mark/check`.** Order is bit-set order (deterministic across runs), which is a *stricter* ordering than the current `AHashSet::iter()` (which is non-deterministic). Tests that depended on AHashSet's nondeterminism by sorting first will continue to pass; the stricter ordering is strictly stronger.
>    - `snapshot_sorted()`: walk set bits in order, materialize, return Vec.
>    - `mark_closure(root, graph)`: unchanged — calls into `graph.closure_of_dependents(root)` and feeds each into `mark`. The mark fast-path is what changes.
>
>    **Public API explicitly preserved (no rename, no signature change, no removal):**
>    `pub fn new() -> Self`, `pub fn mark(&mut self, coord: CellCoordinate)`, `pub fn mark_closure(&mut self, root: CellCoordinate, graph: &DependencyGraph)`, `pub fn is_dirty(&self, coord: &CellCoordinate) -> bool`, `pub fn clear(&mut self, coord: &CellCoordinate)`, `pub fn clear_all(&mut self)`, `pub fn len(&self) -> usize`, `pub fn is_empty(&self) -> bool`, `pub fn iter(&self) -> impl Iterator<Item = &CellCoordinate> + '_` (or whatever the current return type is — match it), `pub fn snapshot_sorted(&self) -> Vec<CellCoordinate>`. Plus `Default`, `Debug`. **Internal-only (allowed to change):** the `set` field, any private helper, the bitset width/stride math.
>
>    **Public API explicitly added (new, additive):**
>    `pub fn with_shape(shape: Arc<CubeShape>) -> Self`. Re-export from `lib.rs` only if outside callers need it; otherwise leave it `pub` on the type so `cube.rs` can call it but it doesn't appear in `mc-core`'s public surface. (`Arc` and `CubeShape` likely stay `pub(crate)` if `CubeShape` is internal.)
>
> 3. **Construct the tracker with the shape from `CubeBuilder::build`.** `CubeBuilder::build` computes the `Arc<CubeShape>` from the assembled dimensions, then constructs the cube's tracker via `DirtyTracker::with_shape(shape.clone())`. The `iter()` API needs the shape (held in the tracker's `Option<Arc<CubeShape>>` field) to inverse-linearize, so trackers built via `with_shape` materialize coords correctly; trackers built via `new()` without a shape use the AHashSet fallback path and `iter()` returns coords directly from the set. **Do not** change `CubeBuilder::build`'s public signature.
>
> 4. **Add a kernel unit test** at `crates/mc-core/src/dirty.rs::tests::bitset_tracker_observationally_equivalent_to_ahashset` that builds a small cube, drives a sequence of mark/clear operations against both an old AHashSet-backed tracker (kept inline as a test-only struct, not retained in the kernel) and the new bitset tracker, and asserts they agree on `is_dirty` for every coord, `len()`, and `iter().sorted()`. This is the §10.1 dirty-set membership invariant proven exactly.
>
> 5. **Re-run the Phase 2C bench gate** at `--baseline phase-2c`. The §6.12.7 `load_canonical_inputs` rows + §6.12.1 `write_input_leaf` rows are the diagnostic targets. PERF.md §6.15 records the diff.
>
> **Hard rules** *(see amendment §A above for the WritebackResult.invalidated scope expansion):*
>
> - Source change confined to: `crates/mc-core/src/dirty.rs`, `crates/mc-core/src/cube.rs` (including the writeback path per amendment §A.3), optionally a new file `crates/mc-core/src/cube_shape.rs` (or equivalent — `Arc<CubeShape>` lives somewhere internal). Bench-side label/comment updates per amendment §A.7 also allowed in `crates/mc-core/benches/dirty_propagation.rs`, `hierarchy_mark.rs`, `combined_workflow.rs`. **No other source file may change.**
> - The public API surface in `crates/mc-core/src/lib.rs` MUST NOT lose or rename any re-export. `DirtyTracker`, `CellCoordinate`, `Cube`, `Snapshot` re-exports stay byte-for-byte.
> - The `DirtyTracker` public method signatures stay byte-for-byte. Internal repr changes are fine; signature changes are not.
> - No new external dependency. `bit-vec` is in std-adjacent crates; this phase uses `Vec<u64>` + manual bit-twiddling, NOT a new crate. (If the implementer makes a strong case for `bit-vec` or `bitvec` via SPEC QUESTION + ADR, that's reviewable, but the default is in-house.)
> - No async / threads / rayon / tokio / serde / external storage.
> - All 216 existing tests must still pass. Per amendment §A.6, four new kernel tests (Test A through D) must land in `tests/writeback*.rs`; new total ≥ 220.
> - All Phase 1B / 2A / 2B / 2C benches must still build and run.
> - The §10.1 dirty-set assertions (which check exact dirty-set membership and iter content) MUST pass byte-for-byte.
> - Do not bump `rust-toolchain.toml`. Do not run `cargo update`. Do not touch `docs/specs/`. Do not amend ADR-0003 (Phase 2D consumes it; doesn't modify it).
>
> **Acceptance gate (the one thing that determines done):**
> PERF.md §6.12.7 `demo_path/load_canonical_inputs (126000 writes)` (the 50× row) drops from 230.84 s (phase-2c baseline) to ≤ 50 s. Higher-fan-out write rows (`write_input_leaf/10x`, etc.) should also improve materially; record those numbers in §6.15 but they are not gating.
>
> Secondary expectation: §6.13 combined-workflow per-edit-amortized-over-dirty-delta cost stays flat (within ±10% of ≈ 422 µs at 50×, computed as `edit_time ÷ dirty_delta`; see PERF.md §6.13.2 unit caveat — the bench's `eprintln!` labels the divisor unit "ns" but the result magnitude is µs). This confirms the change doesn't introduce per-edit regression in the saturated-set regime where the AHashSet was already efficient. Note: this metric amortizes total per-edit work over marks-per-edit, so it does not isolate the AHashSet-insert component on its own; the load-bearing acceptance gate is the §6.12.7 ingest cliff above.
>
> **Validation gate before reporting done:**
> Run, in order:
> - `cargo fmt --check --all` (exit 0)
> - `cargo clippy --workspace --all-targets -- -D warnings` (exit 0)
> - `cargo build --release --workspace` (zero warnings)
> - `cargo test --workspace` (must remain 216 / 0)
> - `cargo run --release --bin mc -- demo` (must match brief §4.6)
> - `cargo bench -p mc-core --bench demo_path -- --baseline phase-2c` — confirms the 50× ingest row hits the ≤ 50 s gate
> - `cargo bench -p mc-core --bench combined_workflow -- --baseline phase-2c` — confirms no within-session regression
> - Spot-run other Phase 1B/2A/2B/2C bench files at `--baseline phase-2c`; expect ±10% noise, no row beyond
> - 10 consecutive `cargo test --workspace -q` (still deterministic)
>
> **PERF.md update requirements:**
> - Append §6.15 "Phase 2D verification" with a before/after diff table for every row that improved.
> - Update §9.3 from "data-justified candidate" / "tracked" wording to "**closed in Phase 2D (commit `<hash>`)**". Cite the §6.15 diff.
> - Update §6.14's "Phase 2D pointer" paragraph to a closure note pointing at §6.15. Do not delete the pointer text — leave it as the historical record of what drove the pick.
> - Update §10's files-changed manifest to include the kernel source files Phase 2D touched.
>
> **Rollback plan (in case complexity explodes):**
> If the bitset implementation balloons beyond ~250 lines of source, or if a §10.1 invariant breaks under the new representation in a way that requires more than a one-line fix, **stop and write a SPEC QUESTION per CLAUDE.md §11**. Two recovery paths:
>
> 1. **Roaring Bitmap (Option B).** Add `roaring = "0.10"` as an mc-core runtime dep — requires an ADR documenting the new dep and confirming it doesn't pull in async/serde transitives on Rust 1.78.
> 2. **Hybrid: keep AHashSet, optimize hashing.** Replace the per-coord hash with a precomputed 64-bit hash stored on `CellCoordinate` at construction. Smaller win (probably 2–3×, not the 4–5× the cliff demands), but avoids the bitset complexity.
>
> Either fallback is a Phase 2D.1 amendment, not a Phase 2D scope rewrite.
>
> **Completion report format:**
> ```
> DONE: Phase 2D Bitset-Backed Dirty Tracker
>
> Build:    [command] ✓/✗
> Format:   [command] ✓/✗
> Lint:     [command] ✓/✗
> Tests:    cargo test --workspace 216 / 216
> Demo:     target/release/mc demo ✓/✗
> Bench:    cargo bench -p mc-core --bench demo_path -- --baseline phase-2c ✓/✗
>
> Acceptance gate:
> - load_canonical_inputs/50x: <BEFORE> → <AFTER> (target ≤ 50 s)
>
> Other write-shaped row deltas:
> - write_input_leaf/10x:        <BEFORE> → <AFTER>
> - write_input_leaf/Acme:       <BEFORE> → <AFTER>
> - dirty_propagation/spend:     <BEFORE> → <AFTER>
>
> Within-session combined-workflow check:
> - per-edit ÷ dirty-delta (amortized) iter 1/50/100: <NUMBERS> µs (target: stays in ±10% of ≈ 422 µs; see PERF.md §6.13.2 unit caveat — bench labels divisor "ns" but magnitude is µs)
>
> Source changes:
> - crates/mc-core/src/dirty.rs        (~N lines)
> - crates/mc-core/src/cube.rs         (~N lines)
> - crates/mc-core/src/cube_shape.rs   (new, ~N lines) [optional]
>
> Implementation summary:
> - <one paragraph: bitset width, linearization scheme, allocation pattern>
>
> §10.1 invariant proof:
> - <unit-test name + how it asserts equivalence>
>
> Memory footprint at 100× Acme:
> - <rough number — sanity check the bitset doesn't blow memory>
>
> Deviations:
> - <list any>
> ```
>
> Do NOT commit or tag. The user reviews first.

---

## Context the prompt above does NOT spell out

These are landmarks the receiving instance will need.

### A. The exact code being optimized

[`crates/mc-core/src/dirty.rs`](../../crates/mc-core/src/dirty.rs) currently holds:

```rust
pub struct DirtyTracker {
    set: AHashSet<CellCoordinate>,
}
```

And exposes the public methods `new`, `mark`, `mark_closure`, `is_dirty`, `clear`, `clear_all`, `len`, `is_empty`, `iter`, `snapshot_sorted`. Every mark is a `CellCoordinate.hash()` (which walks the SmallVec<[ElementId; 6]>, hashing 6 u64s) + an AHashSet insert (which does open-addressed probing + occasional rehash as the table grows past load-factor thresholds). The decisive measurement is the cross-scale ingest cliff in PERF.md §6.12.7: `load_canonical_inputs` jumps from 4.33×/write at 10× to 19.7×/write at 50×, with the dirty set saturating at **305 K entries (measured)** at 50× and projecting larger at 100× (run abandoned mid-criterion-estimation at > 38 minutes). The within-session per-edit-amortized number (≈ 422 µs at 50×, computed as `edit_time ÷ dirty_delta`; see PERF.md §6.13.2 unit caveat) is *flat* once the dirty set is saturated — that flatness is consistent with §9.3 but does not isolate the AHashSet-insert cost on its own; the cliff in §6.12.7 is what attributes cost to set growth.

### B. Why a Cartesian-product flat bitset is the right shape

Linearize `CellCoordinate { cube, elements: [e0, e1, ..., e_{D-1}] }` to a `usize index` via:

```rust
fn linearize(coord: &CellCoordinate, shape: &CubeShape) -> usize {
    let mut idx = 0;
    for (dim, eid) in coord.elements.iter().enumerate() {
        let local = shape.element_index_in_dim[dim][eid];   // u32 lookup
        idx += local as usize * shape.stride[dim];
    }
    idx
}
```

`shape.element_index_in_dim[dim]` is a `AHashMap<ElementId, u32>` precomputed at cube-build time (one lookup per dim, no allocation). `shape.stride[dim]` is precomputed (per-dim stride for the row-major linearization). Total cost: D = 6 hash lookups + 6 multiplies + 6 adds — ~50 ns on Acme hardware. **Independent of set size.** That's the win.

**Indexing domain.** The bitset is keyed by **every Cartesian-product coordinate** in the cube's cross-product of dim-element sets — *all* coords, not just populated/storable. A coord is one bit regardless of whether the store ever holds a value at it. This is what makes mark/check O(1): linearize-then-test, with no set-membership lookup.

**Cube cardinality** = product of `|dim_i|` where `|dim_i|` includes both leaves and consolidations (per-dim element count, since dirty marks can target any element including consolidated nodes via `mark_closure`). Acme dims:

- Scenario: 3 (Baseline, Aggressive, Conservative)
- Version: 3 (Working, Submitted, Approved)
- Time: 17 (12 monthly leaves + Q1–Q4 + FY)
- Channel: 8 (5 leaves + 2 channel groups + All_Channels)
- Market: **15 at Acme; widens with `scale`** — `(7 × scale) + 5 + 2 + 1` per `mc-fixtures::build_scaled_market_dim` (cities + states + regions + USA)
- Measure: 11 (6 input + 5 derived)

So Cartesian cardinality = `3 × 3 × 17 × 8 × Market × 11 = 13,464 × |Market|`.

| Scale | `|Market|` | Cartesian coords | Bitset bytes (`⌈coords / 8⌉`) | Notes |
|---|---:|---:|---:|---|
| Acme (1×) | 15 | 201,960 | ~25 KB | comfortable; an L1-cache-line-friendly working set |
| 10× | 78 | 1,050,192 | ~128 KB | comfortable |
| 50× | 358 | 4,820,112 | ~588 KB | comfortable; well under any per-process memory bound |
| 100× | 708 | 9,532,512 | ~1.16 MB | comfortable; equivalent to a single mid-sized struct allocation |

**Cardinality-explosion guard.** Compute `cardinality` at `CubeBuilder::build`. If it exceeds a generous safety threshold (suggested: `1 << 30` ≈ 1 G coords ≈ 128 MB bitset), **fall back** to the no-shape AHashSet representation rather than allocating the flat bitset. Phase 2D's calibration scales (≤ 100×) are nowhere near that threshold; the guard is a forward-compat bound for hypothetical larger cubes (e.g., a real-production cube with 100M+ Cartesian coords). If the implementer's measured cardinality at 100× exceeds 100 M, surface a SPEC QUESTION before proceeding — that's a signal that flat-bitset is the wrong shape and Roaring Bitmap (Option B in the rollback plan) should be considered first.

**Why not a sparse-by-default hash-keyed bitmap (e.g., Roaring) at this scale?** At ≤ 1 M coords, the flat bitset fits in O(100 KB), which is small enough that the dense representation's O(1) bit-test wins outright. At 10 M+ coords, the equation changes (Roaring's compressed run-length representation starts to win when the actual mark density is < ~5%). Phase 2D anchors on the calibration scales (≤ 9.5 M coords at 100×) where flat is the right pick; if a future Phase 2D.1 needs to address a 100 M+ scale it should re-evaluate via the rollback plan.

### C. The §10.1 dirty-set membership invariant

`crates/mc-core/tests/acme_demo.rs::t_acme_dirty_set_size_within_bound_after_one_spend_write` asserts the dirty-set delta after a single Spend write is bounded (≤ 215 marks at Acme). The bitset implementation must reproduce this exact membership — every coord that the AHashSet implementation marked as dirty after that write must be marked in the bitset, and no others. This is a strictly stronger guarantee than "len matches" because a bitset that marks the right *count* of cells but the wrong *cells* would still pass len assertions.

The kernel unit test at §"Phase 2D scope" item 4 is what proves this: build both representations side-by-side, drive identical operation sequences, assert byte-for-byte equivalence on the resulting set membership. If that test passes, every higher-level test (§10.1 etc.) inherits the equivalence.

### D. iter() ordering — strictly stronger, not weaker

The current `AHashSet::iter()` is non-deterministic across runs (it's a hash table). Any test that compares iter content for equality already collects + sorts first (per CLAUDE.md §2.11). The bitset's `iter()` is deterministic in bit-position order — strictly stronger. **Tests that worked under AHashSet keep working under bitset.** No test should need modification for ordering.

### E. The shape's lifetime relative to the cube

`CubeShape` is computed at `CubeBuilder::build` and held by `Cube` for its lifetime. `Snapshot` (which clones `HashMapStore`) does **not** need to clone the shape — the shape is structural metadata about the cube's dimensions, not part of the store. When `Cube::rollback_to(&snap)` runs, the shape is unchanged. `DirtyTracker::clear_all()` zeroes bits but keeps the shape. **No interaction with snapshot semantics.**

### F. Phase 2C regression guard

The phase-2c baseline at `docs/reports/bench-data/phase-2c/` is your forward baseline. After your change:

- `load_canonical_inputs` rows should improve dramatically (this is the gate).
- `write_input_leaf` rows should improve modestly (per-write fixed cost includes the AHashSet insert).
- `dirty_propagation` rows should improve (mark_closure walks more efficiently).
- Read rows (`read_input_leaf_warm/cold`, `read_derived_leaf_*`, `consolidation_warm/cold`) should be **unchanged or marginally faster** (read paths don't touch the dirty tracker write surface, but the `is_dirty` check is now O(1) bit-test instead of hash-and-probe).
- Snapshot/rollback rows should be unchanged.
- Within-session per-edit-amortized cost (combined_workflow §6.13.3, computed as `edit_time ÷ dirty_delta`; see PERF.md §6.13.2 unit caveat — the bench labels the divisor "ns" but the magnitude is µs at 50×) should stay flat — no regression in the saturated regime where AHashSet was already cheap.

Any **regression** beyond noise is a stop-the-line signal — investigate before recording.

### G. Phase 2E forecast (after Phase 2D ships)

If 2D succeeds:
- §9.3 closes; the §9 list narrows to §9.2 (opportunistic), §9.6 (leave it), §9.7 (toolchain).
- The next §6.14-driven candidate becomes whichever row Phase 2C's 50× / 100× env-gated benches surface when Phase 2D opts into them.
- **Phase 2E may not need to exist.** If the §6.14 cliff is the only super-linear row and Phase 2D closes it, Phase 2 can exit and Phase 3A becomes proposed.

If 2D's bitset hits a memory wall at 100× or beyond:
- Phase 2D.1 amendment: switch to Roaring Bitmap (under ADR), or switch to a hybrid that uses bitset for hot dim ranges and falls back to AHashSet for sparse coordinates.

---

## Pointers to existing files you will most likely touch

| Why | File | Action |
|---|---|---|
| The optimization site | [`crates/mc-core/src/dirty.rs`](../../crates/mc-core/src/dirty.rs) | replace internal repr; preserve public method signatures |
| Hold the precomputed shape + correct writeback semantics | [`crates/mc-core/src/cube.rs`](../../crates/mc-core/src/cube.rs) | add `cube_shape: Arc<CubeShape>` field; populate in `CubeBuilder::build`; **plus per amendment §A: rewrite the `WritebackResult.invalidated` construction in `Cube::write` to capture clean→dirty transitions only, not the cumulative dirty set** |
| (optional) Define `CubeShape` | `crates/mc-core/src/cube_shape.rs` | new file (private module); or inline in `cube.rs` |
| (optional) Linearize helper | [`crates/mc-core/src/coordinate.rs`](../../crates/mc-core/src/coordinate.rs) | add `pub(crate) fn linearize(&self, shape: &CubeShape) -> usize` if it reads cleaner there |
| Bench preflight wording (per amendment §A.7) | [`crates/mc-core/benches/dirty_propagation.rs`](../../crates/mc-core/benches/dirty_propagation.rs), [`hierarchy_mark.rs`](../../crates/mc-core/benches/hierarchy_mark.rs), [`combined_workflow.rs`](../../crates/mc-core/benches/combined_workflow.rs) | update `eprintln!` strings + comments so `dirty_set_delta` and `invalidated.len` are clearly two different quantities (under corrected semantics they will agree on clean cubes); rename `final_invalidated_len` → `last_write_invalidated_len` in combined_workflow |
| Required new tests (per amendment §A.6) | `crates/mc-core/tests/writeback.rs` (or new `tests/writeback_invalidated.rs`) | add Tests A through D (kernel-level marginal-semantics assertions) |
| Phase 2D verification subsection + §9.3 closure note + §10 manifest + amendment §A.8 historical-rationalization annotations | [`../PERF.md`](../PERF.md) | append §6.15 + targeted edits at §6.4 / §6.13 / §6.14 |
| Phase 2D completion report | `docs/reports/phase-2d-completion-report.md` | new file (use [`../templates/phase-completion-report.md`](../templates/phase-completion-report.md)) **plus the dedicated "WritebackResult.invalidated semantic correction" section per amendment §A.10** |
| Save phase-2d criterion baseline | [`../reports/bench-data/phase-2d/`](../reports/bench-data/) | new dir; use the [bench-data README](../reports/bench-data/README.md) workflow |
| Status flips | [`../CURRENT_STATE.md`](../CURRENT_STATE.md), [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) | flip Phase 2D from `proposed` → `complete` |

**Do not touch:**

- `crates/mc-core/src/` — any file other than `dirty.rs`, `cube.rs`, optionally `cube_shape.rs` and `coordinate.rs`. Touching `consolidation.rs`, `rule.rs`, `dependency.rs`, etc. is a signal that the scope has crept.
- `crates/mc-core/tests/` — the contract test suite is locked. **Exception per amendment §A.6:** *adding* new tests A through D in `writeback.rs` (or a new sibling file) is required and explicitly authorized; **modifying** existing tests is still forbidden unless one was hard-coded to the Phase 1A cumulative bug (in which case open a SPEC QUESTION first).
- `crates/mc-core/benches/` — Phase 2D does not need new bench code; the existing files run against `--baseline phase-2c`. **Exception per amendment §A.7:** label/comment updates in three bench files are explicitly authorized; no behavior change.
- `crates/mc-fixtures/src/lib.rs` — public fixtures are a shared contract. The two `result.invalidated.is_empty()` assertions hold trivially under both readings (no-deps cubes); leave them alone.
- `docs/specs/` — locked. **The amendment in §A above lives in this handoff and the Phase 2D completion report; the spec docs themselves are not edited.** The interpretation that wins is documented; the spec wording stays as-is for audit trail.
- `rust-toolchain.toml` — pinned.
- ADR-0003 — Accepted; amendments go in `0003-amendment-N.md`.

---

## Reproducible commands you can rely on

```bash
cd /Users/edwinlovettiii/Projects/mc-v2

# (only if your shell didn't initialize rustup)
source $HOME/.cargo/env

# Pre-2D gate — must remain green throughout
cargo build --release --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                  # 216 / 0
cargo run --release --bin mc -- demo

# Restore phase-2c baseline locally so --baseline phase-2c works:
for bench in $(ls docs/reports/bench-data/phase-2c/); do
  [ "$bench" = "README.md" ] && continue
  mkdir -p "crates/mc-core/target/criterion/$bench"
  cp -R "docs/reports/bench-data/phase-2c/$bench/." "crates/mc-core/target/criterion/$bench/"
done

# Pre-2D bench check (sanity — every row should match phase-2c):
cargo bench -p mc-core -- --baseline phase-2c

# Quick smoke during 2D development:
cargo bench -p mc-core --bench dirty_propagation -- \
  --warm-up-time 1 --measurement-time 1 --sample-size 10
cargo bench -p mc-core --bench leaf_read_write -- \
  --warm-up-time 1 --measurement-time 1 --sample-size 10

# The acceptance-gate row — full sample:
cargo bench -p mc-core --bench demo_path -- --baseline phase-2c

# Save the post-2D baseline once at end of phase:
cargo bench -p mc-core --bench <name> -- --save-baseline phase-2d
mkdir -p docs/reports/bench-data/phase-2d
# then mirror the bench-data/README.md workflow to copy phase-2d JSON
```

---

## Final checklist before you call Phase 2D done

- [ ] Cartesian-product flat bitset implemented in `dirty.rs` (Option A from §B above).
- [ ] `CubeShape` struct added (in `cube.rs` or `cube_shape.rs`) and populated at `CubeBuilder::build`.
- [ ] Linearize / inverse-linearize implementations + tests for round-tripping coords.
- [ ] **`WritebackResult.invalidated` semantic correction shipped per amendment §A.3** — clean→dirty transitions only.
- [ ] **Bench preflight wording updated per amendment §A.7** in `dirty_propagation.rs`, `hierarchy_mark.rs`, `combined_workflow.rs`.
- [ ] Source change confined to `dirty.rs`, `cube.rs` (incl. writeback path), optionally `cube_shape.rs` + `coordinate.rs`, plus the three benches above for label/comment-only updates.
- [ ] No public API symbol from `crates/mc-core/src/lib.rs` removed or renamed. `WritebackResult` field types unchanged; only `.invalidated` *contents* changed.
- [ ] No new external dependency. (If a SPEC QUESTION authorized one, link the ADR.)
- [ ] Kernel unit test `bitset_tracker_observationally_equivalent_to_ahashset` lands in `dirty.rs::tests` and passes.
- [ ] **Tests A through D from amendment §A.6** land in `tests/writeback*.rs` (or equivalent) and pass.
- [ ] All 216 existing tests still pass; new total ≥ 220.
- [ ] **A/B isolation per amendment §A.5 captured** — bitset-only configuration measured at 10× and 50×, results reported in completion report's "Source attribution" section.
- [ ] §10.1 `t_acme_dirty_set_size_within_bound_after_one_spend_write` passes byte-for-byte (this is the load-bearing membership invariant on `cube.dirty`, not on `WritebackResult.invalidated`; under the corrected semantics those quantities now agree for the test's clean-cube setup, so the assertion holds either way).
- [ ] 10 consecutive `cargo test --workspace -q` runs identical.
- [ ] `cargo run --release --bin mc -- demo` still matches §4.6 *structure*. The "{N} dependent cells dirtied" line will print a small N (matches "bounded per brief §8") instead of the cumulative ~17,820+ figure under Phase 1A; this is correct, not a regression.
- [ ] **Acceptance gate met:** `load_canonical_inputs/50x` ≤ 50 s.
- [ ] Within-session combined-workflow per-edit-amortized cost flat (within ±10% of ≈ 422 µs at 50× *or better* — improvements are expected as a free side-effect of removing the cumulative-collection penalty; see PERF.md §6.13.2 unit caveat — bench labels divisor "ns" but result magnitude is µs).
- [ ] No Phase 1B / 2A / 2B / 2C bench row regressed beyond noise (~10%).
- [ ] PERF.md §6.15 written; §9.3 closure-noted; §6.14 pointer paragraph updated to closure note; §10 manifest updated. Per amendment §A.8, the §6.4 / §6.13 / §6.14 historical-rationalization sites annotated as Phase 1A bug artifacts.
- [ ] Completion report at `docs/reports/phase-2d-completion-report.md` written from template **plus the amendment §A.10 dedicated section** ("WritebackResult.invalidated semantic correction").
- [ ] CURRENT_STATE.md and MASTER_PHASE_PLAN.md updated to flip Phase 2D from `proposed` → `complete`.
- [ ] **You did NOT commit, tag, or push.** The user does that after reading the review.
- [ ] **You did NOT start Phase 2E or any later phase.**

If you are uncertain at any point, the resolution order is:

1. The Phase 2D prompt above.
2. PERF.md §6.14 (the data justifying this phase) + §6.12.7 (the gate row).
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

**Measure before you optimize.** Phase 2D exists because Phase 2C measured what 2D should optimize. The §6.14 cliff data is the justification. Your kernel change must trace back to a §6.14 / §6.12.7 row.

**Source-locked between phases — except this one.** This is the rare phase that touches the kernel, surgically, with a unit test. Phase 2E (if any) is back to source-locked.

**A bench is a contract, not a draft.** Phase 2D's verification rows must be reproducible by anyone with the repo. Use the `phase-2c` baseline; save the `phase-2d` baseline; commit the JSON.

**Do not pick the next optimization.** Phase 2D's deliverable is the source change + its verification. If §9 still has rows after this phase, the next sub-phase is its own pick — driven by the post-2D bench data, not by the §9 list.

**Rollback is a real path.** If the bitset implementation outgrows ~250 lines or the §10.1 invariant breaks in a non-trivial way, **stop and write a SPEC QUESTION per CLAUDE.md §11.** The two recovery paths (Roaring Bitmap, hashed-CellCoordinate) are explicit. Don't ship a kernel change that broke a contract test "with one tiny tweak."

---

*Phase 2D handoff promoted from scaffold (`docs/reports/phase-2d-handoff-scaffold.md`) on 2026-05-02 after Phase 2C committed at `789db15` (tag `phase-2c-workload-baseline`). Branch A picked from PERF.md §6.14's `load_canonical_inputs` cliff finding.*
