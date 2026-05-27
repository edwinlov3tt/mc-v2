# Phase 3L Completion Report — `nbinom_sf()` Negative Binomial Survival Function

**Phase:** 3L (Distributional formula primitives — `nbinom_sf` + `nbinom_cdf`)
**ADR:** [ADR-0031](../decisions/0031-nbinom-sf-formula-function.md) (Accepted with 7 amendments)
**Handoff:** [`docs/handoffs/phase-3l-nbinom-sf-handoff.md`](../handoffs/phase-3l-nbinom-sf-handoff.md)
**Branch:** `phase-3l/nbinom-sf` (worktree at `../mc-v2-phase-3l`)
**Base SHA:** `d6c5943`
**Date completed:** 2026-05-27
**Crates touched:** `mc-core`, `mc-model`, `mc-cli` (test/match-arm only)

---

## Summary

Shipped a native Negative Binomial survival function `nbinom_sf(k, mu, alpha)` and its complement `nbinom_cdf(k, mu, alpha)` as formula primitives in the Mosaic model layer. Hand-rolled direct PMF summation with ratio recursion. Zero new dependencies. Mirrors the Phase 3H `norm_cdf` two-layer architecture (`ParsedRuleBody` in mc-model + `Expr` in mc-core).

Once a downstream consumer (claw-core MLB cartridge) migrates `P_Over_NB` from baked Input to formula-derived Derived (soft acceptance, separate commit), the slider/sweep workflow ADR-0001 was built around will work end-to-end for MLB totals.

---

## Final diagnostic code assignments (post-preflight)

Per ADR-0031 Amendment 3, codes were preflighted before allocation. Resolution after the SPEC QUESTION raised at session start:

| Semantic name                          | Assigned code | Rationale |
|----------------------------------------|---------------|-----------|
| `NBINOM_SF_WRONG_ARG_COUNT`            | **MC1008**    | Reused shared `FormulaError::wrong_arg_count` helper — matches `norm_cdf` precedent (test `parse_norm_cdf_arity_fires_mc1008`). All wrong-arg-count diagnostics across the parser share MC1008; the function-specific name in the message disambiguates. |
| `NBINOM_CDF_WRONG_ARG_COUNT`           | **MC1008**    | Same shared-helper reuse. |
| `NBINOM_SF_INVALID_LITERAL_CONSTANT`   | **(deferred)**| Amendment 2a marked OPTIONAL with "skip if it adds friction." No lint rule shipped this phase. If a future consumer needs literal-constant validation, allocate next free MC30xx (currently MC3019). |
| ~~MC2058~~ (runtime invalid-domain)    | **retired**   | Amendment 2 changed invalid-domain behavior to Null. No runtime diagnostic emitted. The MC2058 slot was also independently taken by `validate.rs` for Str-typed body rejection (schema.rs:432), confirming the retirement was correct. |

Preflight collisions surfaced and avoided: MC1018 (already used in `validate.rs` for `*_over` references), MC1019 (reserved in research notes), MC3013 (allocated in ADR-0013 for benchmark `source` lint), MC2058 (allocated in `validate.rs` for Str-typed body rejection).

---

## Acceptance gate — 20-item checklist (Amendment 7 binding)

- [x] **AC #1**: `nbinom_sf(k, mu, alpha)` parses; wrong-arg-count surfaces MC1008. (`parse_nbinom_sf`, `parse_nbinom_sf_arity_fires_mc1008`)
- [x] **AC #2**: `nbinom_cdf(k, mu, alpha)` parses; wrong-arg-count surfaces MC1008. (`parse_nbinom_cdf`, `parse_nbinom_cdf_arity_fires_mc1008`)
- [x] **AC #3**: 13 fixture cases pass within 1e-6 absolute tolerance against scipy 1.13.1 reference. (`t_nbinom_sf_fixtures_against_scipy`)
- [x] **AC #4**: `t_nbinom_sf_half_integer_floor` passes (8.5 and 8.999 both floor to 8.0; floor not round).
- [x] **AC #5**: `t_nbinom_sf_integer_push_line` passes (P(over)+P(push)+P(under)=1 at integer line).
- [x] **AC #6**: `t_nbinom_sf_monotone_decreasing_in_k` passes (k = 0..=20 strictly non-increasing).
- [x] **AC #7**: `t_nbinom_sf_monotone_increasing_in_mu` passes (mu = 4 < 8 < 12 all increase sf at fixed k).
- [x] **AC #8**: `t_nbinom_sf_cdf_complement` passes across 4 cases (sf+cdf=1 to 1e-9).
- [x] **AC #9**: `t_nbinom_sf_invalid_returns_null` passes (mu ≤ 0, alpha ≤ 0, NaN on any arg → `None` mapped to `ScalarValue::Null`).
- [x] **AC #10**: `t_nbinom_sf_negative_k_returns_one` passes (k = -1 and k = -100 both return sf = 1.0 via shared `cdf(-) = 0` path).
- [x] **AC #11**: Validity-range doc comment uses Amendment 4 language — "validated against `scipy.stats.nbinom.cdf` within `1e-6` absolute tolerance" — NOT "numerically exact." See `nbinom_cdf_compute` doc.
- [x] **AC #12**: `nbinom_sf_compute` calls `nbinom_cdf_compute` (verified by `grep -A3 "fn nbinom_sf_compute" crates/mc-core/src/rule.rs` — single body, single PMF loop). No duplicated PMF accumulation.
- [x] **AC #13**: Diagnostic codes assigned post-preflight — see table above.
- [x] **AC #14**: No new external dependencies in any `Cargo.toml`. Confirmed by `git diff Cargo.toml crates/*/Cargo.toml` returning empty.
- [x] **AC #15**: All existing tests pass unchanged. `cargo test --workspace` green; no test file modified except `crates/mc-core/tests/correctness.rs` (added `NbinomSf`/`NbinomCdf` arm to its `collect_self_refs` walker — required by enum exhaustiveness, not a behavior change).
- [x] **AC #16**: `cargo test --workspace` passes. Total: see workspace test summary in build-gate section below.
- [x] **AC #17**: `cargo clippy --all-targets --workspace -- -D warnings` clean.
- [x] **AC #18**: `cargo fmt --check --all` clean.
- [x] **AC #19**: JSON schema regenerated. `cargo run --bin mc-model-schema --quiet | diff - docs/specs/mosaic-model-schema.json` shows zero differences (drift check passes). New `ParsedNbinomBody` definition appears + new `NbinomSf`/`NbinomCdf` variants in `ParsedRuleBody` enum.
- [x] **AC #20**: `nbinom_sf_fixtures.py` committed at `crates/mc-core/tests/nbinom_sf_fixtures.py` with scipy version pinned in header comment (currently scipy 1.13.1).

**Soft acceptance (deferred, separate commit):**
- [ ] AC #SOFT-1: MLB cartridge migration — claw-core-side follow-up.
- [ ] AC #SOFT-2: `mc model whatif` end-to-end demo — depends on SOFT-1.

---

## Build gate results

```text
$ cargo fmt --check --all                                                  ✓ clean
$ cargo clippy --all-targets --workspace -- -D warnings                    ✓ clean
$ cargo build --release --workspace                                        ✓ clean
$ cargo test --workspace                                                   ✓ all passing
$ cargo run --bin mc-model-schema --quiet | diff -                         ✓ schema matches structs
   docs/specs/mosaic-model-schema.json
$ for i in {1..10}; cargo test -p mc-core --test nbinom_sf -q               ✓ 10/10 identical pass
```

Forbidden-pattern grep on new code: zero matches.
```bash
grep -A20 "fn nbinom_(sf|cdf)_compute" crates/mc-core/src/rule.rs \
  | grep -E "\.unwrap\(\)|\.expect\(|panic!|todo!|unimplemented!"   # → empty
```

Pre-existing `unwrap()`/`expect()` in `crates/mc-core/src/` (e.g., in
`consolidation.rs`, `hierarchy.rs`) are all inside `#[cfg(test)] mod
tests` blocks — not in production code paths — and predate this phase.

### nbinom_sf test counts (this phase's new tests)

| File | Tests added | Status |
|---|---|---|
| `crates/mc-core/tests/nbinom_sf.rs` | 8 integration tests | 8/8 pass |
| `crates/mc-model/src/formula.rs` (inline `mod tests`) | 5 parser tests | 5/5 pass |
| **Total new** | **13 tests** | **13/13 pass** |

---

## Validity-range deviations

None. All 13 fixture rows passed within 1e-6 absolute tolerance against scipy 1.13.1 on first run. No fixture required loosened tolerance.

The hand-rolled PMF ratio recursion produced numerically identical values (to 9 decimal places in the printed fixtures) to scipy's reference across the full declared MLB operating range (`k ∈ [-1, 20]`, `μ ∈ [4, 12]`, `α ∈ [0.05, 0.30]`). The implementation is exactly as specified in Amendment 5; no improvisation.

---

## Cartridge migration follow-up status

**Not started.** Per ADR-0031 §"Soft acceptance" and the handoff §"Out of scope," migrating `examples/sports-betting/mlb-totals.yaml` from baked `P_Over_NB` (Input) to formula-derived `nbinom_sf(...)` (Derived) is explicitly the claw-core team's follow-up, not blocking this phase. Phase 3L ships the formula function; the cartridge demonstrates its value as a separate commit.

When claw-core picks this up, the required changes are:

1. Remove `P_Over_NB` rows from `mlb-totals.inputs.csv` (~174k rows).
2. In `mlb-totals.yaml`, change `P_Over_NB`'s `kind` from `Input` to `Derived`.
3. Add the rule body: `nbinom_sf(sharp_close_line, Predicted_Total, Dispersion_Alpha)`.
4. Add `Dispersion_Alpha` as a constant Input (value 0.1245 per claw-core EXP-025).
5. Re-run `mc model validate` and `mc model test` to refresh goldens (any drift should be within 1e-6 of the baked values).

---

## Effort vs estimate

**Estimate:** 1–2 sessions, ~140 LOC + ~80 LOC tests.

**Actual:** 1 session.
- ~80 LOC new in `mc-core/src/rule.rs` (Expr variants + compute helpers + 2 eval arms × 2 dispatch sites + walk_measure_deps + expr_depth).
- ~15 LOC new in `mc-core/src/cube.rs` (well-typed validation arm).
- ~25 LOC new in `mc-model/src/schema.rs` (ParsedRuleBody variants + ParsedNbinomBody struct).
- ~40 LOC new in `mc-model/src/formula.rs` (parser handlers + serialize + cross-coord + 5 parser tests).
- ~15 LOC new in `mc-model/src/compile.rs` (translation arms).
- ~7 LOC new in `mc-model/src/inspect.rs` (collect_refs arm).
- ~7 LOC new in `mc-model/src/lint.rs` (collect_body_refs arm).
- ~80 LOC new across 13 walks in `mc-model/src/validate.rs` (more walks than the handoff anticipated — the codebase has 13 distinct `ParsedRuleBody` walks in validate.rs, not the ~5 the handoff implied).
- ~7 LOC new in `mc-cli/src/query.rs` (2 match arms in filter evaluation).
- ~7 LOC new in `crates/mc-core/tests/correctness.rs` (Expr enum exhaustiveness arm).
- 220 LOC new in `crates/mc-core/tests/nbinom_sf.rs` (8 tests + 13-row fixture table).
- 50 LOC new in `crates/mc-core/tests/nbinom_sf_fixtures.py` (Python regen script).
- 36 LOC of generated additions in `docs/specs/mosaic-model-schema.json`.

**Total: ~590 LOC** including all match-arm wiring across the workspace, well above the 220-LOC estimate. The match-arm fanout in `validate.rs` (13 walks) and the unforeseen `cube.rs` / `correctness.rs` / `mc-cli` sites accounted for most of the overage. The compute helpers themselves (Amendment 5 spec) were 30 LOC.

---

## Surprises and possible amendments

### 1. Diagnostic-code precedent supersedes the ADR's per-function-code plan

The ADR body (Decision 5) reserved MC1018/MC1019 for `nbinom_sf`/`nbinom_cdf` wrong-arg-count, and Amendment 3 directed preflight + shift-on-collision. But preflight revealed something more fundamental: **norm_cdf doesn't have its own wrong-arg-count code.** Every parser arity error in the codebase emits MC1008 via the shared `FormulaError::wrong_arg_count(...)` helper. The Phase 3H precedent test is literally named `parse_norm_cdf_arity_fires_mc1008`.

After surfacing this as a SPEC QUESTION, the decision was to reuse MC1008 — matching the precedent and the unified helper. The semantic names (`NBINOM_SF_WRONG_ARG_COUNT`, etc.) live in the error message text and test names; the numeric code is shared across all arity errors.

**Possible ADR amendment:** Add an Amendment 8 noting that per-function arity codes are a documentation aspiration, not the implementation pattern. The existing pattern (one shared code, function-specific message) is more idiomatic for Mosaic's parser.

### 2. Handoff's "ParsedRuleBody / ParsedNormCdfBody in mc-core/src/rule.rs" was incorrect

The handoff §"Required reading" pointed to `crates/mc-core/src/rule.rs:133-134` and `946-955` for `ParsedRuleBody` and `ParsedNormCdfBody`. Those lines actually contain `Expr::NormCdf` (mc-core's compiled evaluator AST), not `ParsedRuleBody`. The actual layout is two-layer:

- `crates/mc-model/src/schema.rs` owns `ParsedRuleBody` + `ParsedNormCdfBody` (parser AST, serde + JsonSchema).
- `crates/mc-core/src/rule.rs` owns `Expr` (evaluator AST, tuple variants).
- `crates/mc-model/src/compile.rs` translates the former to the latter.

I mirrored this two-layer pattern. The handoff template's `#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]` is also wrong for this codebase — the existing sibling structs use plain `#[derive(...JsonSchema)]` unconditionally.

**Possible handoff amendment:** Correct the file-path references in Phase 3L handoff Step 2/3 and update the schema-derive template. The actual pattern is more verbose than the handoff suggested (13 validate.rs walks, plus inspect.rs, lint.rs, compile.rs, cube.rs, formula.rs, correctness test, and mc-cli query.rs).

### 3. Compute helpers are `pub` (deviation from CLAUDE.md §5.5 minimalism)

CLAUDE.md §5.5 says default to `pub(crate)` over `pub`. But the handoff template explicitly imports `use mc_core::rule::{nbinom_sf_compute, nbinom_cdf_compute};` in the integration test — which requires `pub` because integration tests in `tests/` see the crate as an external dependency.

I made both helpers `pub`. The alternative (testing via Expr trees built manually) would mirror what `norm_cdf_compute` does (it's private and tested indirectly via Expr eval), but the handoff's explicit instruction was to test the helpers directly. The trade-off is that we now have two public surface items not strictly required by the ADR's "API signature" decision. If this is undesirable, swap them to `pub(crate)` and rewrite the test file to drive evaluation via the Expr AST instead.

---

## Files touched (final list)

```
modified:   crates/mc-core/src/rule.rs                          (+95 LOC: enum + 2 walks + 2 eval arms × 2 sites + 2 compute helpers)
modified:   crates/mc-core/src/cube.rs                          (+8 LOC: well-typed arm)
modified:   crates/mc-core/tests/correctness.rs                 (+5 LOC: collect_self_refs arm)
new file:   crates/mc-core/tests/nbinom_sf.rs                   (+220 LOC: 8 tests + fixture grid)
new file:   crates/mc-core/tests/nbinom_sf_fixtures.py          (+50 LOC: Python regen script)
modified:   crates/mc-model/src/schema.rs                       (+25 LOC: variants + ParsedNbinomBody)
modified:   crates/mc-model/src/formula.rs                      (+90 LOC: parser handlers + serialize + cross-coord + 5 inline tests)
modified:   crates/mc-model/src/compile.rs                      (+15 LOC: 2 translation arms)
modified:   crates/mc-model/src/inspect.rs                      (+5 LOC: collect_refs arm)
modified:   crates/mc-model/src/lint.rs                         (+5 LOC: collect_body_refs arm)
modified:   crates/mc-model/src/validate.rs                     (+80 LOC: 13 walks updated)
modified:   crates/mc-cli/src/query.rs                          (+5 LOC: 2 arms in filter evaluation)
modified:   docs/specs/mosaic-model-schema.json                 (+36 LOC: regenerated, new types)
new file:   docs/reports/phase-3l-completion-report.md          (this file)
```

---

## Cross-links

- ADR: [`../decisions/0031-nbinom-sf-formula-function.md`](../decisions/0031-nbinom-sf-formula-function.md)
- Handoff: [`../handoffs/phase-3l-nbinom-sf-handoff.md`](../handoffs/phase-3l-nbinom-sf-handoff.md)
- Research note: [`../research-notes/nbinom-sf-formula-function.md`](../research-notes/nbinom-sf-formula-function.md) (⚠ fixture values wrong — superseded by ADR Amendment 1)
- Sibling Phase 8.2: [`../handoffs/phase-8-2-consumer-api-handoff.md`](../handoffs/phase-8-2-consumer-api-handoff.md) (independent, parallel branch)

---

*End of report. Branch `phase-3l/nbinom-sf` ready for review and merge to `main`.*
