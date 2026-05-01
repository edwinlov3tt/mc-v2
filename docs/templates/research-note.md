# [Research note title — short, conclusion-shaped]

**Status:** `active | superseded`
**Created:** YYYY-MM-DD
**Last touched:** YYYY-MM-DD
**Spans phases:** `1A, 1B, 2, …`

---

## Conclusion (one sentence)

Lead with the takeaway. The reader should know the answer before they read the rationale.

## Why this matters

What goes wrong without this knowledge. What decisions or implementations rely on it. Which phases will need it.

## Evidence

What we observed, measured, or read that produced the conclusion. Be concrete:

- Benchmark numbers, with the command to reproduce.
- Spec sections quoted verbatim.
- Source-code excerpts with file:line.
- LLM dialogue links to [`../external-conversations/`](../external-conversations/).
- Vendor manual page numbers (PDFs in [`../../research/`](../../research/)).

## Where it shows up in the engine

- **Source:** [`../../crates/.../foo.rs`](../../crates/...) — the production enforcement / use point(s).
- **Tests:** [`../../crates/mc-core/tests/...`](../../crates/mc-core/tests/) — the assertion(s) that lock the contract.
- **Spec:** [`../specs/engine-semantics.md`](../specs/engine-semantics.md) §X / [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) §Y.
- **ADR (if any):** [`../decisions/...`](../decisions/) — the decision that rests on this conclusion.

## Edge cases / gotchas

What surprises a reader. What the conclusion does NOT cover. What is easy to get wrong if you're not paying attention.

## Related notes

- [`./other-note.md`](./)

## History

- YYYY-MM-DD — created.
- YYYY-MM-DD — refined after [observation / benchmark / spec amendment].
- (If `superseded`) → replaced by [`./new-note.md`](./new-note.md).
