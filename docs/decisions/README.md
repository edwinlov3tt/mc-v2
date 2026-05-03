# decisions/

Architecture Decision Records (ADRs). **One file per decision.**

Each ADR captures **why** the engine looks the way it does at a moment in time — not just what was chosen, but the alternatives considered, the trade-offs accepted, and what would need to change to revisit the decision.

ADRs are append-only. When a decision is revised, the new ADR supersedes the old one (and the old one's status becomes `Superseded by ADR-NNNN`); the original record is preserved.

## Format

ADRs follow the standard short form (Michael Nygard style):

- **Status** — `Proposed | Accepted | Deprecated | Superseded by ADR-NNNN`
- **Context** — what situation forces a decision
- **Decision** — what we chose, in concrete terms
- **Consequences** — what follows (upsides, accepted trade-offs, reversal cost)
- **Alternatives considered** — what we rejected and why
- **Cross-links** — to specs, source, reports, related ADRs

Use [`../templates/adr.md`](../templates/adr.md) as the starting point.

## Naming

`NNNN-short-slug.md` where `NNNN` is a four-digit sequence number, zero-padded. Number sequentially. Do not renumber when one ADR supersedes another — supersession is captured in the status field.

## Index

| ADR | Title | Status |
|---|---|---|
| [0001](./0001-phase-1-scope.md) | Phase 1 scope: smallest kernel that runs the Acme demo | Accepted |
| [0002](./0002-perf-assertions-in-benchmarks-not-tests.md) | Performance assertions belong in criterion benchmarks, not in `cargo test` | Accepted |
| [0003](./0003-workload-sketch.md) | Workload sketch & perception thresholds | Accepted — Provisional (sunset 2026-11-01) |
| [0004](./0004-phase-3a-model-definition-format.md) | Phase 3A model-definition format & parser scope | Accepted (with acceptance amendments) |

## When to write an ADR

- **Anytime the project draws a scope line** that affects what does and does not get built. Phase 1 scope (this ADR file's first entry) is the canonical example.
- **Anytime a non-trivial implementation choice has at least one credible alternative.** If the choice was obvious, it's not an ADR.
- **Anytime the engine behavior locks in a contract** that downstream phases will be built on top of.

## When NOT to write an ADR

- Routine implementation that follows the brief and semantics doc verbatim.
- Bug fixes (those are commit messages).
- Documentation-shape choices (those are README content).

## Relationship to the rest of `docs/`

| File type | Captures | Lives in |
|---|---|---|
| Brief / engine-semantics | The contract the engine must implement | [`../specs/`](../specs/) |
| ADR | A decision about scope, design, or trade-offs | [`./`](./) (here) |
| Phase completion report | What shipped + acceptance criteria | [`../reports/`](../reports/) |
| Phase handoff | What the next phase needs to know | [`../handoffs/`](../handoffs/) |
| Research note | A distilled lesson from research / a benchmark / a spike | [`../research-notes/`](../research-notes/) |

ADRs and reports complement each other: a report says "we shipped X with these gates"; an ADR says "we chose X over Y because of Z."
