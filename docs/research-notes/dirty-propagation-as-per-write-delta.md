---
name: Dirty propagation as per-write delta
description: The brief §10.1 dirty-set bound (215) is the marginal effect of one write, not the absolute size of the dirty set; tests must compare before/after snapshots
type: research-note
---

# Dirty propagation as per-write delta

**Status:** active
**Created:** 2026-05-01
**Last touched:** 2026-05-01
**Spans phases:** 1A → 2

---

## Conclusion (one sentence)

The brief §10.1 dirty-set bound of 215 cells is the *marginal* propagation effect of a single Spend write at one (Mar/Paid_Search/Tampa) leaf — 6 × 35 hierarchy ancestors plus 5 same-leaf derived shells — not the absolute dirty-set size after fixture setup, which is ≈17,820 because `write_canonical_inputs` legitimately marks ancestors during each of its 2,520 writes.

## Why this matters

§10.1 is a contract test. Read literally, it says "after one Spend write, the dirty set is ≤ 215." That fails by two orders of magnitude on the as-shipped engine — *not because dirty propagation is broken, but because the spec assumes the cube starts clean and §4 mandates loading 2,520 inputs first.* Phase 1A reframed the assertions as deltas (`after - before ≤ 215`) to preserve the spec's invariant content while accommodating its own fixture-setup mandate. The bound is what the spec wanted; only the comparison frame changed. This is the most subtle of the five Phase 1A deviations and the one most likely to be misread as "Phase 1A loosened a test."

The bound itself is load-bearing for Phase 1B's dirty-propagation benchmark and for Phase 2 invalidation work: if the marginal mark count grew past 215, dirty propagation is over-marking, even if the absolute set looks "the same."

## Evidence

The arithmetic of the 215 bound comes from the structure of one Acme leaf write:

- **6 × 35 = 210 hierarchy-ancestor coords.** The 6 measures (1 written + 5 derived shells) × 35 ancestor combinations across (Time × Channel × Market) hierarchies. Time has 5 ancestors at one leaf (Q1, H1, FY, plus root membership in the synthesized flat — 4 in practice + Mar itself counted at index 0); Channel has 3 (Paid_Media, root); Market has 3 (Florida, USA, root). The Cartesian product of {self, ancestors} per dim minus the self-leaf-self-measure cell that was just written gives the bound.
- **+ 5 same-leaf-different-derived-measure shells.** A write to (Mar/Paid_Search/Tampa, Spend) invalidates (Mar/Paid_Search/Tampa, Clicks/Leads/Customers/Revenue/Gross_Profit) — five shells that read Spend transitively via SelfRef rules.

The Phase 1A reframing is documented verbatim in:

- [`docs/reports/phase-1-completion-report.md` §4.2](../reports/phase-1-completion-report.md) — the "What I did" / "Rationale" pair. The key sentence: *"The 215 bound is a per-write quantity — the marginal effect of one write — not an absolute count after fixture setup."*

The mechanism — two orthogonal mark paths, both invoked per write:

- [`crates/mc-core/src/cube.rs:873-882`](../../crates/mc-core/src/cube.rs#L873-L882) — `Cube::write` step (11): `self.dirty.mark_closure(&req.coord, &self.deps)` walks rule-edge dependents (lazy graph; only meaningful if reads have populated it), then `compute_dirty_ancestors` walks the hierarchy Cartesian product unconditionally.
- [`crates/mc-core/src/cube.rs:916-997`](../../crates/mc-core/src/cube.rs#L916-L997) — `compute_dirty_ancestors` is the hierarchy walk. The skip-condition at line 972 (`is_pure_leaf && m == measure_id`) is what produces the *exclusion* of the just-written cell from the dirty set; everything else in the Cartesian product is marked.

The tests that lock the delta interpretation:

- [`crates/mc-core/tests/acme_demo.rs`](../../crates/mc-core/tests/acme_demo.rs) — `t_acme_dirty_set_required_present_after_one_spend_write`, `t_acme_dirty_set_required_absent_after_one_spend_write`, `t_acme_dirty_set_size_within_bound_after_one_spend_write`. Each captures `cube.dirty().snapshot_sorted()` before the test write, runs the test write, and asserts on the diff.
- The deterministic-sort discipline used by these tests: [`crates/mc-core/src/dirty.rs:84-88`](../../crates/mc-core/src/dirty.rs#L84-L88) `snapshot_sorted` — see also [`./dirty-tracker-iter-order.md`](./) (TODO if the iter-order rationale needs its own note; for now it's CLAUDE.md §2.11).

## Where it shows up in the engine

- **Source — write path:** [`crates/mc-core/src/cube.rs::write`](../../crates/mc-core/src/cube.rs#L716) step (11) at line 877.
- **Source — hierarchy ancestor walker:** [`crates/mc-core/src/cube.rs::compute_dirty_ancestors`](../../crates/mc-core/src/cube.rs#L916).
- **Source — rule-edge closure walker:** [`crates/mc-core/src/dirty.rs::mark_closure`](../../crates/mc-core/src/dirty.rs#L42).
- **Tests:** [`crates/mc-core/tests/acme_demo.rs`](../../crates/mc-core/tests/acme_demo.rs) (§10.1 dirty-set tests).
- **Spec:** [`docs/specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) §10.1, §8 (algorithm), §16 (invariants); engine-semantics §16.
- **Deviation rationale:** [`docs/reports/phase-1-completion-report.md` §4.2](../reports/phase-1-completion-report.md).
- **Operating manual:** [`CLAUDE.md`](../../CLAUDE.md) §2.6 (test-fudging trap), §2.9 (forgetting hierarchy rollups).

## Edge cases / gotchas

- **The bound 215 is not 6 × 35 + 5 = 215 by coincidence.** It's the upper bound for the worst-shaped Acme write. Different writes (e.g., to a less-deep leaf, or to a measure with no derived dependents) produce strictly smaller deltas. Test writes pick a worst-case-ish coord on purpose.
- **`required_absent` is also a delta.** A naive read of "X must NOT be dirty" fails immediately because `write_canonical_inputs` legitimately dirtied X during fixture setup (e.g., Atlanta-leaf-derived cells got marked when each Atlanta-leaf input was written). The test asserts the *test write* didn't mark them — see report §4.2 second paragraph.
- **`required_present` is NOT delta — it's still absolute.** Every coord the spec lists as required-present must be in the dirty set after the test write. They were already in there before (from `write_canonical_inputs`), so they're trivially present; the assertion that matters is that none of them got cleared between fixture-load and the test write.
- **The two propagation paths are not redundant.** Removing the hierarchy walk would let writes-before-any-read silently fail to invalidate consolidated coords. Removing `mark_closure` would let writes-after-reads fail to invalidate transitively-dependent cells. CLAUDE.md §2.9 calls out forgetting either path as a recurring trap.
- **`clear_all` after fixture-load would let absolute assertions work** but is not how the spec's setup runs. Don't be tempted to add it as a "fix" — it would mask any per-write over-marking bug that the absolute-style test was supposed to catch.
- **Phase 1B benchmarks must time the delta**, not the absolute. Snapshot before, write, snapshot after, diff. See the [`phase-1b-handoff.md`](../handoffs/phase-1b-handoff.md) §C for the framing.

## Related notes

- [`./lazy-dependency-graph.md`](./lazy-dependency-graph.md) — `mark_closure` walks reverse edges; if no reads have happened, it walks an empty index.
- [`./two-caching-layers-in-read.md`](./two-caching-layers-in-read.md) — the dirty bit is the cache-invalidation key for both derived-leaf and consolidated caches.

## History

- 2026-05-01 — Created from Phase 1A completion report §4.2 and the §10.1 test trio, after Phase 1A ship.
