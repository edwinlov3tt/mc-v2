# For-Dummies

> **Plain-English explanations of what the hell is going on in this project.** Same content as the technical docs — just translated into normal-human.

If you find yourself reading something in `docs/specs/`, `docs/research-notes/`, or `CLAUDE.md` and your eyes are glazing over, come here first.

---

## What MarketingCubes V2 actually is

Imagine the world's most paranoid Excel spreadsheet.

That's about it. Really.

A normal Excel spreadsheet has rows and columns — two dimensions. Our spreadsheet has **six** dimensions: time, market, channel, scenario, version, and measure. Instead of one big grid, it's a six-dimensional grid where every cell is identified by a six-part address like *(Q1, Florida, Paid_Search, Baseline, Working, Spend)*.

Cells fall into two camps:

- **Input cells** — numbers you type in. *"Florida-Tampa-Paid-Search Spend in March was $11,500."*
- **Derived cells** — numbers that are computed from a formula referencing other cells. *"Clicks = Spend ÷ Cost-Per-Click."* You don't type Clicks; the engine works it out.

When you change an input cell, the engine auto-recomputes every derived cell that depended on it — exactly the same as Excel. The difference is that this engine is being built in Rust to run on data far too big for Excel, with planning-finance-grade rules about *who can edit what when*, what happens if two people edit at once, what happens during a "freeze" before quarterly close, and so on.

It's modeled after a 1980s IBM tool called **TM1** that planning/finance teams have used for forty years. We're rebuilding the kernel — the math engine at the bottom — in modern Rust.

## Why is this so over-engineered?

Because finance is unforgiving. If a CFO is presenting Q3 forecasts to the board and one cell is wrong, "the engine has a small rounding bug" is not an acceptable answer. So:

- Every value has a **provenance** stamp telling you where it came from (an input from user X at time Y, or rule R evaluated at revision Z).
- Every change creates a new **revision number** so old reports stay reproducible.
- Every "approved version" of the cube can be **snapshotted** so you can roll back if a later edit went wrong.
- You can ask the engine: *"why does this cell say $42,000?"* — and it'll show you the **trace**, the entire tree of cells and rules that produced the answer.

This is why the project is so much code for what looks like a spreadsheet. The "spreadsheet" part is easy; the *defensible-in-an-audit* part is what takes the work.

## Where to look in this folder

This `for-dummies/` folder is a parallel translation of the more technical docs:

```
docs/for-dummies/
├── research-notes/        plain-English versions of docs/research-notes/
└── phases/                plain-English explanations of what each optimization phase did + why
```

Layman's notes in `research-notes/` have the **same filename** as their technical counterpart, so if you read the for-dummies version and want the deep version, go to `docs/research-notes/<same-filename>.md`.

Notes in `phases/` cover **what each Phase 2-and-onward sub-phase actually did and why** — Phase 2C, Phase 2D, etc. The technical counterparts live across `docs/reports/<phase>-completion-report.md`, `docs/handoffs/<phase>-handoff.md`, and `docs/PERF.md`; the for-dummies version stitches them together into a single "what / why / what-if-we-didn't" narrative per phase.

## What the technical folders are for

| Folder | What's in it | When you'd read it |
|---|---|---|
| [`../specs/`](../specs/) | The contract — what to build, what it means | When deciding what's in scope |
| [`../decisions/`](../decisions/) | "ADRs" = records of why we picked X over Y | When something seems weird and you want the history |
| [`../reports/`](../reports/) | Phase completion reports | When confirming what shipped |
| [`../handoffs/`](../handoffs/) | Notes from one phase's Claude to the next | When picking up work |
| [`../research-notes/`](../research-notes/) | Distilled lessons; this is what for-dummies translates | When you need the deep version |
| [`../external-conversations/`](../external-conversations/) | Verbatim chats with other AIs that informed design | Curiosity / context |
| [`../product/`](../product/) | The original PRD before scope-cutting happened | Historical |
| `for-dummies/` (here) | Plain-English versions of the above | When you're me, not the engineer |
