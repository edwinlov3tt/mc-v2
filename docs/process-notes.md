# Process Notes

> **Carry-forward decisions about HOW the project runs**, separate from WHAT it builds. Operational rules that don't fit cleanly in CLAUDE.md (which is the kernel-implementation manual) or in ADRs (which are scope/architecture decisions). This file captures process-level conventions that future phases should inherit.

**Last updated:** 2026-05-06 (post-Phase 6A.2; Rule 11 added codifying git workflow — when to branch, when to worktree, when to commit on main)

---

## Process rules

### 1. ADR-first vs handoff-first flow — when to use which

The standard flow for shipping a phase is:

```
ADR-first (default for larger phases):

  1. Project owner: "Phase XX direction is [...]"
  2. PM/spec-maintainer drafts ADR (Proposed)
  3. GPT + Claude Desktop review; project owner amends
  4. ADR Accepted
  5. PM drafts handoff (binding contract for implementer)
  6. Project owner reviews handoff
  7. Kickoff prompt → implementer
  8. Metadata updates land alongside ADR Acceptance
  9. Implementer DONE → review → commit + tag
```

Phase 3D introduced an alternative:

```
Handoff-first parallel flow (small phases only):

  1. Project owner: "Phase XX direction is [...]"
  2. PM drafts handoff DIRECTLY (the binding contract)
  3. Quick project-owner check on handoff direction
  4. Kickoff prompt → implementer ← they start NOW
  5. ── IN PARALLEL ──
     5a. PM drafts ADR (Proposed) + GPT/Desktop review
     5b. Implementer codes
  6. ADR Accepted + metadata updates land independently
  7. Implementer DONE → review → commit + tag
```

**The carry-forward rule (binding):**

| Phase scope | Use which flow? |
|---|---|
| **Small, well-implied by prior ADRs** (e.g., Phase 3D — the structured-tree AST already exists; formula syntax is just authoring ergonomics over it) | **Handoff-first parallel flow OK** |
| **Large, novel scope** (e.g., Phase 4 LLM authoring — new dependency on LLM provider, prompt scaffolding, iteration-loop semantics, error-feedback contract; or Phase 5 actuals — new external data sources, schema versioning concerns, lineage requirements) | **ADR-first required** — return to the standard flow |
| **Anything that adds a new crate, a new dep, or modifies the kernel** | **ADR-first required** |
| **Anything that changes a contract surface** (Diagnostic shape, schema_version bump, public API of mc-core or mc-fixtures, kernel semantics) | **ADR-first required** — Phase 3D was an additive parser layer with no contract changes |

**Why the distinction matters:**

- ADR-first forces strategic alignment BEFORE the implementer is committed. For a large phase, surfacing a direction question mid-implementation costs the implementer's time + creates SPEC QUESTION churn.
- Handoff-first works for small phases because the strategic decisions are derivable from prior ADRs; the implementer can absorb a late-arriving ADR refinement via SPEC QUESTION cheaply since they're early in the work.
- The ADR is still required either way — just the SEQUENCING differs. Handoff-first means the ADR lands in parallel (or after) the implementation; ADR-first means the ADR lands before kickoff.

**When in doubt, default to ADR-first.** Phase 3D is the proof of concept for handoff-first; Phase 4 is the test that proves the rule scales correctly.

**Self-test before picking handoff-first:**

1. Does the new phase introduce a kernel change? → No → handoff-first OK.
2. Does the new phase add a runtime dep to any crate? → No → handoff-first OK.
3. Does the new phase change a contract shape (Diagnostic struct, schema_version bump, public API of mc-core/mc-fixtures)? → No → handoff-first OK.
4. Is the scope < ~1500 lines of code added across all crates? → Yes → handoff-first OK.
5. Are the strategic decisions derivable from prior ADRs? → Yes → handoff-first OK.

If all 5 are "yes," handoff-first is appropriate. If any is "no," default to ADR-first.

### 2. Acceptance amendment audit trail

When a Proposed ADR is reviewed (typically by GPT and Claude Desktop) and the project owner approves with amendments, the amendments land in the ADR's "Acceptance amendments" section as a numbered table. This is the same shape across ADR-0004 / 0005 / 0006 / 0007.

**Amendment numbering convention:**
- GPT-sourced amendments: numbered 1–N (or labeled "GPT N").
- Desktop-sourced amendments: numbered 11+ to avoid collision (the convention started with ADR-0004 and is consistent through ADR-0007).
- Mid-flight execution notes (e.g., "GPT execution note #3"): numbered 27+ in ADR-0007 — extends the same numbering rather than re-using the Desktop range.

**Why audit-trail matters:** the amendment table is the project's institutional memory of "what changed at acceptance and why." Future readers (humans + LLMs) can reconstruct the decision rationale without spelunking commit history. The amendment numbers are stable across ADR revisions.

### 3. Diagnostic-code retirement is forever

CVE-style. Once a code is shipped (validation MC2xxx, lint MC3xxx, parse MC1xxx), its meaning is locked. If the rule it represents is removed or repurposed, the code stays *retired* — never reused for a different rule.

Established by ADR-0005 amendment #11 (MC3008's permanent retirement after promotion to MC2011). Carry-forward through ADR-0006 (no retirements; MC2025 was repurposed PRE-acceptance, which is the only window for repurposing — once shipped, locked) and ADR-0007 (no retirements; deliberately did NOT introduce MC1007 to keep the option for tighter codes later).

**Implementation requirement:** every active phase ships an assertion test that no active validator/lint emits a retired code. Phase 3B established this for MC3008; Phase 3D will add a check that no formula-related diagnostic emits MC1007 (reserved-for-future).

### 4. Locked surfaces are forever (until explicit ADR unlocks them)

`mc-core` has been locked since Phase 2D. `mc-fixtures` has been locked since Phase 1A. The locks are enforced by:

- Hard-rule statements in every handoff after Phase 2D.
- `git diff <previous-tag> -- crates/mc-core/ crates/mc-fixtures/` returns 0 lines as a success-gate item in every Phase 3 completion.
- Any phase that needs to unlock either crate requires an explicit ADR documenting why.

The lock is the single most valuable property of the Phase 3 sub-phases. Every phase's handoff explicitly carries the lock forward; if a phase can't honor it, the phase scope changes.

### 5. Hand-rolled wins over deps

The project's "minimum dep churn" pattern is consistent across phases:

- Phase 1A: only `smallvec`, `ahash`, `thiserror`, `once_cell` in `mc-core`.
- Phase 1B: criterion added to dev-deps via three Cargo.lock pins (clap/clap_lex/half) to avoid a Rust 1.85 toolchain bump.
- Phase 3A: `serde_yaml` 0.9.34 added to mc-model with `indexmap → 2.7.0` transitive pin.
- Phase 3B: snapshot tests hand-rolled (no `insta`).
- Phase 3B: JSON serialization hand-rolled (no `serde_json`).
- Phase 3C: CSV parser hand-rolled (no `csv` crate); strict subset only.
- Phase 3D: formula parser hand-rolled (no `pest`/`nom`/`lalrpop`); recursive descent.

**The rule:** every dep adds transitive deps + toolchain risk + build time + version-bump churn. The hand-rolled equivalent is usually < 100 lines and provides exactly the surface needed without surprises. Default to hand-rolled; only pull in a dep when you can prove the hand-rolled version would be substantially worse.

### 6. Snapshot tests — manual diff before regenerating

Per GPT execution note #4 (Phase 3D): when snapshot test output drifts, do NOT use `MC_SNAPSHOT_UPDATE` (or equivalent) blindly to regenerate. Manually diff the new output first; the new output should be EASIER TO READ, not just different.

If a snapshot regeneration produces uglier output, that's a signal the underlying code changed in a way the snapshot author didn't intend. Investigate before committing the new snapshot.

This applies to all four kinds of snapshots in the project: text-format diagnostic output, JSON envelope output, `mc model inspect` output, and any future snapshot tests.

### 7. Backwards compat is a hard gate, not a nice-to-have

Phase 3A's structured-tree YAMLs work after Phase 3D's formula syntax addition. Phase 3B's lint fixtures still parse after Phase 3C's `canonical_inputs:` schema addition. Phase 3C's `Diagnostic` shape stays unchanged after Phase 3D adds new codes.

Each phase that adds a schema/diagnostic change MUST include a backwards-compat test asserting prior-phase fixtures still load identically. The pattern is:

- Phase 3C: structural-equivalence test against `build_acme_cube()` from Phase 1A.
- Phase 3D: backwards-compat test loading a Phase 3C structured-form fixture.
- Future phases: same shape — load a fixture from a prior phase and assert structural identity (modulo intentional schema additions).

### 8. Process changes belong in this file

When a new process rule emerges (e.g., handoff-first parallel flow), it lands here, NOT in CLAUDE.md (which is for kernel-implementation guidance) and NOT in ADRs (which are scope/architecture decisions). This file is the carry-forward index of operational conventions.

Future readers should be able to start from `CLAUDE.md` (kernel rules) → `MASTER_PHASE_PLAN.md` (what's been built and what's next) → `CURRENT_STATE.md` (current snapshot) → this file (operational conventions) → ADRs (specific decisions) and reconstruct the full operating model.

### 9. Four-source cube state model (write-log replay rule)

Cube state has four sources of cell values:

```
Cube state = compile(YAML)
           + apply(canonical_inputs from YAML)
           + apply(Tessera imports from .tessera/audit.jsonl)
           + apply(post-hoc writes from .tessera/writes.jsonl)
```

CLI verbs split into two categories based on which sources they replay:

**Replay all four sources (show current operational reality):**
- `mc model query` — shows what the cube currently contains
- `mc model whatif` — hypothetical over current state
- `mc model trace` — explains current computed values
- `mc model query --output` — exports current state to file
- `mc model diff` (current side) — compares current vs. prior

**Replay only the first three (verify reproducibility / experiment with hypotheticals):**
- `mc model test` — verifies the version-controlled model is correct
- `mc model sweep` — experiments with parameter values in pristine state
- `mc tessera apply` — clean import without post-hoc patch interference

**The rule:** post-hoc writes (`.tessera/writes.jsonl`) are real-time patches applied outside the version-controlled model definition. They represent "a line just moved" or "an agent patched one cell" — operational reality that isn't part of the reproducible model. Tests verify the model; queries show reality. Future verbs inherit this rule by asking: "does this verb show current reality or verify reproducibility?"

### 10. Completion report self-audit pattern

Completion reports SHOULD include an explicit **"what I would have done with more time / known debt"** section. This surfaces technical debt that would otherwise be invisible to future maintainers.

The pattern (established by the Phase 6A implementer):
- Name the debt explicitly (what's unfinished or fragile)
- Distinguish bugs from design tensions
- Prioritize the fixes (P0/P1/P2)
- Surface trade-offs made deliberately ("took the simpler but slower path")

This is the same discipline as SPEC QUESTIONs (surface ambiguity rather than guessing), applied to completion reports. The SHIPPED list is the achievement; the DEBT list is the maintenance roadmap. Both belong in the report.

### 11. Git workflow — when to branch, when to worktree, when to commit on main

**The rule (binding decision matrix):**

| Work shape | Branch? | Worktree? | Pattern |
|---|---|---|---|
| **Quick fix** (1 file, <30 min, no contract change — typo, single-line bug fix, doc update, README change) | No | No | Commit directly on `main`. Round-trip overhead exceeds the work. |
| **Phase work, single instance, sequential** (any phase under MASTER_PHASE_PLAN.md handled by one agent at a time — the default for Phase 3+ sub-phases and all `.x` follow-ups) | **Yes** (`phase-X.Y/short-name`) | No | Branch on main. Single instance works on the branch. PM merges + tags + pushes when done. Branch deleted after merge. |
| **Parallel work** (2+ instances on independent scopes simultaneously — e.g., the Phase 5A Stream A/B/C/D split) | Yes (one per instance) | **Yes** (one per instance) | Worktrees prevent working-directory conflicts. Each instance commits to its branch; PM merges them in dependency order. |
| **Risky work** (multi-crate, contract-touching, kernel-adjacent) | Yes | Optional | Branch even if single-instance. Makes rollback trivial if review surfaces issues. |
| **Doc-only update** (CURRENT_STATE, MASTER_PHASE_PLAN, README, completion-report-only) | No | No | Treat as quick fix; commit on main. |
| **Cleanup** (deleting stale branches, removing worktrees, etc.) | No | No | Same as quick fix. |

**The PM's decision rule (load-bearing):** the PM decides which path to use BEFORE the implementer starts. Communicate the choice in the kickoff prompt. Don't switch mid-flight (an instance that started on a worktree should not silently move to main, and vice versa).

**Why both extremes are wrong:**
- *Always-branch* costs round-trip overhead on every typo fix. The PM's commits on main get blocked behind branch hygiene that buys nothing.
- *Always-main* loses the rollback safety net for risky multi-crate work and breaks parallel scenarios entirely.
- *Always-worktree* (the Phase 5A pattern that bled into 6A.1 + 6A.2) creates orphan worktrees and stale branches when the work is sequential. Worktrees are for parallelism, not for "we're going to branch anyway."

**Rule for "instance needs docs" (the load-bearing fix to the recurring problem):**

Write the handoff/audit/spec docs the instance needs on the *same artifact* they will work on:

- Instance will work on `main` (quick fix or doc-only) → commit docs to main FIRST, then start the instance.
- Instance will work on a branch → create the branch from main HEAD, then write the docs ON THE BRANCH (or write them on main first and create the branch immediately after — both work; pick one and be consistent within a phase).
- **Never split work across "docs on main, code on branch."** The instance starts on the branch, doesn't see the docs that landed on main after the branch was cut, and either guesses or fails.

The audit-pattern variant: when running parallel audit instances (e.g., the Phase 6A.2 pre-audit with A/B/C/D lenses), put the AUDIT-PROTOCOL.md and any shared context on main BEFORE creating any worktrees, so each instance's worktree inherits the protocol from the same baseline.

**After a phase ships:**

1. Tag main at the merge commit (or at the direct commit if no branch).
2. Delete the local branch: `git branch -d phase-X.Y/short-name`.
3. Delete the remote branch (if it was pushed): `git push origin :phase-X.Y/short-name`.
4. Remove any worktrees for the phase: `git worktree remove ../mc-v2-X` (or `git worktree prune` if the directory is already gone).
5. Confirm `git worktree list` shows only the main worktree before declaring cleanup done.

**Stale-branch sweep:** every ~5 phases, run `git branch | grep phase-` and verify each surviving branch is either (a) actively in flight or (b) deliberately preserved for some reason. Delete the rest. The tag history is the canonical record; branches are scaffolding.

**Anti-patterns observed and to avoid:**

- Creating a worktree "just in case" when the work is sequential. (Phase 6A.2 — the worktree was created and never used; the implementer worked on main directly. Both ended up working, but the worktree was wasted setup.)
- Writing handoff/review docs on main while the instance is on a branch. (Recurring failure mode — the instance's session lacks the doc context.)
- Letting old phase branches accumulate after merge + tag. (As of 2026-05-06: 8 stale branches existed for shipped phases. Tags preserve history; branches were just clutter.)
- Pushing branches before the work is reviewed. (Branches on origin become a contract someone might fork from; only push when the work is reviewed and ready.)

**The PM's git workflow self-test (run before kickoff):**

1. Will more than one agent work on this simultaneously? → If yes, worktrees. If no, no worktrees.
2. Is this trivial enough that a single direct-to-main commit is the right shape? → If yes, no branch. If no, branch.
3. Has the instance been told explicitly which directory to `cd` into and which branch to expect? → If no, fix the kickoff prompt.
4. Are all docs the instance needs reachable from their starting commit? → If no, commit them first OR create the branch later.

If all four are clear, proceed. If any is ambiguous, resolve it in the kickoff before the instance starts.

---

## Open process questions

(Revisit periodically; these aren't binding rules yet.)

1. **Should the "Acceptance amendments" table include the project owner's voice explicitly, alongside GPT and Desktop?** Today the project owner's role is implicit (they authorize amendments). Could surface them as a separate column.
2. **Should there be a "Phase health check" rhythm — every N phases, audit whether the locked surfaces, the diagnostic-code namespace, and the hand-rolled-wins rule are still appropriate?** Today these are inherited per-phase; a periodic step-back might catch drift.
3. **Should the "for-dummies" notes (`docs/for-dummies/phases/`) become required deliverables?** Today they're optional follow-ups; making them required would catch user-facing-explanation gaps before they ossify.

These don't have answers today; they're flagged so the project owner notices them when relevant.
