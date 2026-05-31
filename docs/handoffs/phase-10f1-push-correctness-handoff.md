# Phase 10F.1 Handoff — Push-Accuracy Default + EXP-029 Gap Fixes

**Status:** Accepted, ready to start
**Date:** 2026-05-27
**ADR:** [ADR-0035](../decisions/0035-phase-10f-model-simulate.md) — Amendments 18 & 19 (binding); the 10F body + Amdts 1-17 already shipped at `7462c22`
**Estimated effort:** 1 session (~70 LOC + tests across files you already wrote)
**Crate:** `mc-cli` ONLY (extends the shipped simulate; zero mc-core)
**Branch:** `phase-10f.1/push-correctness`
**Best run by:** the Phase 10F instance (the `simulate_reader.rs` outcome-mode logic + `simulate_command.rs` flag parsing are in your context). If a fresh instance: read `crates/mc-cli/src/simulate_reader.rs` (esp. `normalize_outcome`, `detect_outcome_kind`, the effective-mode decision at ~line 419) + `simulate_command.rs` flag parsing first.

---

## Why this patch exists (read this — it's the whole point)

claw-core ran your shipped `simulate` on their real `exp028_bets.parquet`
(a `won`-0/1 file → `legacy-binary` mode) and `--derive-pushes` surfaced
a **38% overstatement in their published numbers**. Integer-line games
landing exactly on the line (`actual_total == line`) are pushes — stake
returned, neutral. Their `won` column scored those as WINS for UNDER bets
(24 of 26 in 2025). The model is UNDER-heavy, so phantom-push-wins
compounded all season:

| 2025 V1.0 | final_bank |
|---|---|
| sequential legacy (their published headline) | $2,962 |
| batch push-accurate (correct) | **$1,829 (−38%)** |

The push correction is **~500× larger** than the batch-vs-sequential
correction (A1/A17) we spent so much care on. simulate is working
exactly as designed — it CAUGHT the bug. But the default made the user
opt INTO correctness (`--derive-pushes`) instead of getting it by
default, even though the data to detect pushes was right there in the
file. That's the silently-wrong-default footgun.

This patch makes push-accuracy the default whenever it's derivable, and
closes two small EXP-029-family gaps claw-core flagged.

---

## What this patch ships (Amendments 18 & 19)

| # | Item | Amendment |
|---|---|---|
| 1 | Auto-derive pushes when score columns present (default); `--no-derive-pushes` to opt out | A18 |
| 2 | `win_rate = wins/(wins+losses)` — pushes excluded from num AND denom; JSON caveat under legacy-binary-without-pushes | A18 |
| 3 | Escalate the legacy-binary-without-derivable-pushes warning (inaccurate, not just approximate; names the compounding risk) | A18 |
| 4 | EXP-049 push-accurate paired assertion (~$1,829) alongside the legacy $2,962 | A18 |
| 5 | `--max-stake <amount>` absolute-dollar cap (EXP-029d) | A19 |
| 6 | `--window first:<n>` confirmed count-based (EXP-029e) | A19 |

---

## Implementation path

### Step 0: Worktree (your 10F worktree was removed at merge)
```
cd /Users/edwinlovettiii/Projects/mc-v2
git pull origin main   # ensure you have 7462c22 + the Amdt 18/19 ADR (5fdc259+)
git worktree add ../mc-v2-phase-10f1 -b phase-10f.1/push-correctness main
cd ../mc-v2-phase-10f1
```

### Step 1: Auto-derive pushes (A18 — the core fix)
**File:** `crates/mc-cli/src/simulate_reader.rs`, the effective-mode
decision (~line 419, the `if derive.is_some() { ... } else { match
detect_outcome_kind(...) }` block).

New logic:
- Detect whether push-derivation is POSSIBLE: are the score columns
  present? (default pair `actual_total` + `line`; or whatever
  `--derive-pushes a=b` named). Add a helper
  `can_derive_pushes(&table.columns) -> Option<(actual_col, line_col)>`
  that checks for the default pair when no explicit `--derive-pushes` was
  given.
- **New default behavior:** if push-derivation is possible AND
  `--no-derive-pushes` was NOT passed → `OutcomeMode::DerivedPushes`
  (using the detected or explicit column pair), regardless of
  `--outcome-mode`. This means a binary `won` file WITH `actual_total` +
  `line` columns now gets push-accurate scoring by default.
- `--no-derive-pushes` forces the prior behavior (binary → legacy-binary
  if requested, else the canonical hard-error).
- Explicit `--derive-pushes a=b` still names non-default column pairs.
- Precedence: `--no-derive-pushes` > explicit `--derive-pushes a=b` >
  auto-derive-default > legacy-binary/canonical.

Add the `--no-derive-pushes` flag in `simulate_command.rs` (boolean, no
value).

### Step 2: win_rate excludes pushes (A18)
**File:** wherever `win_rate` is computed (`simulate_metrics.rs`). Ensure
`win_rate = wins / (wins + losses)` — pushes NOT in numerator or
denominator. When the effective mode is legacy-binary AND pushes were not
derivable (so undetected pushes may be hiding in the wins), add a caveat
string to the JSON `warnings` and (if there's a per-metric note channel)
flag `win_rate` as possibly push-inflated.

### Step 3: Escalate legacy-binary warning (A18)
**File:** `simulate_reader.rs` ~line 455 (the existing legacy-binary
warning). Strengthen it: state the bankroll is **inaccurate, not just
approximate**, that any integer-line push is mis-scored, and that for
direction-skewed models (mostly-OVER or mostly-UNDER) the error
compounds. Keep legacy-binary available — AC #12 needs it to reproduce
the published number — but make the warning impossible to miss.

### Step 4: `--max-stake` (A19, EXP-029d)
**File:** `simulate_command.rs` (flag parse) + the sizing/stake
computation. `--max-stake <amount>` is an absolute-dollar cap applied
AFTER the sizing rule and the fractional `cap=` modifier:
`stake = min(sized_stake, fractional_cap × bankroll, max_stake)`.
Default: no cap (None).

### Step 5: `--window first:<n>` count-based (A19, EXP-029e)
**File:** the window parsing/selection. Confirm `first:<n>` selects the
first N placed bets chronologically AFTER `--filter` (count-based), as
distinct from `range:<a>:<b>` (date-based). If it's already count-based
from 10F, just add a test proving it; if it slipped to date-based, fix it.

### Step 6: EXP-049 push-accurate paired assertion (A18, AC #38)
Update the EXP-049 reproduction test:
- Keep the legacy assertion: `--replay sequential --outcome-mode
  legacy-binary --no-derive-pushes` → $2,962.1597 (the published, now
  known-wrong figure). Comment: "reproduces the historical published
  number, which overstates ~38% due to push mis-scoring."
- Add the correct assertion: default (auto-derive) or explicit
  `--derive-pushes actual_total=line` on the same 2025 window → ~$1,829,
  asserted as the CORRECT figure. Tolerance per A15 (0.1% final).

### Step 7: Tests
- `--no-derive-pushes` opt-out works (binary file → legacy-binary as before)
- Auto-derive: binary `won` file + `actual_total`/`line` cols → pushes
  derived by default, push count > 0, bankroll differs from legacy
- win_rate excludes pushes (a fixture with known pushes → denominator
  excludes them)
- legacy-binary-without-score-columns → escalated warning present
- `--max-stake`: a run where the cap binds → stakes clamped to the dollar amount
- `--window first:5` → exactly 5 bets, chronological, post-filter
- EXP-049 both assertions (legacy $2,962 + push-accurate $1,829)

### Step 8: Cookbook + gates
Update the `mc model simulate` cookbook section: document push
auto-derivation as the default, `--no-derive-pushes`, `--max-stake`,
count-based `--window first`. Note the push-accuracy guidance ("if your
records carry score + line columns, you get push-accurate bankroll by
default; legacy-binary is for reproducing historical published numbers").

All gates (CLAUDE.md §6) — and per §6.7, run `cargo test --workspace` as
the LAST action and quote the real result line. (And the 10B.1 lesson:
don't commit/push off an unconfirmed gate run; re-run after any fix.)

---

## Acceptance gate (binding — Amdts 18-19 → AC #35-40)

- [ ] AC #35: push auto-derive default when score cols present; `--no-derive-pushes` opts out
- [ ] AC #36: win_rate excludes pushes (num + denom); legacy-without-pushes JSON caveat
- [ ] AC #37: escalated legacy-binary warning (inaccurate, names compounding risk)
- [ ] AC #38: EXP-049 paired assertions — legacy $2,962 (--no-derive-pushes) + push-accurate ~$1,829
- [ ] AC #39: `--max-stake` absolute-dollar cap (EXP-029d)
- [ ] AC #40: `--window first:<n>` count-based, post-filter (EXP-029e)
- [ ] Existing 30 simulate tests + 363 workspace tests still pass (no regression)
- [ ] Build gates: fmt, clippy -D warnings, build, **`cargo test --workspace` quoted result line (§6.7)**
- [ ] Zero mc-core change; zero new deps; no float `==`
- [ ] Cookbook updated

---

## Common pitfalls

1. **Making auto-derive too aggressive.** Only auto-derive when the score
   columns are ACTUALLY present. A file without `actual_total`/`line`
   can't derive pushes — that path still needs legacy-binary (with the
   escalated warning) or the canonical hard-error. Don't error a file
   just because it can't derive.
2. **Breaking AC #12's legacy reproduction.** The published $2,962 MUST
   still be reproducible via `--no-derive-pushes --outcome-mode
   legacy-binary`. Auto-derive is the new default, but the escape hatch
   that reproduces history must keep working — it's how we prove fidelity
   to claw-core's published number.
3. **win_rate regression.** If 10F already computed win_rate over
   wins/(wins+losses) it may already be correct — verify, and only add
   the push-exclusion + caveat. Don't double-fix.
4. **`--max-stake` vs `cap=`.** `cap=` is a fraction of bankroll
   (existing); `--max-stake` is absolute dollars (new). Both apply;
   stake = min of all three (sized, cap×bankroll, max_stake). Don't
   conflate them.
5. **Unconfirmed gate push (the 10B.1 lesson).** Run the full gate as the
   last thing; quote the real result. Don't push off a stale green.

---

## Cross-links
- ADR-0035 Amendments 18-19: [`../decisions/0035-phase-10f-model-simulate.md`](../decisions/0035-phase-10f-model-simulate.md)
- 10F completion report: `docs/reports/phase-10f-completion-report.md`
- claw-core EXP-055 (the push-correction finding): `claw-core/docs/reports/exp-055-simulate-adoption-and-push-correction.md`
- The shipped reader: `crates/mc-cli/src/simulate_reader.rs` (`normalize_outcome`, effective-mode decision ~line 419)
- CLAUDE.md §6.7 (quoted test run), §3.1 (no float ==)

## Completion report
`docs/reports/phase-10f1-completion-report.md`: quoted test result line; the two EXP-049 numbers (legacy + push-accurate); confirmation auto-derive is the default; effort vs estimate; recommended next (Phase 10C backtest — claw-core's confirmed #2 ask).
