# Phase 6A.1 Post-Ship Gap Audit — Protocol

> **Shared protocol document for the four audit instances (A, B, C, D).**
> Read this first. Then read your lens-specific kickoff prompt.
> Then begin.

**Status:** Active 2026-05-06.
**Scope:** every gap, edge case, and missed feature in Mosaic as of `phase-6a-1-review-fixes` (`44a7437`).

---

## Why we're auditing

Mosaic shipped Phase 6A (agent-ready CLI) and Phase 6A.1 (review-driven fixes) on 2026-05-06. A real-world test with email-matchback (a Tide Cleaners marketing-attribution / forecasting / MMM project) produced ~1,260 lines of Python — of which ~840 are workarounds for Mosaic gaps.

**The problem:** every Python workaround a user writes is evidence of an engine gap. The email-matchback feedback surfaced 11 specific gaps; many of those are now closed, but we don't know what we don't know. Other models (sports betting, FP&A, demand planning) will hit different gaps. **We need a comprehensive map before we patch anything**.

**What this audit produces:** four markdown reports under `docs/audits/`. The synthesizer's `master-gap-report.md` becomes the input for sequencing Phase 3I, 5D, 6A.2, and any 3H amendments. **No code changes, no ADRs, no benchmarks.** Findings only.

---

## Hard rules (binding for all four instances)

1. **Read-only.** Do not edit any code, any spec, any ADR, any existing doc. Do not create commits. The only files you create are your single audit report at the path your kickoff prompt specifies.
2. **Cite evidence by file:line.** Every finding must reference a specific file and line number — both in `email-matchback/` (the evidence) and in `crates/` (the capability or its absence).
3. **Don't recommend fixes.** Describe the gap and its impact. The synthesizer + project owner decide what to fix and how.
4. **Distinguish three categories** in your findings:
   - **Closed by 6A/6A.1** — gap existed pre-6A but is now addressed; cite the Mosaic feature that closed it.
   - **Open — clear path** — gap is real and the fix is well-defined (e.g., "add an `extrapolate_last_value` rule").
   - **Open — needs design** — gap is real but the fix is non-obvious or has multiple shapes (e.g., "indicator role" could be a measure-role enum extension OR a new `indicators:` block OR a synthesized derived measure).
5. **Include "should-be-engine but no current Python evidence" findings.** If a planning/forecasting/analytics use case CAN'T be expressed in Mosaic today and the email-matchback scripts didn't happen to surface it — flag it. Don't constrain to what Tide Cleaners hit.
6. **Don't rank within your report.** Just describe. Cross-cutting prioritization is the synthesizer's job.
7. **No "this is fine" findings.** If something works well, don't write it up. Negative space only.

---

## Output format (every audit report uses this shape)

```markdown
# <Lens> Audit — Phase 6A.1 Gap Analysis

## Reviewer: Claude Sonnet 4.6
## Date: 2026-05-06
## Scope: <what you read>

---

## Closed by 6A/6A.1 (verification)

For each previously-known gap that 6A or 6A.1 closed, confirm:

### G-CLOSED-N: <one-line summary>
**Was:** <Python workaround pattern with file:line evidence>
**Now:** <Mosaic feature that replaces it, with file:line>
**Evidence:** <run a command if cheap; cite test if applicable>

---

## Open gaps — clear path

For each gap with an obvious fix shape:

### G-OPEN-N: <one-line summary>
**Use case:** <what the user is trying to do>
**Evidence (Python):** <file:line in email-matchback or "no current evidence; theoretical">
**Evidence (Mosaic absence):** <file:line of where it would land + what's missing>
**Impact:**
  - Lines of Python eliminated: <estimate>
  - Other affected use cases: <list>
**Proposed fix shape:** <one paragraph, NOT a design — just "this is the obvious approach">
**Phase mapping:** <existing phase placeholder OR "needs new phase">

---

## Open gaps — needs design

For each gap where the fix is real but the shape is non-obvious:

### G-DESIGN-N: <one-line summary>
**Use case:** <what>
**Evidence:** <Python and/or theoretical>
**Why design is non-obvious:** <2-3 sentences — name the alternatives>
**Alternatives:** <bullet list of 2-4 fix shapes, each with one-line tradeoff>
**Phase mapping:** <"needs ADR before phase scoping">

---

## Edge cases / latent bugs found during audit

Anything you noticed that isn't a gap but is a bug or fragility:

### E-N: <one-line summary>
**Where:** <file:line>
**What I expected:** ...
**What I observed:** ...
**Impact:** ...

---

## Confirmed working (sanity checks only)

Brief list — one line each — of things you specifically verified work.

---

## What I couldn't verify

Anything you flagged but couldn't confirm without running code or accessing data you don't have.
```

---

## Where to look in `email-matchback/`

```
~/Projects/email-matchback/
├── scripts/mosaic/          ← Python workarounds (THE EVIDENCE)
│   ├── flatten_ltd_comparison.py    # 220 lines: year-blocked Excel grid → long CSV
│   ├── build_ltv_cohort.py          # 200 lines: customer rows → cohort aggregates
│   ├── prepare_v2_inputs.py         # 170 lines: rename/mirror/extend hacks
│   ├── prepare_mmm_inputs.py        # 80 lines: 464 indicator rows
│   ├── fit_mmm.py                   # 240 lines: Lasso fit (correctly Python per ADR-0015)
│   ├── bench.py                     # uses goldens-as-probes (closed by 6A?)
│   ├── whatif_v2.py                 # uses goldens-as-probes (closed by 6A?)
│   ├── budget_reallocator.py        # uses goldens-as-probes
│   ├── ltv_report.py                # uses goldens-as-probes
│   └── whatif_demo.py
├── models/                  ← The actual Mosaic YAML cubes
│   ├── tide-matchback.yaml + .inputs.csv
│   ├── tide-ltv-cohort.yaml + .inputs.csv
│   └── tide-mmm.yaml + .inputs.csv
└── data/                    ← Raw input CSVs
    ├── ltd-comparison-long.csv
    ├── mmm-inputs.csv
    └── ...
```

---

## Where to look in `mc-v2/` (your capability survey)

```
crates/
├── mc-core/         (calculation engine; locked surface)
├── mc-fixtures/     (Acme demo cube; locked since 1A)
├── mc-model/        (YAML schema, formula parser, validator, lint, diagnostics)
│   ├── src/formula.rs      ← formula AST + parser
│   ├── src/validate.rs     ← MC2xxx validation
│   ├── src/lint.rs         ← MC3xxx lint rules
│   └── src/compile.rs      ← compile to Cube
├── mc-cli/          (14 verbs + MCP server)
├── mc-recipe/       (Tessera recipe schema)
├── mc-drivers/      (11 source drivers)
└── mc-tessera/      (orchestrator + schedule daemon)

docs/
├── decisions/       (ADRs 0001–0014)
├── specs/           (engine-semantics + Phase 1 brief; locked since 1A)
└── research-notes/  (formula-language-expansion.md, cross-coord-dep-graph.md)
```

---

## How to run cheap verification

If a gap is "I think Mosaic can't do X":

1. Try it: write a 5-line YAML, run `mc model validate`, see what happens.
2. If it errors, paste the error code in your report.
3. If it succeeds, run `mc model test` or `mc model query` to see if it actually evaluates as expected.
4. Don't author large fixtures — use `crates/mc-model/examples/acme.yaml` as the starting shape.

---

## Synthesizer's reading order (D only)

Read in this order:
1. `data-in-audit.md`
2. `calculation-audit.md`
3. `data-out-audit.md`
4. Spot-check 5 random findings from each by running the cited file:line.

Then produce `master-gap-report.md` with:
- **Section 1 — Master gap inventory** (deduplicated, all G- entries from all three reports renumbered as M-N).
- **Section 2 — Cross-cutting patterns** (gaps that show up in multiple lenses; these are usually the highest-leverage fixes).
- **Section 3 — Phase mapping** (each gap → existing phase placeholder OR new phase recommendation).
- **Section 4 — Sequencing recommendation** (if you fix nothing else, fix these N — with rationale; assume the project owner can deliver ~3 phases of capacity).
- **Section 5 — Findings the audits missed** (your own scan; what didn't make it into A/B/C that should have).

---

*End of protocol.*
