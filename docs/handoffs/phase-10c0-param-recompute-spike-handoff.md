# Phase 10C.0 Handoff — Param-Recompute Spike (gates all of 10C)

**Status:** Accepted, ready to start — THIS IS A SPIKE, not the command
**Date:** 2026-05-27
**ADR:** [ADR-0036](../decisions/0036-phase-10c-model-backtest.md) Amendment 1 (binding)
**Estimated effort:** 0.5–1 session (it's one experiment + a written verdict)
**Crate:** investigation across `mc-core` + `mc-cli`; the FIX (if needed) is `mc-core`
**Branch:** `phase-10c.0/param-recompute-spike`

---

## Why this exists (read first)

ADR-0036 (`mc model backtest`) was reviewed by Codex and Claude Code, both
with codebase access. They confirmed against source:

- **No `param(name)` setter exists** — `sweep.rs` overrides only
  coefficients + set-coord cells; nothing sets a `parameters:` scalar.
- **Parameters are explicitly OUTSIDE dirty propagation** — `cube.rs:3069`:
  *"no dependency-graph participation (constants don't participate in
  dirty propagation)."*
- **Snapshot doesn't cover reference_data** — `snapshot.rs` clones only
  the cell store; `rollback_to` won't reset a param.

backtest's PRIMARY axis is `param:` sweep. If overriding a param doesn't
make its dependent derived cells recompute, then backtest's headline
mechanism is broken — and fixing it is `mc-core` work, which breaks the
"zero kernel change, ~400 LOC composition, 3-4 sessions" plan.

**This spike answers exactly one question:** when you change a
`param(name)` value and re-read a derived measure that depends on it,
**does the value move, or does it serve stale cache?**

The answer decides whether 10C.1 is a CLI composition or a kernel change.
Do NOT build the backtest command. Build the smallest test that answers
the question, then write the verdict.

---

## The experiment

### Step 0: Worktree
```
cd /Users/edwinlovettiii/Projects/mc-v2
git pull origin main
git worktree add ../mc-v2-phase-10c0 -b phase-10c.0/param-recompute-spike main
cd ../mc-v2-phase-10c0
```

### Step 1: Build the minimal cube
A tiny inline-YAML cube (single braces — §4.5) with:
- a `parameters:` block declaring one scalar, e.g. `threshold: 0.10`
- one Input measure
- one Derived measure whose rule references `param(threshold)` — e.g.
  `should_flag = if(some_input >= param(threshold), 1.0, 0.0)` (or any
  formula where the param materially changes the output)

### Step 2: The failing-then-passing test
In a test (or a throwaway `examples/` binary — your call, test is cleaner):
1. Load + compile the cube. Read the derived measure at a coord where the
   param value matters. Record value A.
2. **Override `param(threshold)`** to a different value that should flip
   the derived result. THE PROBLEM: there may be no API to do this. Find
   out:
   - Is there a `cube.set_parameter(name, value)` or equivalent? (grep
     `parameters` in `cube.rs` for a mutator — the review says there's
     only a reader.)
   - If there's no setter, that itself is a finding (the primary axis
     needs one). For the spike, you may construct a second cube with the
     different param value (recompile from YAML with the param changed)
     to isolate the *recompute* question from the *setter* question.
3. Re-read the same derived measure (same cube if you found a setter; the
   second cube otherwise). Record value B.
4. **Assert A ≠ B** (the derived measure moved with the param).

### Step 3: The diagnostic that matters
Two distinct sub-questions — report BOTH:

**(a) Setter:** Is there an in-place `param(name)` override API on a
loaded cube? (Y/N. If N, backtest needs one built — small, but it's net
new, not "composition.")

**(b) Recompute/cache:** If you override the param in place (or once a
setter exists), does the derived cell recompute, or does it serve a
cached value from before the override? Specifically:
- After overriding the param, does reading the derived measure return the
  NEW value, or the stale one?
- If stale: what cache is holding it? (The derived-leaf cache and/or the
  consolidated cache — see `cube.rs` read path. `cube.rs:3069` says
  params don't participate in dirty propagation, so the dirty bit that
  normally busts those caches won't fire on a param change.)

### Step 4: Write the verdict
`docs/reports/phase-10c0-spike-report.md` — answer the gating question in
the first paragraph, one of three outcomes:

- **GREEN (zero-kernel-change):** param override → derived recompute works
  (setter exists or is trivial; cache busts correctly). 10C.1 proceeds as
  the planned CLI composition. AC #17 holds.
- **YELLOW (small kernel additive):** needs a `set_parameter` + a
  cache-bust-on-reference_data-mutation, but it's a contained, additive
  mc-core change (estimate it: LOC, which caches, test surface). 10C.1
  proceeds after the additive lands; AC #17 updated to "one additive
  kernel function."
- **RED (real kernel work):** params genuinely can't recompute without
  restructuring dirty propagation or the snapshot model. 10C.1 is blocked
  on a kernel ADR; re-scope (e.g. backtest restricted to `coef:`/`input:`
  axes for v1, which DO work via existing override mechanics, deferring
  `param:` until the kernel supports it).

Include: the test code, the exact source lines that explain the behavior,
and a recommended path with an estimate.

### Step 5: Gates (light — it's a spike)
`cargo test` for your new test, fmt, clippy. No full workspace gate
needed for a spike branch, but quote whatever you ran (§6.7). The
deliverable is the VERDICT REPORT, not shipped product code — if you
prototyped a setter, mark it clearly as spike code (it may or may not be
the real implementation).

---

## What NOT to do
- Do NOT build the `backtest` command. That's 10C.1, gated on this.
- Do NOT fold in the other 7 amendments (values-list, rmse, best-by-segment,
  etc.) — those are 10C.1 scope. This spike is ONLY the param-recompute
  question.
- Do NOT silently add a kernel mutator and call it done — if the answer is
  YELLOW/RED, the kernel change needs its own scoping/ADR review, not a
  drive-by add (CLAUDE.md kernel discipline). Surface it; don't ship it.

---

## Acceptance gate (the spike)
- [ ] AC-spike-1: a test demonstrates whether a `param(x)`-dependent
  derived measure moves when `param(x)` changes (the core question,
  answered with code + assertion)
- [ ] Setter question answered (Y/N + where)
- [ ] Recompute/cache question answered (recomputes / serves stale + which cache)
- [ ] Verdict report at `docs/reports/phase-10c0-spike-report.md`:
  GREEN/YELLOW/RED + recommended 10C.1 path + estimate
- [ ] If YELLOW/RED: the kernel change is scoped (LOC, caches touched,
  whether AC #17 survives), NOT shipped

---

## Cross-links
- ADR-0036 Amendment 1 (this spike's mandate): [`../decisions/0036-phase-10c-model-backtest.md`](../decisions/0036-phase-10c-model-backtest.md)
- `crates/mc-core/src/cube.rs:3069` — "constants don't participate in dirty propagation" (the suspect line)
- `crates/mc-core/src/snapshot.rs` — clones only the cell store
- `crates/mc-cli/src/sweep.rs` — the existing coef/set-coord override (what param sweep would parallel)
- CLAUDE.md §2.15 (read mutates — caching), §4.5 (single-brace YAML), §6.7

---

## Why a spike instead of just building it
Five ADRs in this track shipped clean because the design was sound before
implementation. 10C's design rests on a mechanism two source-reading
reviewers flagged as possibly-nonexistent. Building the whole command and
discovering the param axis serves stale cache at test time would waste a
3-4 session phase. The spike costs <1 session and converts an unknown
into a GREEN/YELLOW/RED decision. This is the cheapest possible way to
de-risk the one assumption the whole phase rests on.
