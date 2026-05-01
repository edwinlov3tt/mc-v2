# reports/

Phase completion reports and performance reports. **One file per phase**, written when the phase ships.

Use [`../templates/phase-completion-report.md`](../templates/phase-completion-report.md) as the starting point. Do not paraphrase the brief; do paraphrase your decisions.

## Contents

- [`phase-1-completion-report.md`](./phase-1-completion-report.md) — Phase 1A audit: commands run, 203 / 0 test count, 5 deviations with rationale, acceptance criteria status (9 of 10 satisfied; criterion 5 deferred), files implemented, Phase 2 follow-ups, no out-of-scope features.

Phase 1B will land [`PERF.md`](./PERF.md) here when the benchmark gate closes (see [`../handoffs/phase-1b-handoff.md`](../handoffs/phase-1b-handoff.md)).

## What every report MUST include

(Per the template; do not skip.)

1. Commands run + summarized outputs.
2. Final test count.
3. Deviations from the brief.
4. Exact rationale per deviation.
5. Acceptance criteria — complete.
6. Acceptance criteria — deferred.
7. Implemented files / modules.
8. Known follow-ups for the next phase.
9. Confirmation no out-of-scope features were added.

## Order of authority for the audit

When the audit and the brief disagree:

1. The brief / engine semantics doc.
2. CLAUDE.md.
3. The report itself.

The report describes the world; it does not amend the contract. Drift gets surfaced in the report, not normalized into it.
