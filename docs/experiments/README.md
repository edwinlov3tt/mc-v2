# experiments/

Dated experiment reports. **Every experiment gets a file. Null results required.**

This is the appropriate place for Phase 1B benchmark runs (alternative tooling attempts, criterion vs std-only comparisons, hot-spot characterization), and for any future Phase 2+ characterization work.

## Status

Empty as of Phase 1A ship.

## Conventions

- Filename: `YYYY-MM-DD-<short-slug>.md`. Date first so files sort chronologically.
- Use [`../templates/experiment.md`](../templates/experiment.md) as the starting point.
- One experiment per file. If two questions are entangled, write two files and link them.
- **Null results are required to be documented** — a failed experiment prevents re-attempts and is worth as much as a successful one. CLAUDE.md §2.6 is the corollary.

## What every experiment file MUST include

(Per the template; do not skip.)

1. **Hypothesis** — the specific claim being tested.
2. **Method** — data, baseline, variants, metrics.
3. **Results** — full numbers, not summaries.
4. **Interpretation** — what the numbers mean.
5. **Decision** — did we ship? did we abandon? did we file a hypothesis?
6. **Cross-links** — to concepts, hypotheses, dead-ends.

## What does NOT belong here

- Production phase reports → [`../reports/`](../reports/).
- Open questions without a test → [`../hypotheses/`](../hypotheses/).
- Failed approaches → [`../dead-ends/`](../dead-ends/) (with explicit reopen conditions).
