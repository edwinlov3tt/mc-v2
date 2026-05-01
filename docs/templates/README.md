# templates/

Blank templates for the recurring file types in this docs tree. **Copy these. Don't reinvent them.**

## Index

| Template | Where to put the filled-in copy | When to use |
|---|---|---|
| [`experiment.md`](./experiment.md) | [`../experiments/YYYY-MM-DD-<slug>.md`](../experiments/) | Every experiment, including null-result ones |
| [`hypothesis.md`](./hypothesis.md) | [`../hypotheses/H<NNN>-<slug>.md`](../hypotheses/) | Every open question with a test method in mind |
| [`concept.md`](./concept.md) | [`../concepts/<slug>.md`](../concepts/) | Cross-cutting engine concepts that span phases |
| [`dead-end.md`](./dead-end.md) | [`../dead-ends/YYYY-MM-DD-<slug>.md`](../dead-ends/) | Approaches that didn't work; **with explicit reopen conditions** |
| [`handoff.md`](./handoff.md) | [`../handoffs/phase-<N>-handoff.md`](../handoffs/) | One per upcoming phase |
| [`phase-completion-report.md`](./phase-completion-report.md) | [`../reports/phase-<N>-completion-report.md`](../reports/) | One per shipped phase |

## Filing rules

The filing rules live in [`../README.md`](../README.md). Read them.

## How to use a template

1. Copy the template file to the destination folder under its real name.
2. Fill in every section. Empty section headings are noise; either fill them or delete them.
3. Cross-link related files.
4. Update [`../RESEARCH_JOURNAL.md`](../RESEARCH_JOURNAL.md) with a one-line entry pointing at the new file.

## How to update a template

If a template is missing a section that several real files needed, add it to the template. If a section in the template has been empty in the last three real uses, consider removing it. Edit deliberately — these templates set the convention.
