# archive/

Old files that have been superseded but are kept for reference. **Don't delete — archive.**

## Contents

- [`RESEARCH_JOURNAL.md`](./RESEARCH_JOURNAL.md) — chronological session log used during Phase 1A. Superseded on 2026-05-01 when the docs structure switched to a spec-driven layout (ADRs + reports + research notes). Keeping it for the Phase 1A history it captures.

## When to archive

- A document was the source of truth at some point but has been superseded by a newer version (e.g., a Phase 2 brief replaces a Phase 1 brief — keep both).
- A planning artifact has been fully consumed by the brief / semantics doc and the original is no longer authoritative but still informative.
- A folder pattern was retired (the experiment / hypothesis / dead-end pattern was retired in the 2026-05-01 reorg; the spec-driven decisions / reports / research-notes shape replaced it).

## Conventions

- Move, don't copy. Use `git mv` so history follows.
- Add a one-line `**Superseded by:** [link]` banner at the top of the archived file.
- Update [`../CURRENT_STATE.md`](../CURRENT_STATE.md) if the archive event changes a state-bearing fact.

## What does NOT belong here

- Live phase work → wherever it lives now.
- Locked spec contracts → [`../specs/`](../specs/).
- Decisions about archival itself → those go in [`../decisions/`](../decisions/) when scope-relevant.
