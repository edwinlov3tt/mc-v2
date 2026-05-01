# handoffs/

Phase-to-phase handoff documents. **One file per upcoming phase**, written by the outgoing instance for the incoming instance.

Use [`../templates/handoff.md`](../templates/handoff.md) as the starting point.

## Contents

- [`phase-1b-handoff.md`](./phase-1b-handoff.md) — Phase 1B: Benchmark Baseline + PERF.md. Closes Phase 1A's deferred benchmark gate.

## What every handoff MUST include

1. **Where the previous phase ended.** Commit hash, test counts, gate status, deferred items.
2. **The next-phase prompt verbatim.** This is the contract.
3. **Context the prompt does not spell out.** Landmarks, surface area, decisions made during the previous phase that the next instance needs to know.
4. **Touch / don't-touch file table.** Make it easy to know what's locked.
5. **Reproducible commands.** Things that exit 0 today on the inherited HEAD.
6. **A final checklist** the next instance has to clear before claiming done.

## How handoffs flow

Outgoing instance writes the handoff at the end of its phase. Incoming instance reads it first thing. The handoff is the **only** trustworthy way for the next instance to know what is locked and what is fair game.

If the next phase needs to amend the brief / semantics doc, that amendment happens in its own commit **before** the handoff is consumed; the handoff itself never amends contract documents.
