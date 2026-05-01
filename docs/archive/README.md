# archive/

Old files that have been superseded but are kept for reference. **Don't delete — archive.**

## Status

Empty as of Phase 1A ship.

## When to archive

- A document was the source of truth for a phase but has been superseded by a newer version (e.g., a Phase 2 brief replaces a Phase 1 brief — keep both).
- A planning artifact has been fully consumed by the brief / semantics doc and the original is no longer authoritative but still informative.
- A handoff was written but the receiving phase pivoted; archive the original alongside the new handoff.

## Conventions

- Move, don't copy. Use `git mv` so history follows.
- Add a one-line `**Superseded by:** [link]` banner at the top of the archived file.
- Update [`../RESEARCH_JOURNAL.md`](../RESEARCH_JOURNAL.md) with the archive event.

## What does NOT belong here

- Live phase work → wherever it lives now.
- Failed experiments → [`../dead-ends/`](../dead-ends/) (they have specific reopen conditions; archive entries don't).
- Locked input contracts (engine semantics + brief) → `docs/` root.
