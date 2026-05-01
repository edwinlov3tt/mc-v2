# dead-ends/

Approaches we tried that didn't work. **Every dead-end file MUST include "exact conditions at failure" and "what would need to change for this to work"** so a future session knows when to re-open the question.

## Status

Empty as of Phase 1A ship. (Phase 1A had no failed approaches that merited their own files; deviations from the brief are tracked in [`../reports/phase-1-completion-report.md`](../reports/phase-1-completion-report.md) §3-§4 instead because they are deliberate, contractual choices, not failed attempts.)

## Conventions

- Filename: `YYYY-MM-DD-<short-slug>.md` so they sort chronologically.
- Use [`../templates/dead-end.md`](../templates/dead-end.md) as the starting point.

## What every dead-end file MUST include

1. **What we tried** — concrete description of the approach.
2. **Why we tried it** — what we hoped it would unlock.
3. **What happened** — the failure mode, with the actual error / measurement / contradiction.
4. **Exact conditions at failure** — toolchain version, cube size, fixture, code commit. This is the bar for "could this work later under different conditions?"
5. **What would need to change for this to work** — explicit reopen conditions.
6. **Cross-links** — to the experiment that proved it, the hypothesis it answered, the concept it touches.

## Why this is non-negotiable

Without explicit reopen conditions, a dead-end becomes folklore: "we tried that and it didn't work, don't try again." The whole point of writing it down is to know **when the conditions change enough to revisit.** A dead-end without conditions is worse than no record at all.
