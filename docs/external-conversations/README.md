# external-conversations/

Verbatim primary sources: LLM dialogues, vendor email threads, and other external-origin text that informed the design.

This is the **transcript** layer. Distilled takeaways belong in [`../research-notes/`](../research-notes/); decisions belong in [`../decisions/`](../decisions/); shipped phase audits belong in [`../reports/`](../reports/).

## Contents

- [`chat-gpt-response-1.md`](./chat-gpt-response-1.md) — GPT-5 critiquing the original PRD + transfer inventory: "very strong foundation, but it is not ready to execute as-is." Argued the spec needed tightening into an executable engine specification before any Rust work.
- [`claude-response-2.md`](./claude-response-2.md) — Claude's response, conceding most of GPT-5's points.
- [`chat-gpt-response-2.md`](./chat-gpt-response-2.md) — GPT-5's follow-up reply.
- [`claude-xgboost.md`](./claude-xgboost.md) — Claude analyzing XGBoost experiments from the predecessor project. **Not Phase 1 scope** — model cells are explicitly out of Phase 1 and 1B per the briefs and [`../decisions/0001-phase-1-scope.md`](../decisions/0001-phase-1-scope.md).

The exchange in the first three files is the rationale chain that produced the Phase 1 scope decision. If you want to know why Phase 1 is so narrow, that is where the argument plays out.

## How to add entries

- One file per response or session. Filename: `<source>-<topic>.md` (e.g. `chat-gpt-response-1.md`, `gemini-toolchain-blocker.md`, `vendor-tm1-clarification.md`).
- **Don't paraphrase** — paste the response verbatim. The session that consumed it summarizes in a research note ([`../research-notes/`](../research-notes/)) or an ADR ([`../decisions/`](../decisions/)).
- Include the date and source somewhere in the file (filename, front-matter, or first paragraph) if not obvious.

## What does NOT belong here

- Distilled lessons → [`../research-notes/`](../research-notes/).
- Decisions derived from these conversations → [`../decisions/`](../decisions/).
- Phase audits → [`../reports/`](../reports/).
- Reference manuals / books / vendor PDFs → [`../../research/`](../../research/) (binary files belong in the parallel `research/` tree).
