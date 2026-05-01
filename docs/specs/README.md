# specs/

The contract documents the engine implements. **Locked during a phase.**

These files override everything else in `docs/`. When an ADR, report, or research note appears to disagree with a spec, the spec wins; surface the conflict in the dependent file rather than amending the spec mid-phase.

## Contents

- [`engine-semantics.md`](./engine-semantics.md) — what the kernel *means*. Invariants, vocabulary, the canonical definition of every concept the engine exposes.
- [`phase-1-rust-kernel-build-brief.md`](./phase-1-rust-kernel-build-brief.md) — what to *build* in Phase 1. Exact types, signatures, tests, fixtures, acceptance criteria.

## How specs evolve

- **Mid-phase amendments are rare and explicit.** If a spec genuinely needs editing during a phase, that edit happens in its own commit, before code work resumes, and the change is justified in a SPEC QUESTION exchange (see [`../../CLAUDE.md`](../../CLAUDE.md) §11).
- **New phases get new files.** When Phase 1B / Phase 2 / Phase 3 land, add a new brief here (e.g. `phase-1b-brief.md`) — do not overwrite `phase-1-rust-kernel-build-brief.md`. The older brief remains the authoritative record for what its phase shipped.
- **Cross-phase invariants belong in `engine-semantics.md`.** Phase-specific narrowings belong in the phase brief. If the brief and the semantics doc appear to conflict, [`../../CLAUDE.md`](../../CLAUDE.md) §0 lists the resolution: "If the brief intentionally narrows the semantics doc, obey the brief."

## How specs are referenced

- **Source code** documents invariants with `// Per engine-semantics.md §X I-Y-Z: …` and `// Per phase-1-rust-kernel-build-brief.md §N: …` comments. These reference the spec by basename, not by path; if a spec moves directories, the comment text remains accurate.
- **ADRs** ([`../decisions/`](../decisions/)) cross-link to specific spec sections in their **Cross-links** section.
- **Reports** ([`../reports/`](../reports/)) cite spec sections in their deviation rationales.
- **Handoffs** ([`../handoffs/`](../handoffs/)) point new phases at the relevant spec sections in their **Final checklist** resolution-order list.

## What does NOT belong here

- Decisions about how to interpret the spec → [`../decisions/`](../decisions/).
- Implementation reports → [`../reports/`](../reports/).
- Research notes that informed the spec → [`../research-notes/`](../research-notes/) or [`../external-conversations/`](../external-conversations/) (the latter for verbatim source material).
- Earlier drafts of the brief that have been superseded → [`../archive/`](../archive/).
