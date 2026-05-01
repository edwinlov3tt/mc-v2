# MarketingCubes V2 — Documentation

Spec-driven Rust systems project. This folder is the project's written record: contracts, decisions, reports, handoffs, and curated research.

For the **operating manual** (rules of engagement, hierarchy of authority, gates) see [`../CLAUDE.md`](../CLAUDE.md). That file wins every conflict with anything in this folder.

## Start here

1. [`HANDOFF.md`](./HANDOFF.md) — 5-minute orientation. Who, what, where the active work is.
2. [`CURRENT_STATE.md`](./CURRENT_STATE.md) — what is live right now (commit, gates, deferrals).
3. [`roadmap/MASTER_PHASE_PLAN.md`](./roadmap/MASTER_PHASE_PLAN.md) — the master roadmap (Phase 1 → Phase 7). The single source of truth for what phase is next; do not invent phase names without updating it.
4. [`specs/`](./specs/) — the contract documents.

## Layout

```
docs/
├── README.md                          (this file)
├── HANDOFF.md                         5-min orientation + pointer to active handoff
├── CURRENT_STATE.md                   build / test / gate / deferral snapshot
├── PERF.md                            performance baseline (Phase 1B + Phase 2A)
├── roadmap/                           master phase plan (kernel → productization)
├── product/                           product framing (PRDs, inventories) — historical
├── specs/                             engine semantics + phase briefs (the contracts)
├── decisions/                         architecture decision records (ADRs)
├── reports/                           phase completion reports + perf reports
├── handoffs/                          phase-to-phase handoff docs
├── research-notes/                    distilled lessons from research / spikes / benchmarks
├── external-conversations/            verbatim LLM dialogues that informed design
├── templates/                         blank templates (ADR, handoff, completion report, …)
└── archive/                           superseded files preserved for reference
```

For raw reference material (PDFs, books, vendor docs), see [`../research/`](../research/).

## Folder map

| Folder | What lives here | What does NOT |
|---|---|---|
| [`specs/`](./specs/) | The contract: engine semantics, phase briefs. **Locked during a phase.** | Decisions, reports, prose. |
| [`roadmap/`](./roadmap/) | The master phase plan and any forward-looking sequencing docs. **Single source of truth for "what phase next."** | Detailed implementation plans (those go in handoffs); decisions (those go in `decisions/`). |
| [`product/`](./product/) | PRDs and transfer inventories from the pre-engine phase. Historical. | Anything currently load-bearing. The brief consumed these and is the authority now. |
| [`decisions/`](./decisions/) | ADRs — one per decision, append-only, supersession-aware. | Implementation notes (those go in code) or routine choices that follow the brief verbatim. |
| [`reports/`](./reports/) | Phase completion reports + performance reports (`PERF.md`). One per phase. | Decisions (those are ADRs); ongoing work logs. |
| [`handoffs/`](./handoffs/) | Per-phase handoff documents. The bridge between phase N and phase N+1. | Permanent contracts; those go in `specs/`. |
| [`research-notes/`](./research-notes/) | Distilled lessons. One concept / finding per file, written for a future reader. | Raw transcripts (those go in `external-conversations/`). |
| [`external-conversations/`](./external-conversations/) | Verbatim LLM responses, vendor email threads, etc. — primary sources. | Decisions derived from them (those become ADRs or research-notes). |
| [`templates/`](./templates/) | Blank templates: ADR, handoff, completion report, research note. | Filled-in copies (those go in their target folder). |
| [`archive/`](./archive/) | Superseded files preserved for reference. | Active work. |

## Filing rules

1. **Specs are locked during a phase.** [`specs/engine-semantics.md`](./specs/engine-semantics.md) and [`specs/phase-1-rust-kernel-build-brief.md`](./specs/phase-1-rust-kernel-build-brief.md) do not get edited mid-phase. Future briefs (Phase 1B, Phase 2…) are added as new files; they do not overwrite earlier ones.

2. **Every phase ships a completion report.** Use [`templates/phase-completion-report.md`](./templates/phase-completion-report.md). The report lists commands run, exact test count, deviations from the brief with rationale, acceptance criteria status, files changed, deferred items, and explicit confirmation that no out-of-scope features were added. See [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md).

3. **Every phase hands off via a handoff doc.** Use [`templates/handoff.md`](./templates/handoff.md). The handoff embeds the next-phase prompt verbatim, captures landmarks the receiving instance will need (commit hash, test counts, fixture surface, caches), and lists touch / don't-touch files. See [`handoffs/phase-1b-handoff.md`](./handoffs/phase-1b-handoff.md).

4. **Every non-trivial decision gets an ADR.** Use [`templates/adr.md`](./templates/adr.md). Status (`Proposed | Accepted | Deprecated | Superseded by ADR-NNNN`), context, decision, consequences, alternatives considered. ADRs are append-only — when revised, the new one supersedes the old. See [`decisions/0001-phase-1-scope.md`](./decisions/0001-phase-1-scope.md).

5. **Every research finding worth keeping gets a research note.** Use [`templates/research-note.md`](./templates/research-note.md). One concept per file. Cross-link to source code, ADRs, and external-conversation primary sources.

6. **Cross-link everywhere.** Reports link to ADRs. ADRs link to specs. Research notes link to external conversations. Use relative paths so links work when browsing the filesystem.

7. **Specs override everything else** (per [`../CLAUDE.md`](../CLAUDE.md) §0). When an ADR or report appears to disagree with a spec, the spec wins; surface the conflict explicitly rather than normalizing it.

## How a typical phase flows

```
specs/<phase>-brief.md          (locked input — written before the phase starts)
       │
       ▼
decisions/<NNNN>-<slug>.md      (ADRs written when scope/design choices land)
       │
       ▼
[implementation in crates/]
       │
       ▼
reports/phase-<N>-completion-report.md
       │
       ▼
handoffs/phase-<N+1>-handoff.md
```

Research notes, external conversations, and product artifacts feed in throughout — they are sources, not phase outputs.
