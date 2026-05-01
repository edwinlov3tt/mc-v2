# MarketingCubes V2 — Documentation

This is the **research, planning, and learning log** for the MarketingCubes V2 Rust kernel.

For the **operating manual** (rules of engagement, hierarchy of authority, gates) see [`../CLAUDE.md`](../CLAUDE.md). That file wins every conflict with anything in this folder.

For the **locked-input contract documents** see:
- [`engine-semantics.md`](./engine-semantics.md) — what the kernel *means* (invariants, semantics).
- [`phase-1-rust-kernel-build-brief.md`](./phase-1-rust-kernel-build-brief.md) — what to *build* in Phase 1 (exact types, tests, fixtures).

These two are **never edited** during a phase. If they need amendments, they happen in their own commit, before any code work.

---

## Navigation

**Start here for a new session:**

1. [`HANDOFF.md`](./HANDOFF.md) — pointer to the current active handoff and the 60-second project orientation.
2. [`CURRENT_STATE.md`](./CURRENT_STATE.md) — what's live RIGHT NOW (commit, test count, gates, deferrals).
3. [`RESEARCH_JOURNAL.md`](./RESEARCH_JOURNAL.md) — chronological log of what was tried, what shipped, what failed.

**Locked input documents (do not edit):**

- [`engine-semantics.md`](./engine-semantics.md)
- [`phase-1-rust-kernel-build-brief.md`](./phase-1-rust-kernel-build-brief.md)

**Phase work:**

- [`reports/`](./reports/) — phase completion reports (one per phase, written when the phase ships)
- [`handoffs/`](./handoffs/) — handoff documents from one phase to the next

**Knowledge (cross-phase):**

- [`concepts/`](./concepts/) — engine-level concepts that span phases (lazy dep graph, dirty propagation semantics, weighted aggregation, etc.)
- [`experiments/`](./experiments/) — dated experiment reports (benchmarks, alternative implementations, characterization runs). Null results required.
- [`hypotheses/`](./hypotheses/) — open research questions with status (`open | testing | confirmed | rejected`).
- [`dead-ends/`](./dead-ends/) — approaches that failed; each entry includes "exact conditions at failure" and "what would need to change for this to work."
- [`audits/`](./audits/) — external or self-run audit reports.

**Planning + history:**

- [`planning/`](./planning/) — original PRDs and transfer inventories from the project's pre-engine phase. Historical, kept for context.
- [`external-research/`](./external-research/) — back-and-forth research with external models (GPT-5, Claude, etc.) that informed the design.
- [`archive/`](./archive/) — older docs that have been superseded but worth keeping for reference.

**Authoring tools:**

- [`templates/`](./templates/) — blank templates for experiments, hypotheses, concepts, dead-ends, handoffs, and phase completion reports. Copy these; don't reinvent them.

---

## Filing rules (non-negotiable)

These mirror the claw-core conventions that made that project's research log useful across many sessions.

1. **Every experiment gets a file.** Use [`templates/experiment.md`](./templates/experiment.md). Null results are required to be documented — a failed experiment prevents re-attempts and is worth as much as a successful one.

2. **Every phase ships with a completion report.** Use [`templates/phase-completion-report.md`](./templates/phase-completion-report.md). The report lists commands run, exact test count, deviations from the brief with rationale, acceptance criteria status, files changed, deferred items, and a confirmation that no out-of-scope features were added. See [`reports/phase-1-completion-report.md`](./reports/phase-1-completion-report.md) for the live example.

3. **Every phase hands off via a handoff doc.** Use [`templates/handoff.md`](./templates/handoff.md). The handoff embeds the next-phase prompt verbatim, captures landmarks the receiving instance will need (commit hash, test counts, fixture surface, caches), and lists touch / don't-touch files. See [`handoffs/phase-1b-handoff.md`](./handoffs/phase-1b-handoff.md) for the live example.

4. **Every open question gets a hypothesis.** Use [`templates/hypothesis.md`](./templates/hypothesis.md). Include prerequisites (what needs to exist before testing) and expected test method.

5. **Every failed approach gets a dead-end file.** Use [`templates/dead-end.md`](./templates/dead-end.md). **Must include** "Exact conditions at failure" and "What would need to change for this to work" so future sessions know when to revisit.

6. **Every cross-phase insight gets a concept file.** Use [`templates/concept.md`](./templates/concept.md). This is the knowledge layer that transfers across phases.

7. **Every significant session updates [`RESEARCH_JOURNAL.md`](./RESEARCH_JOURNAL.md)** with a dated entry summarizing what was tried. Link to the detail files.

8. **Cross-link everything.** Experiments link to concepts. Concepts link to experiments. Issues link to concepts and experiments. Use relative paths so the links work when browsing the filesystem.

9. **Every file declares its status** in the front-matter or first paragraph: `active | complete | superseded | open | closed`.

10. **Locked inputs are locked.** [`engine-semantics.md`](./engine-semantics.md) and [`phase-1-rust-kernel-build-brief.md`](./phase-1-rust-kernel-build-brief.md) are part of the Phase 1 acceptance criterion (#7 — "unchanged"). Future briefs (Phase 1B, Phase 2…) get their own files; they do not edit the originals.

---

## Where research artifacts live

External binary references (PDFs, books, vendor docs) are in [`../research/`](../research/), not in this folder. See [`../research/README.md`](../research/README.md) for the index.
