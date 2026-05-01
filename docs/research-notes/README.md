# research-notes/

**Distilled lessons from research, spikes, benchmarks, and surprise findings.**

Where [`../external-conversations/`](../external-conversations/) holds verbatim primary sources (LLM responses, vendor threads), this folder holds the *takeaways* — short, opinionated notes written for a future reader who needs the conclusion, not the transcript.

A research note is the right shape when:

- You investigated something (a benchmark, a library, a TM1 manual, a thread of advice from another model) and the conclusion is worth keeping.
- The conclusion will inform an ADR or report later but isn't itself a decision.
- The conclusion is broader than a single phase — it's a fact about the world or about the engine that future phases need to know.

If the finding is itself a decision (we chose X over Y), write it as an ADR in [`../decisions/`](../decisions/) instead.
If the finding is one phase's report (we shipped X with these gates), write it in [`../reports/`](../reports/) instead.

## Status

Empty as of Phase 1A ship.

## Candidate topics

The Phase 1A code embodies several non-obvious choices whose rationale is currently distributed across the brief, the report, and CLAUDE.md. Promoting any of these into a self-contained research note is appropriate when a future phase needs to lean on the rationale:

- **Lazy dependency graph.** Why edges materialize on first read rather than at construction. Sources: [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) §3.12, [`../../crates/mc-core/src/dependency.rs`](../../crates/mc-core/src/dependency.rs), [`../../CLAUDE.md`](../../CLAUDE.md) §2.1.
- **Dirty propagation as a per-write delta.** Why §10.1 dirty-set assertions are framed as deltas, not absolutes. Source: [`../reports/phase-1-completion-report.md`](../reports/phase-1-completion-report.md) §4.2.
- **Null vs zero vs NaN.** The spec §7 rules, where they're enforced, what bugs they prevent. Sources: [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) §7, [`../../CLAUDE.md`](../../CLAUDE.md) §2.5, [`../../crates/mc-core/src/value.rs`](../../crates/mc-core/src/value.rs), [`../../crates/mc-core/src/rule.rs`](../../crates/mc-core/src/rule.rs).
- **Weighted-average consolidation.** Why CPC / CVR / Close_Rate / AOV / COGS_Rate don't simple-sum. Sources: [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) §3.17, [`../../CLAUDE.md`](../../CLAUDE.md) §2.10, [`../../crates/mc-core/src/consolidation.rs`](../../crates/mc-core/src/consolidation.rs).
- **Two caching layers in `read`.** Derived-leaf cache + consolidated cache, the dirty bit as cache invalidator, the trace-bypass-cache rule. Source: [`../../crates/mc-core/src/cube.rs`](../../crates/mc-core/src/cube.rs) `read_derived_leaf` and `read_consolidated`.
- **Snapshot as deep-clone.** Why Phase 1 snapshots are clones rather than COW. Sources: [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) §3.16, [`../../CLAUDE.md`](../../CLAUDE.md) §2.18, [`../../crates/mc-core/src/snapshot.rs`](../../crates/mc-core/src/snapshot.rs).

Phase 1B benchmarks are likely to produce two more research notes:

- **Criterion compatibility under Rust 1.78.** What we tried (specific version pins), what worked, what didn't, and what the closure conditions are.
- **Hierarchy clone overhead in consolidated reads.** Whether the per-read clone in `cube.rs::read_consolidated` is meaningful at Acme scale.

## How to write a research note

1. Copy [`../templates/research-note.md`](../templates/research-note.md).
2. One topic per file. Don't combine.
3. Lead with the conclusion in one sentence, then unpack the *why*.
4. Cross-link source code, ADRs, primary-source conversations, and reports.
5. Status: `active | superseded`. If superseded, link to the replacement note or ADR.
