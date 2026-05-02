# ADR-0002: Performance assertions belong in criterion benchmarks, not in `cargo test`

**Status:** Accepted
**Date:** 2026-05-01
**Deciders:** Project owner + implementing instance
**Phase:** 2B

---

## Context

Phase 2B's job was a single targeted kernel optimization: eliminate the
per-call hierarchy/dimension clone in
[`crates/mc-core/src/cube.rs::read_consolidated`](../../crates/mc-core/src/cube.rs)
so the brief §11.2 3-leaf 1B target (≤ 3 µs cold) is met. The
implementation succeeded — the §6.7 3-leaf cold row drops from 14.3 µs
to ~2.7 µs (PERF.md §6.7 + §6.11) — but in doing so it broke a Phase
1A-era contract test that *measured* the cold/warm read ratio with
`std::time::Instant::elapsed()` inside `cargo test`:

> [`crates/mc-core/tests/consolidation.rs::t_consolidation_caches_value_within_revision`](../../crates/mc-core/tests/consolidation.rs)
> (per [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md)
> §10.3, lines 2258–2260)
>
> > Read consolidated Q1 Spend; record duration. Read again immediately;
> > assert second read is at least 10x faster (cache hit).

The test implemented that wording as a single-shot ratio:
`assert!(d2_ns * 10 <= d1_ns)`. Pre-2B that was satisfied with ~43×
headroom in debug mode (d1 ≈ 60 µs vs d2 ≈ 1.4 µs); post-2B it falls to
~9× under `cargo test --workspace` parallel-test load (d1 ≈ 12.5 µs vs
d2 ≈ 1.4 µs) and flakes about 50% of the time. The release-mode bench
equivalent of the same operation still shows ~43× (2.7 µs vs 63 ns) —
the failure is a debug-mode + workspace-parallel-load + timer-noise
artifact, not a regression in caching behavior.

This is a structural problem with where the assertion lives, not with
the operation it was trying to verify.

## Decision

**Wall-clock micro-performance assertions belong in criterion benchmarks
(`cargo bench`), not in `cargo test`.** `cargo test` is a correctness
gate; criterion is a performance gate. Mixing them produces flaky tests
when an optimization succeeds.

The rule, in three lines:

1. **`cargo test`** asserts *what* the kernel does — values, types,
   provenance, dependency edges, dirty-set membership, error variants,
   revision monotonicity, snapshot/rollback identity, lock-conflict
   outcomes. Anything observable through the public API as a value, a
   structural shape, or a control-flow outcome.
2. **`cargo bench`** asserts *how fast* the kernel does it — wall-clock
   cost of an operation, with statistical bounds (criterion's
   sample-of-100 + outlier handling), against documented ceilings in
   `docs/PERF.md` per brief §11.
3. **A failing performance bench is informational at first** (PERF.md
   §6 / §11.2 deltas). It only becomes a ship-blocker when the relevant
   acceptance gate explicitly cites a numerical ceiling that has been
   missed (e.g. brief §12 acceptance criterion 5 + PERF.md §6.7
   3-leaf row's 1B target).

Concretely for the §10.3 case: the brief comment "10x faster (cache
hit)" is preserved as the *intent* — "the cache hit happened" — but the
test now asserts that intent semantically (cache entry has
Consolidation provenance + matching revision; second read returns
byte-for-byte identical value; revision unchanged across reads;
post-write invalidation reaches the consolidated coord; recompute
reflects the new leaf). The "X× faster" claim is recorded in PERF.md
§6.3 (warm reads ≈ 63 ns) and §6.7 (cold reads ≈ 2.7 µs), where the
~43× speedup is statistically established over 100 samples per row
rather than measured once with `Instant::elapsed()`.

## Consequences

**Positive:**

- `cargo test` flakes that come from "the kernel got faster than the
  test thought possible" become impossible by construction. Future
  optimizations cannot wedge themselves against a wall-clock test
  bound that was sized against an earlier (slower) baseline.
- Performance evidence concentrates in one place (PERF.md), with
  consistent methodology (criterion + sample-of-100 + outlier handling
  + recorded median + range), instead of being scattered across
  ad-hoc `Instant::elapsed()` calls in test code.
- The semantic assertions (provenance, dirty-set membership, revision
  invariants) actually verify the *invariants* the cache is supposed
  to uphold, not the timing-shaped shadow of those invariants.

**Negative / accepted trade-offs:**

- A test rewrite is a one-time deviation from the
  "test names + assertions in §10 are character-exact contracts"
  rule (CLAUDE.md §4.1). This ADR is the deviation's audit trail.
  The test name, file location, and stated intent are unchanged; only
  the assertion mechanism moved. Future similar deviations require
  the same surface-and-approve protocol per CLAUDE.md §11.
- `cargo bench` is slower to run than `cargo test` and is not in the
  default developer inner loop. A regression introduced by an
  optimization that doesn't yet have a corresponding bench will go
  un-caught until the next bench gate run. **Mitigation:** the Phase
  1B / Phase 2A bench suites already cover every §11 row. New
  optimizations that touch a benched code path must be benched as
  part of the optimization; PERF.md is the audit trail.

**Reversal cost:**

Cheap. The semantic assertions added in the §10.3 rewrite are pure
additions — they don't preclude *also* asserting timing in a future
context that has a more robust harness (e.g. release-mode-only,
multi-iteration averaging, fixed CPU pinning). If a future phase wants
to put a wall-clock guard back into `cargo test`, it can — but the
default is now: **don't**.

## Alternatives considered

1. **Tighten the cold path's cache fast path so d2 shrinks too.**
   Rejected. Tightening d2 makes the d2/d1 ratio *worse*, not better,
   so it doesn't help the test. It also drags the optimization into a
   second code path with no PERF.md justification.

2. **Add an artificial floor to d1.** Rejected immediately. Slowing
   the kernel down to make a test pass is the worst possible failure
   mode for a performance phase.

3. **Loosen the test's constant from 10× to 5× (or some other number).**
   Rejected. That is exactly the test-fudging CLAUDE.md §2.6 forbids,
   and it leaves the same brittleness pattern in place — Phase 2C
   (or any future cache-path optimization) would re-trigger the
   problem at a different threshold.

4. **Restructure the test to average over N reads.** Considered. This
   would have helped the immediate flake but kept the wrong methodology
   in the wrong place: a test binary running un-optimized code in
   parallel under variable load is the wrong place to measure
   sub-microsecond operations. Criterion exists for exactly this
   problem.

5. **Reject Phase 2B and keep the deep clones.** Rejected. The brief
   §11.2 3-leaf 1B target (≤ 3 µs) is a documented acceptance gate
   that Phase 2B was specifically chartered to close. Trading a real
   acceptance-gate miss for a test-shape preference would invert the
   importance of the two.

## Cross-links

- The brief test wording this ADR reinterprets:
  [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md)
  §10.3 lines 2258–2260.
- The rewritten test:
  [`../../crates/mc-core/tests/consolidation.rs`](../../crates/mc-core/tests/consolidation.rs)
  `t_consolidation_caches_value_within_revision`.
- The performance evidence the test no longer measures itself:
  [`../PERF.md`](../PERF.md) §6.3 (warm) + §6.7 (cold) + §6.11 (Phase
  2B before/after).
- The Phase 2B kernel change that triggered this ADR:
  [`../../crates/mc-core/src/cube.rs`](../../crates/mc-core/src/cube.rs)
  `Cube::read_consolidated`,
  [`../../crates/mc-core/src/dimension.rs`](../../crates/mc-core/src/dimension.rs)
  `Dimension::hierarchies` (Arc-wrapped).
- The Phase 2B handoff that scoped this work:
  [`../handoffs/phase-2b-handoff.md`](../handoffs/phase-2b-handoff.md).
- The Phase 2B completion report that documents the rewrite under
  "Source changes" + "Deviations":
  [`../reports/phase-2b-completion-report.md`](../reports/phase-2b-completion-report.md).
- The operating manual rules this ADR creates an authorized exception to:
  [`../../CLAUDE.md`](../../CLAUDE.md) §2.6, §4.1, §11.
- ADR predecessor establishing the contract-test discipline:
  [`0001-phase-1-scope.md`](0001-phase-1-scope.md).

## Notes

The "stop and surface via SPEC QUESTION" path defined in CLAUDE.md §11
is what produced this ADR. The implementing instance hit the conflict,
declined to bend either side silently, posted the question with full
context, and the project owner approved the rewrite. **That round-trip
is the operating manual working as designed.** A future maintainer who
hits a similar conflict between a hard rule and a deliverable should
follow the same path: surface the conflict, propose paths forward,
wait for direction, and document the decision in an ADR.
