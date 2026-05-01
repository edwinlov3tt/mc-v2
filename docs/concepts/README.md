# concepts/

Cross-cutting engine concepts that span phases. **The knowledge layer that transfers from Phase N to Phase N+1.**

Each concept is a self-contained file explaining one idea: what it is, why we chose it over alternatives, where it shows up in the code, and what to watch out for.

## Status

Empty as of Phase 1A ship. Future Phase 1B / Phase 2 work should add files here when a non-obvious decision needs to be preserved.

## Candidate topics waiting to be written

These are concepts the Phase 1A code embodies but doesn't yet have a written document for. Add them as they become relevant in a session:

- **Lazy dependency graph** — why the dep graph is built on first read rather than at construction time. Source: brief §3.12, [`../../crates/mc-core/src/dependency.rs`](../../crates/mc-core/src/dependency.rs). CLAUDE.md §2.1 traps.
- **Dirty propagation as a per-write delta** — why §10.1 dirty-set assertions are framed as deltas, not absolutes. Source: [`../reports/phase-1-completion-report.md`](../reports/phase-1-completion-report.md) §4.2.
- **Null vs zero vs NaN** — the spec §7 rules, where they're enforced, and what bugs they prevent. Source: brief §7, CLAUDE.md §2.5, [`../../crates/mc-core/src/value.rs`](../../crates/mc-core/src/value.rs), [`../../crates/mc-core/src/rule.rs`](../../crates/mc-core/src/rule.rs).
- **Weighted-average consolidation** — why CPC, CVR, etc. don't simple-sum. Source: brief §3.17, CLAUDE.md §2.10, [`../../crates/mc-core/src/consolidation.rs`](../../crates/mc-core/src/consolidation.rs).
- **Two caching layers in `read`** — derived-leaf cache + consolidated cache, the dirty bit as the cache invalidator, and the trace-bypass-cache rule. Source: [`../../crates/mc-core/src/cube.rs`](../../crates/mc-core/src/cube.rs) `read_derived_leaf` and `read_consolidated`.
- **Snapshot as deep-clone** — why Phase 1 snapshots are clones rather than COW. Source: brief §3.16, CLAUDE.md §2.18, [`../../crates/mc-core/src/snapshot.rs`](../../crates/mc-core/src/snapshot.rs).

## How to write a concept file

1. Copy [`../templates/concept.md`](../templates/concept.md).
2. One concept per file. Don't combine.
3. Lead with the rule / fact, then the rationale.
4. Cross-link to experiments, dead-ends, and source files.
5. Status: `active | superseded`. If superseded, link to the replacement.
