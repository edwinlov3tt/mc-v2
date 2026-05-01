# ADR-NNNN: [Title — short, decision-shaped]

**Status:** `Proposed | Accepted | Deprecated | Superseded by ADR-NNNN`
**Date:** YYYY-MM-DD
**Deciders:** [who decided — usually the project owner + the implementing instance]
**Phase:** `1A | 1B | 2 | …`

---

## Context

What is the situation that requires a decision? What facts, constraints, and forces are at play? Cite the brief, the semantics doc, CLAUDE.md, prior ADRs, and prior reports as needed.

Be concrete. The reader (often a future instance of yourself) needs enough context to judge whether the decision still applies if conditions change.

## Decision

The decision in one sentence. Then unpack what it means in concrete terms — types created, behavior chosen, scope drawn.

**What we are doing:**

- …
- …

**What we are explicitly NOT doing:**

- …
- …

## Consequences

What follows from this decision — both the upsides we wanted and the costs we accepted.

**Positive:**

- …

**Negative / accepted trade-offs:**

- …

**Reversal cost:**

How hard would it be to change this later? Cheap, expensive, or one-way? If one-way, why is that acceptable now?

## Alternatives considered

For each rejected alternative, one paragraph: what it was, why we rejected it. Don't paper over reasonable alternatives — show the work.

1. **[Alternative 1].** …
2. **[Alternative 2].** …

## Cross-links

- Spec sections: [`../specs/engine-semantics.md`](../specs/engine-semantics.md) §…, [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) §…
- Related ADRs: [`./NNNN-...md`](./)
- Source code where this decision lives: [`../../crates/...`](../../crates/)
- Reports / handoffs that reference this decision: [`../reports/...`](../reports/), [`../handoffs/...`](../handoffs/)

## Notes

Anything that doesn't fit above but a future reader will want to know — false starts, performance implications observed later, surprises.
