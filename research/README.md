# research/

Reference PDFs, books, and vendor documents that inform the engine's design but are not themselves contracts. **Binary files only.**

For markdown research artifacts (verbatim LLM dialogues), see [`../docs/external-conversations/`](../docs/external-conversations/).
For distilled lessons, see [`../docs/research-notes/`](../docs/research-notes/).
For decisions, see [`../docs/decisions/`](../docs/decisions/).

For the Phase 1 contracts see [`../docs/specs/engine-semantics.md`](../docs/specs/engine-semantics.md) and [`../docs/specs/phase-1-rust-kernel-build-brief.md`](../docs/specs/phase-1-rust-kernel-build-brief.md).

## Structure

- [`tm1/`](./tm1/) — IBM TM1 reference manuals. The dominant prior art for the engine; the brief's terminology and many invariant choices trace back to here.
- [`books/`](./books/) — book excerpts and chapters that informed the design.
- [`architecture/`](./architecture/) — third-party architecture and infrastructure spec documents.

## How to add references

- Drop the file in the right subfolder. Rename to a stable, no-spaces, no-special-character filename if needed.
- Add a one-line entry to the subfolder's `README.md` describing what the file is and why it's here.
- If the reference triggered a takeaway worth preserving, write a research note at [`../docs/research-notes/`](../docs/research-notes/). If it triggered a non-trivial scope or design decision, write an ADR at [`../docs/decisions/`](../docs/decisions/). **Don't bury insights in research/ READMEs.**

## What does NOT belong here

- Markdown research artifacts → [`../docs/external-conversations/`](../docs/external-conversations/) (transcripts) or [`../docs/research-notes/`](../docs/research-notes/) (distilled).
- Product framing (PRDs, transfer inventories) → [`../docs/product/`](../docs/product/).
- Code → [`../crates/`](../crates/).
