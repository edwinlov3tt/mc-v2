# audits/

External or self-run audit reports. **One file per audit pass.**

An audit is different from a phase completion report:
- A **completion report** ([`../reports/`](../reports/)) is written by the instance that built the phase — first-person, narrative, decision-justifying.
- An **audit** is written by an outside reviewer (LLM, human, automated tool) — third-person, finding-driven, not committed to defending decisions.

## Status

Empty as of Phase 1A ship.

## When to commission an audit

- After a phase ships and before the next phase starts (so findings land in the next handoff).
- When the project has been idle for a long enough stretch that the team has forgotten the live state.
- When a benchmark or test suite produces an unexpected result and the instance running it isn't sure if its mental model is wrong.
- When the user asks for one.

## Conventions

- Filename: `YYYY-MM-DD-<source>-<scope>.md`. Examples: `2026-06-15-gpt-phase-1b-bench-review.md`, `2026-07-01-self-pre-phase-2-readiness.md`.
- Include who/what ran the audit, against what commit, with what scope.
- Findings are categorized: `blocker | finding | suggestion | informational`.
- Each finding has a recommended action and a link to where the action would go (an issue file, a hypothesis, a concept update, etc.).
