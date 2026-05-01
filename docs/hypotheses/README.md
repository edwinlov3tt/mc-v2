# hypotheses/

Open research questions with status. **A hypothesis is a question we have not yet answered.** Once answered, the hypothesis becomes either an experiment file (with the answer) or a dead-end file (if the answer is "this approach doesn't work").

## Status

Empty as of Phase 1A ship.

## Conventions

- Filename: `H<NNN>-<short-slug>.md`. Number sequentially as `H001`, `H002`, ….
- Use [`../templates/hypothesis.md`](../templates/hypothesis.md) as the starting point.
- Status field: `open | testing | confirmed | rejected | superseded`.

## What every hypothesis file MUST include

1. **Claim** — the specific assertion in one sentence.
2. **Why we care** — why this question matters now.
3. **Prerequisites** — what needs to exist before we can test (a fixture, a benchmark harness, a particular dataset).
4. **Expected test method** — how we would test it.
5. **Status + last-touched date.**
6. **Resolution link** (when closed) — to the experiment, dead-end, or concept that answers it.

## How hypotheses flow

```
open → testing → (confirmed | rejected) → file experiment + concept (or dead-end)
                                       ↓
                             update CURRENT_STATE.md if state-bearing
                             append to RESEARCH_JOURNAL.md
```

## Candidate topics

These are open questions Phase 1A surfaced. They are not formal hypotheses yet; promote them when a session is ready to test.

- Will pinning criterion to `0.4` (or earlier) sidestep the `clap_lex 1.1.0` / `edition2024` blocker on Rust 1.78? (Phase 1B handoff §A is the place to answer.)
- Does the consolidated-cache hit rate justify the cost of the per-read hierarchy clones in `cube.rs::read_consolidated`? (Phase 1B benchmarks should be enough to answer.)
- Is `Snapshot`'s deep-clone cost a problem at 25K cells (Acme scale) for the demo path? (Phase 1B benchmarks will tell.)
