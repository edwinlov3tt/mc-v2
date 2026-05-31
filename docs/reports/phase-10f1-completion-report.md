# Phase 10F.1 Completion Report — Push-Accuracy Default + EXP-029 Gaps

**Status:** Complete, gates green
**Date:** 2026-05-31
**Branch:** `phase-10f.1/push-correctness` (cut from `main` @ `2ba6ed7`)
**ADR:** [ADR-0035](../decisions/0035-phase-10f-model-simulate.md) Amendments 18 & 19
**Crate:** `mc-cli` only — **zero `mc-core` change**, **zero new dependencies**

---

## What this patch does (and why it matters)

claw-core ran the shipped `simulate` on their real `exp028_bets.parquet` and
it **caught a 38% overstatement in their own published numbers**: their
`won`-0/1 column scored integer-line pushes (`actual_total == line`) as wins,
and their UNDER-heavy model compounded those phantom wins all season. simulate
worked exactly as designed — but push-accuracy was opt-in (`--derive-pushes`)
when the data to detect pushes was sitting right there in the file. This patch
makes **push-accuracy the default whenever it's derivable**, and closes two
EXP-029-family gaps (`--max-stake`, count-based `--window first:n`).

| 2025 V1.0 | final_bank |
|---|---|
| sequential legacy (published, pushes-as-wins) | $2,962.16 |
| **batch push-accurate (correct, new default)** | **$1,829.37 (−38%)** |

Verified end-to-end against the real file: default run now reports
`outcome_mode: "derived-pushes"`, 26 pushes, 198 win / 152 loss (the 24
phantom UNDER push-wins reclassified out of the win column).

---

## Acceptance gate (AC #35-40)

- **AC #35** — push auto-derive default when score cols present;
  `--no-derive-pushes` opts out. Precedence implemented:
  `--no-derive-pushes` > explicit `--derive-pushes a=b` > auto-derive-default
  > legacy-binary/canonical. Auto-derive only fires over a *binary* outcome
  column (a 4-state enum stays authoritative). ✓
- **AC #36** — `win_rate = wins/(wins+losses)`, pushes excluded from numerator
  and denominator (this was already correct in 10F; verified, not
  double-fixed). The escalated legacy warning explicitly flags `win_rate` as
  possibly push-inflated. ✓
- **AC #37** — escalated legacy-binary warning: states the bankroll is
  **INACCURATE** (not merely approximate), that integer-line pushes are
  mis-scored, and that for direction-skewed models the error **COMPOUNDS**. ✓
- **AC #38** — EXP-049 paired assertions: legacy $2,962.16
  (`--replay sequential --outcome-mode legacy-binary --no-derive-pushes`,
  pinned for audit fidelity with a "known-overstated" comment) **and**
  push-accurate $1,829.37 (default auto-derive, asserted as the correct
  figure). ✓
- **AC #39** — `--max-stake <amount>` absolute-dollar cap, applied after the
  sizing rule and the fractional `cap=`: `stake = min(sized, cap×bankroll,
  max_stake)`. Distinct from `cap=` (a fraction). ✓
- **AC #40** — `--window first:<n>` confirmed count-based (first N placed bets
  chronologically, post-`--filter`); was already count-based from 10F, now
  has an explicit test. ✓
- **No regression** — the 30 existing simulate tests + the full workspace
  suite still pass; 6 new simulate tests added (36 total). ✓
- **Zero mc-core change; zero new deps; no float `==`** — the push test is
  `(actual - line).abs() < 1e-9` (NOT bare `==`; §3.1). ✓
- **Cookbook updated** — push-accuracy-by-default + `--no-derive-pushes` +
  `--max-stake` + count-based window + the schema-specific column note. ✓

---

## The float-`==` trap (claw-core's review catch)

The ADR prose says "actual == line" conceptually. The **implementation is
`(actual - line).abs() < 1e-9`** — bare `==` on f64 is forbidden by §3.1 and
clippy-flagged. This is exactly correct for the domain: integer lines (9.0)
can push, half-integer lines (8.5) can never push (no game scores 8.5 runs),
so the epsilon has **zero false positives**. The push-equality test in
`normalize_outcome` (the `DerivedPushes` branch) and the new
`can_derive_pushes` detection both use the epsilon.

---

## Schema-specific auto-derive (documented for the next consumer)

Auto-derive keys off the canonical `actual_total` / `line` column names
(claw-core's schema). A file with different names (NBA `final_total` /
`closing_line`) will **not** auto-derive — it falls to legacy-binary + the
escalated warning, which is correct (warn, don't silently mis-score). Those
consumers pass `--derive-pushes <actual>=<line>` explicitly. The cookbook
calls this out so the next consumer understands why they didn't get the
default claw-core got.

---

## Build gate results

```
cargo fmt --check --all                              ✓ (exit 0)
cargo clippy --all-targets --workspace -- -D warnings ✓ CLEAN
cargo build --release --workspace                    ✓ (no warnings)
cargo test --workspace                               ✓ 1310 passed; 0 failed (94 suites)
```

Quoted result (the final action before push, §6.7):

```
$ cargo test --workspace 2>&1 | grep -E "test result:" | awk '{p+=$4;f+=$6} END{print p" passed; "f" failed; "NR" suites"}'
1310 passed; 0 failed; 94 suites

$ cargo test -p mc-cli simulate 2>&1 | grep "test result:"
test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured; 33 filtered out; finished in 0.45s
```

(No-regression: the simulate suite went 30 → 36 tests; the full workspace is
0-failed. The 10F report's "363" figure was a partial count; the authoritative
release-mode full run — unit + integration + doctests — is 1310/0.)

---

## EXP-049: both numbers

| | command | final_bank |
|---|---|---|
| **Legacy (published, overstated)** | `--replay sequential --outcome-mode legacy-binary --no-derive-pushes` | $2,962.16 (222 wins incl. 24 phantom push-wins) |
| **Push-accurate (correct, default)** | *(no outcome flags — auto-derives)* | $1,829.37 (198 win / 152 loss / 26 push) |

Both pinned in `t_exp049_reproduction_legacy_and_push_accurate`, with a comment
saying which is which and that the legacy figure is reproduced for audit
fidelity, not correctness.

---

## Files touched

- `crates/mc-cli/src/simulate_reader.rs` — `can_derive_pushes` helper;
  restructured effective-mode decision (auto-derive precedence); escalated
  legacy warning; `read_records` signature (`no_derive` replaces the unused
  hint param).
- `crates/mc-cli/src/simulate.rs` — `replay` threads `max_stake`; per-bet
  absolute-dollar clamp.
- `crates/mc-cli/src/simulate_metrics.rs` — `replay_indices` /
  `run_monte_carlo` thread `max_stake`.
- `crates/mc-cli/src/simulate_command.rs` — `--no-derive-pushes` +
  `--max-stake` parse, struct fields, call-site threading, run_config JSON,
  help text.
- `crates/mc-cli/src/simulate_tests.rs` — EXP-049 paired assertion + 6 new
  tests.
- `docs/specs/metrics-cookbook.md` — push-accuracy default, the two required
  notes, `--max-stake`, count-based window.

---

## Effort vs estimate

Estimate: 1 session / ~70 LOC + tests. Actual: ~120 LOC implementation +
~110 LOC tests, one session. On target — the reader/sizing/replay structure
from 10F made every change a small, localized edit.

---

## Recommended next phase

**Phase 10C `mc model backtest`** — parameter-sweep × holdout, claw-core's
confirmed #2 ask. Composes simulate (each sweep cell is a replay) and reuses
this phase's reader + sizing vocabulary + push-accurate default.
