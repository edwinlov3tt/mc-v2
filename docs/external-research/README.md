# external-research/

LLM responses, vendor research, and other external-source material that informed the design but is not itself a contract.

## Contents

- [`chat-gpt-response-1.md`](./chat-gpt-response-1.md) — GPT-5 critiqued the original PRD + transfer inventory; argued the foundation was strong but the spec needed tightening before code work.
- [`claude-response-2.md`](./claude-response-2.md) — Claude's response, conceding most points.
- [`chat-gpt-response-2.md`](./chat-gpt-response-2.md) — GPT-5's follow-up reply.
- [`claude-xgboost.md`](./claude-xgboost.md) — Claude analyzed XGBoost experiments from the predecessor project. **Not Phase 1 scope** — model cells are explicitly out of Phase 1 and 1B per the briefs.

## How to add entries

- One file per response or session. Use a short, descriptive filename (`<source>-<topic>.md` or `<source>-response-<n>.md`).
- Front-matter is optional but include the date and source if not in the filename.
- Don't paraphrase — paste the response verbatim. The session that consumed it can summarize in [`../RESEARCH_JOURNAL.md`](../RESEARCH_JOURNAL.md) or a [`../concepts/`](../concepts/) file.

## What does NOT belong here

- Decisions derived from this research → [`../concepts/`](../concepts/) (cross-cutting) or the relevant phase report.
- Reference manuals / books / vendor PDFs → [`../../research/`](../../research/) (binary files belong in the parallel `research/` tree).
