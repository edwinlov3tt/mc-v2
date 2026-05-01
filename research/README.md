# research/

Reference PDFs, books, and vendor documents that inform the engine's design but are not themselves contracts. **Binary files only.**

For markdown research artifacts (LLM responses, design notes, etc.) see [`../docs/external-research/`](../docs/external-research/).

For the Phase 1 contracts see [`../docs/engine-semantics.md`](../docs/engine-semantics.md) and [`../docs/phase-1-rust-kernel-build-brief.md`](../docs/phase-1-rust-kernel-build-brief.md).

## Structure

- [`tm1/`](./tm1/) — IBM TM1 reference manuals. The dominant prior art for the engine; the brief's terminology and many invariant choices trace back to here.
- [`books/`](./books/) — book excerpts and chapters that informed the design.
- [`architecture/`](./architecture/) — third-party architecture and infrastructure spec documents.

## How to add references

- Drop the file in the right subfolder. Rename to a stable, no-spaces, no-special-character filename if needed.
- Add a one-line entry to the subfolder's `README.md` describing what the file is and why it's here.
- If the reference triggered a design decision, write it up in [`../docs/concepts/`](../docs/concepts/) — don't bury insights in research/ READMEs.

## What does NOT belong here

- Markdown research → [`../docs/external-research/`](../docs/external-research/).
- Project planning artifacts (PRDs, transfer inventories) → [`../docs/planning/`](../docs/planning/).
- Code → [`../crates/`](../crates/).
