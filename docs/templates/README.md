# templates/

Blank templates for the recurring file types in this docs tree. **Copy these. Don't reinvent them.**

## Index

| Template | Where to put the filled-in copy | When to use |
|---|---|---|
| [`adr.md`](./adr.md) | [`../decisions/<NNNN>-<slug>.md`](../decisions/) | Every non-trivial scope/design/trade-off decision |
| [`handoff.md`](./handoff.md) | [`../handoffs/phase-<N>-handoff.md`](../handoffs/) | One per upcoming phase |
| [`phase-completion-report.md`](./phase-completion-report.md) | [`../reports/phase-<N>-completion-report.md`](../reports/) | One per shipped phase |
| [`research-note.md`](./research-note.md) | [`../research-notes/<slug>.md`](../research-notes/) | A distilled lesson worth preserving across phases |

## Filing rules

The filing rules live in [`../README.md`](../README.md). Read them.

## How to use a template

1. Copy the template file to the destination folder under its real name.
2. Fill in every section. Empty section headings are noise; either fill them or delete them.
3. Cross-link related files.
4. Update [`../CURRENT_STATE.md`](../CURRENT_STATE.md) if the new file changes a state-bearing fact (a deferral closed, a new ADR landed, a phase shipped).

## How to update a template

If a template is missing a section that several real files needed, add it to the template. If a section in the template has been empty in the last three real uses, consider removing it. Edit deliberately — these templates set the convention.

## Removed templates (superseded by the spec-driven layout)

The earlier docs structure included `experiment.md`, `hypothesis.md`, `dead-end.md`, and a `concept.md` template that drove `concepts/`, `experiments/`, `hypotheses/`, `dead-ends/`, and `audits/` folders. That pattern was retired in the docs reorganization on 2026-05-01: a spec-driven Rust systems project does not need an experiment-log shape. Cross-cutting lessons now live in [`../research-notes/`](../research-notes/), and decisions live in [`../decisions/`](../decisions/) — see [`../README.md`](../README.md) for the new layout.
