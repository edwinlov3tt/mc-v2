# ADR-0007: Phase 3D — Friendly Formula Syntax

**Status:** Accepted (with project-owner amendments — see "Acceptance amendments" section below)
**Date:** 2026-05-03 (Proposed); 2026-05-03 (Accepted, same day after implementer DONE)
**Deciders:** project owner
**Phase:** 3D scope (per [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md))

> **Process note: this is the first ADR drafted UNDER the new "handoff-first parallel flow."** Phase 3D's handoff was drafted, reviewed (GPT 5 + Desktop 6 = 11 acceptance amendments + GPT's 5 execution notes = 16 total), and shipped to the implementer BEFORE this ADR was written. The implementer's 3 minor deviations (folded in as amendments #32 + #33; #28 was already in the proposed-stage table) confirmed the flow worked for this small phase. **The flow is appropriate for Phase 3D's small surface; future larger phases (Phase 4 LLM authoring onward) return to ADR-Accepted-then-handoff** — see [`../process-notes.md`](../process-notes.md) for the carry-forward rule.
>
> Phase 3D shipped at `d5ab355`, tagged `phase-3d-friendly-formula-syntax`. Handoff at [`../handoffs/phase-3d-handoff.md`](../handoffs/phase-3d-handoff.md) was the implementation contract. This ADR is the strategic context behind it.

---

## Context

ADR-0004 Decision 4 reserved the friendly-formula phase as the next-after-3A model-layer extension; ADR-0006 renumbered it to **Phase 3D** (the original "Phase 3C" slot was taken by test-fixtures). After Phase 3C shipped at `8d2691a`, Phase 3D is now the next-proposed phase.

**The ergonomics gap.** Phase 3A's structured-tree rule body is precise but verbose:

```yaml
# Phase 3A form
rules:
  - target_measure: Clicks
    body:
      div:
        - ref: { measure: Spend }
        - ref: { measure: CPC }
```

Five Acme rules in this form take ~50 lines of YAML. The same rules in formula form take ~5 lines:

```yaml
# Phase 3D form
rules:
  - target_measure: Clicks
    body: "Spend / CPC"
```

The structured form remains valid; Phase 3D adds the friendly form as an alternate authoring surface. Both compile to the same `ParsedRuleBody` AST (existing 7 variants, unchanged).

**Why now, not later.** Phase 4 (LLM authoring) will emit YAML against the schema. LLMs author formula strings far more reliably than nested s-expression-shaped YAML — the structured form is verbose and error-prone in a way that defeats LLM iteration loops. Phase 3D is the last authoring-ergonomics phase before Phase 4; it should ship before LLM authoring is built so Phase 4 can target the friendlier surface from day one.

**The technical decision shape.** Smaller than ADR-0004 / ADR-0005 / ADR-0006. The big decisions are: (1) what operators? (2) what diagnostic codes? (3) how does the schema represent both forms? (4) how does the round-trip serializer handle paren rules? (5) does Acme migrate? Most of these have one obvious answer driven by the existing AST shape; the ADR captures them but doesn't deliberate at length.

---

## Decisions

### Decision 1: operator scope — exactly the existing AST capability

**Question:** What operators does Phase 3D's formula grammar support?

**Decision (Accepted):** Exactly the operators that map to the existing 7-variant `ParsedRuleBody` AST: `+`, `-`, `*`, `/`, parens, unary `+`/`-`, and `if_null(a, b)` function call. **No** `min`, `max`, `if`, comparison operators, conditional expressions, string/bool literals, or cross-dim references. All of those are deferred to future phases (or never).

**Why:** the AST is the contract. Adding operators to the formula grammar that don't have a corresponding AST node would require kernel-adjacent changes to support — exactly the kind of scope expansion the locked-surfaces guarantee prevents. Phase 3D is an *authoring layer* over the existing kernel, not a kernel extension.

**Downstream:** if Phase 4 LLM authoring or Phase 6 UI editor surfaces a real need for additional operators (e.g., conditional expressions for "if-then-else" planning logic), that's a Phase 3E ADR — adding both AST variants AND grammar rules together.

### Decision 2: diagnostic codes

**Question:** What diagnostic codes ship with Phase 3D's formula parser?

**Decision (Accepted):** Four new codes, all in MC1xxx (parse-time) namespace:

| Code | Rule |
|---|---|
| **MC1003** | Unbalanced or unexpected parenthesis |
| **MC1004** | Unexpected token (catch-all). **Per acceptance amendment #25, also covers "unknown function call" in Phase 3D** — `min(a, b)` fires MC1004 because only `if_null` is recognized. MC1007 is NOT introduced; future phases may split it out if the function table grows. |
| **MC1005** | Expected expression (e.g., trailing operator: `Spend +`) |
| **MC1006** | Invalid number literal (e.g., `1..5`, `1e`, `1.2.3`) |

**Identifier resolution stays MC2003** (existing — Phase 3A's "rule body references unknown measure"). Whether the body was authored as structured or formula, an unknown measure ref produces the same diagnostic.

**Why MC1xxx not MC2xxx:** these are text-syntax-level errors operating on string input, semantically equivalent to YAML parse errors. They run before validate-stage logic. The MC1xxx namespace is for "your text doesn't parse"; MC2xxx is for "your model is structurally wrong but parsed cleanly."

**Why MC1004 covers unknown-function (no MC1007):** stable codes are forever (CVE-style retirement per ADR-0005 amendment #11). Cheaper to add MC1007 later if needed than to ship a code we wish we hadn't shipped. Phase 3D's function table is `if_null` only; the catch-all is sufficient.

### Decision 3: schema shape — `ParsedRuleBodyForm` in `ParsedModel` only

**Question:** How does the schema represent both authoring forms?

**Decision (Accepted):** Wrap the existing `ParsedRuleBody` in `ParsedRuleBodyForm` with serde untagged dispatch:

```rust
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum ParsedRuleBodyForm {
    Formula(String),                  // YAML: body: "Customers * AOV"
    Structured(ParsedRuleBody),       // YAML: body: { mul: [...] }
}
```

**Critically (per acceptance amendment #23):** `ParsedRuleBodyForm` lives in `ParsedModel` ONLY. `ValidatedModel.rules[i].body` is flattened to `ParsedRuleBody` (no enum wrapper). Validate is where formula → AST compile happens AND where the wrapper unwraps. Downstream stages (`resolve_inputs`, `compile`, `inspect`) see `ParsedRuleBody` directly with NO awareness of formula authoring form.

**Why the flatten:** if `ValidatedModel` kept the wrapper, every consumer of `rule.body` would need a `match ... Structured(b) => ...` wrap even though only one variant is reachable post-validate. Existing call sites need ZERO changes if validate flattens.

**No new AST variant.** `ParsedRuleBody` keeps its 7 variants. Formulas compile DOWN to those variants; the AST stays clean (no string nodes).

### Decision 4: round-trip serialization

**Question:** Does Phase 3D ship `formula::serialize(&ParsedRuleBody) -> String`?

**Decision (Accepted):** Yes. Required for `mc model inspect` to render rules uniformly in formula form regardless of authoring form (per amendment #24).

**The risky part is paren handling.** Standard precedence (`+ -` < `* /`) plus right-associativity hazards: `Sub`/`Div` need parens around their right child if the right child is at the same precedence level. Per GPT note #3 + amendment #27, **`Mul` with right-child `Div` ALSO needs parens** — `Mul([a, Div([b, c])])` must serialize to `"a * (b / c)"`, NOT `"a * b / c"` (which reparses as `Div([Mul([a, b]), c])`, a different AST).

**Number literal formatting (per amendment #21):** use `f64::to_string()` (Rust's Ryu-based shortest-roundtrip), NOT `format!("{:.15}", v)`. `0.1_f64.to_string()` = `"0.1"` (what humans want); `format!("{:.15}", 0.1)` = `"0.100000000000000"` (ugly + breaks snapshot tests).

**Round-trip stability is a hard gate.** `parse(serialize(parse(s))) == parse(s)` MUST pass for all 5 Acme rules + the 6 explicit risky-case shapes documented in the handoff scope item 7.

### Decision 5: unary minus desugaring

**Question:** Does `-x` desugar to `Sub([Const(F64(0.0)), x])` or `Mul([Const(F64(-1.0)), x])`?

**Decision (Accepted): `Sub([Const(F64(0.0)), x])`** (pre-picked per amendment #22).

**Why:** preserves IEEE-754 signed-zero semantics under edge cases; cleaner in serialization; matches mental model "negate = subtract from zero". The `Mul` form is mathematically equivalent for non-edge values but introduces a constant `-1.0` literal that doesn't round-trip naturally back to unary syntax.

**Round-trip canonicalization:** the serializer detects `Sub([Const(F64(0.0)), x])` and emits `"-<serialize(x)>"` (canonical unary form), NOT `"0 - x"` (literal form). This is the canonical-form check that makes `parse(serialize(parse("-Spend"))) == parse("-Spend")` work.

### Decision 6: Acme migration

**Question:** Are Acme's 5 rules converted to formula form?

**Decision (Accepted):** Yes — all 5 rules in `crates/mc-model/examples/acme.yaml` migrate to formula form.

**Why:** Acme is the canonical example everyone copies from. If Acme stays structured, the feature is invisible to new model authors. The migration demonstrates the feature works end-to-end and provides the load-bearing round-trip test cases (especially Acme's `Gross_Profit = Revenue * (1 - COGS_Rate)` which exercises Mul wrapping Sub wrapping a numeric constant).

**Test fixtures stay structured (per amendment #10 / handoff §10):** `crates/mc-model/tests/lint_fixtures/` and `crates/mc-model/tests/fixture_validation_fixtures/` keep the structured form. They double as backwards-compat regression tests — proving the structured form still loads identically after Phase 3D.

### Decision 7: inspect rendering — uniform formula form

**Question:** Does `mc model inspect` render rules in formula form, structured form, or mixed?

**Decision (Accepted): Uniform formula form, regardless of authoring form** (per amendment #24, hard requirement).

**Why:** the entire UX point of Phase 3D is friendlier rule authoring. Mixed inspect output (some rules formulas, some trees) defeats that purpose for users who don't author the model themselves. The form a rule was AUTHORED in does NOT determine the form it's RENDERED in.

**Snapshot test coverage:** `tests/expected/inspect_acme.txt` (formula-authored Acme) AND a new snapshot for `_acme_with_bad_golden.yaml` (structured-authored) — both render rules in formula form.

### Decision 8: validate() signature change

**Question:** Does `validate()` change its return type to accommodate formula syntax errors?

**Decision (Accepted, per GPT execution note #1):** The `validate()` signature MAY change from `Vec<ValidationError>` to `Vec<Error>` (or equivalent unified error type) because MC1003–MC1006 are PARSE errors discovered during validation/normalization. The implementer's call on the exact type shape; the constraint is:

- The Diagnostic struct shape is UNCHANGED.
- `schema_version` stays at `"1.0"` (additive code adds are backwards-compatible per ADR-0006 amendment #20).
- The JSON envelope shape downstream consumers (Phase 4 LLM, Phase 6 UI) pin to is unchanged.

This is an `mc-model` API-surface adjustment, documented clearly in the completion report's "Source manifest" / API-changes section. Public API consumers (CLI subcommands, test harness, future Phase 4 LLM scaffolding) handle the unified error type.

### Decision 9: out of scope

**Question:** What is *not* Phase 3D?

**Decision (Accepted):**

| Out of scope | Phase / disposition |
|---|---|
| Function calls beyond `if_null` (`min`, `max`, `if`, `sum_if`) | Phase 3E or later (would add AST variants too) |
| Comparison operators (`<`, `>`, `==`) | Phase 3E or later |
| Conditional expressions (`a > b ? x : y`) | Phase 3E or later |
| String / bool constants | Phase 3E or later (would extend `ParsedScalar` too) |
| Cross-dim references (`DB(...)` lookups) | Phase 3E or later (kernel-adjacent) |
| Custom operators / user-defined functions | Probably never |
| Auto-fix (`mc model fix-formulas`) | Future phase |
| Bidirectional round-trip (Cube → formula text) | Future Phase 3 sub-phase if needed |
| LLM authoring of formulas | Phase 4 (consumes Phase 3D's formula parser as the schema) |
| Performance tuning on formula compile | Not needed; formula compile happens once per rule at validate, not per-cell at read |

---

## Acceptance amendments

This ADR is being drafted in parallel with implementation. The Phase 3D handoff was reviewed before kickoff and incorporated 16 amendments — captured here as the audit trail. Sources:

- **GPT first review (5 clarifications)** — confirmed all 5 handoff decisions with operator-scope tightening, MC1003–MC1006 negative-test list, ParsedModel-vs-ValidatedModel split, Acme migration policy, and round-trip-as-the-real-gate.
- **Claude Desktop first review (6 refinements + 1 wording note)** — Ryu number formatting, unary minus pre-pick, ValidatedModel flatten, uniform inspect, MC1004-covers-unknown-function (no MC1007), null-body test, plus the wording-tightening on `--fixture` (carried over from Phase 3C).
- **GPT execution notes (5 pre-coding additions)** — validate() signature change approval, ValidatedModel normalization (reaffirmation), serializer paren rule for `Mul([_, Div(_)])`, MC_SNAPSHOT_UPDATE caution, and lock reaffirmation.

| # | Source | Amendment (one-line) | Where it landed (handoff or this ADR) |
|---|---|---|---|
| GPT 1 (clarif) | GPT first review | Operator scope confirmed exactly to existing AST capability | Decision 1 |
| GPT 2 (clarif) | GPT first review | MC1003–MC1006 with explicit negative-test list (5 fixtures) | Decision 2; handoff scope item 6 |
| GPT 3 (clarif) | GPT first review | ParsedModel preserves both forms; ValidatedModel normalized to ParsedRuleBody | Decision 3; handoff scope item 2 |
| GPT 4 (clarif) | GPT first review | Acme migrated; structured fixtures stay structured | Decision 6 |
| GPT 5 (clarif) | GPT first review | Round-trip is the gate; explicit risky-case list | Decision 4; handoff scope item 7 |
| 21 | Claude Desktop | Ryu number formatting (`f64::to_string()`), NOT `format!("{:.15}", v)` | Decision 4; handoff scope item 7 |
| 22 | Claude Desktop | Unary minus pre-picked: `Sub([Const(F64(0.0)), x])`, NOT `Mul([Const(F64(-1.0)), x])` | Decision 5; handoff scope item 5 + final checklist |
| 23 | Claude Desktop | `ValidatedModel.body` flattens to `ParsedRuleBody`; `ParsedRuleBodyForm` only in `ParsedModel` | Decision 3; handoff scope item 2 + final checklist |
| 24 | Claude Desktop | Inspect rendering = uniform formula form (HARD requirement, NOT implementer's call) | Decision 7; handoff scope item 8 + acceptance gate item 7 |
| 25 | Claude Desktop | MC1004 covers "unknown function" in Phase 3D; MC1007 NOT introduced | Decision 2; handoff scope item 6 |
| 26 | Claude Desktop | YAML-null-body test (assert existing MC2xxx fires, not new code) | Handoff acceptance gate item 11 + final checklist |
| 27 | GPT execution note #3 | Serializer paren rule extended: `Mul([a, Div([b, c])])` must paren the right child | Decision 4; handoff scope item 7 |
| 28 | GPT execution note #1 | `validate()` signature change to `Vec<Error>` is approved (formula syntax errors are discovered during validation/normalization) | Decision 8; handoff scope item 3 |
| 29 | GPT execution note #2 | ValidatedModel normalization (reaffirmation of #23) | Already covered by #23 |
| 30 | GPT execution note #4 | MC_SNAPSHOT_UPDATE caution — manual diff check before regenerating snapshots | Operating principles (handoff carry-forward; not a code-level change) |
| 31 | GPT execution note #5 | Lock reaffirmation (mc-core, mc-fixtures, deps, AST variants, Diagnostic shape) | Already covered by handoff hard rules |
| **32** | **Implementer (Phase 3D §4.2)** | **`Error::code()`, `Error::as_validation()`, `Error::as_parse()` helpers added.** Convenience wrappers on the unified `Error` type to keep test-side filtering ergonomic. Additive; no behavior change. | New public methods on `Error`; shipped in `crates/mc-model/src/error.rs` |
| **33** | **Implementer (Phase 3D §4.3)** | **YAML `body: null` fires MC1001 (parse-stage), not MC2010 (validate-stage).** Untagged enum dispatch on null fails at the YAML/parse stage before reaching validate. **Amendment #26's intent ("no new formula error code; use existing infrastructure") is satisfied** — MC1001 is the existing Phase 3B parse-stage code, just at a different pipeline stage than amendment #26 anticipated. | Documented in completion report §4.3; null-body fixture asserts MC1001 fires |

No remaining open questions. Phase 3D shipped at `d5ab355` (tag `phase-3d-friendly-formula-syntax`).

---

## What this unlocks

Phase 3D's deliverable is the foundation for:

- **Phase 4 — LLM authoring:** the LLM emits YAML models with formula-form rule bodies. The friendlier syntax dramatically improves LLM authoring success rate vs structured trees. Phase 4 also benefits from MC1003–MC1006's stable codes for the iteration loop.
- **Phase 6 — UI editor:** the editor renders rules in formula form by default (per Decision 7). The `formula::serialize` API is the contract.
- **Future Phase 3E** (if needed): operators beyond Phase 3D's scope would extend both the AST and the formula grammar in tandem.

---

## Recommended decisions — TL;DR

If approved (this ADR is Proposed pending the implementer's report — drafted in parallel per the new flow), Phase 3D ships against:

1. **Operator scope** — exactly the existing AST: `+ - * /`, parens, unary `+/-`, `if_null(a, b)`. No `min`/`max`/`if`/comparisons (Decision 1).
2. **Diagnostic codes** — MC1003–MC1006 in MC1xxx (parse-time). MC1004 covers unknown function calls (no MC1007). Identifier resolution stays MC2003 (Decision 2).
3. **Schema shape** — `ParsedRuleBodyForm { Formula(String), Structured(ParsedRuleBody) }` in `ParsedModel` ONLY. `ValidatedModel.body` is flattened to `ParsedRuleBody` (Decision 3).
4. **Round-trip serialization** — Ryu number formatting; explicit paren rules including `Mul` over `Div`; round-trip stability is a hard test gate (Decision 4).
5. **Unary minus** — pre-picked as `Sub([Const(F64(0.0)), x])` (Decision 5).
6. **Acme migration** — all 5 rules to formula form; test fixtures stay structured (Decision 6).
7. **Inspect rendering** — uniform formula form regardless of authoring form (Decision 7).
8. **validate() signature** — may change to `Vec<Error>`; Diagnostic shape unchanged; `schema_version` stays `"1.0"` (Decision 8).
9. **Out of scope** — extensive list (Decision 9). No new operators, no new AST variants, no kernel changes.

---

## Alternatives considered

1. **Skip Phase 3D; jump straight to Phase 4 LLM authoring.** Rejected — LLMs author structured-tree YAML poorly. Phase 3D's friendlier surface is a precondition for Phase 4's success rate.
2. **Pull in a parser library (`pest`, `nom`, `lalrpop`).** Rejected — the grammar is small (~100 lines of grammar rules); a hand-rolled recursive-descent parser is ~250–350 lines and adds zero deps. The library cost (transitive deps, toolchain risk, build-time) is unjustified for this scope.
3. **Add a `Formula` variant to `ParsedRuleBody`.** Rejected — would pollute the AST with a string node that compile/lint/inspect would have to handle. The wrapper-then-flatten approach (Decision 3) keeps the AST clean.
4. **Defer round-trip serialization to a future phase; ship parse-only.** Rejected — `mc model inspect` needs the serializer for uniform formula rendering (Decision 7). Without it, Phase 3D's UX win is incomplete.
5. **Ship with mixed inspect rendering (formulas for formula-authored, trees for structured-authored).** Rejected per amendment #24 — defeats the UX point of the phase.
6. **Add comparison operators / conditional expressions in Phase 3D.** Rejected — would require extending the AST (Phase 3E or later), which is kernel-adjacent and out of scope.
7. **Bundle Phase 3D + Phase 4.** Rejected — different scopes, different risk profiles. Phase 4's LLM scaffolding is a substantial addition; bundling expands Phase 3D from ~1 day to a week+.

---

## Cross-links

- [`../handoffs/phase-3d-handoff.md`](../handoffs/phase-3d-handoff.md) — Phase 3D implementation contract (in flight).
- [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) — Phase 3D row.
- [`../CURRENT_STATE.md`](../CURRENT_STATE.md) — Phase status.
- [`../process-notes.md`](../process-notes.md) — handoff-first parallel flow rule (this is the first ADR drafted under it).
- [`0004-phase-3a-model-definition-format.md`](0004-phase-3a-model-definition-format.md) Decision 4 — original Phase 3D scope reservation (then named "Phase 3C").
- [`0005-phase-3b-model-qa-linter-diagnostics.md`](0005-phase-3b-model-qa-linter-diagnostics.md) — Diagnostic shape contract (Phase 3D adds codes; doesn't modify shape).
- [`0006-phase-3c-model-test-fixtures.md`](0006-phase-3c-model-test-fixtures.md) — Phase 3C ADR (recent).
- [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) — kernel contract (Phase 3D doesn't touch).
- [`../../crates/mc-model/src/schema.rs`](../../crates/mc-model/src/schema.rs) lines 158–223 — the existing `ParsedRuleBody` AST that Phase 3D compiles to.

---

## Notes

This ADR was **drafted in parallel with implementation** under the new "handoff-first parallel flow" (the first ADR to use it). The handoff was the binding contract during implementation; this ADR landed at Acceptance with the implementer's 3 minor deviations folded as amendments #32 + #33 (#28 was pre-approved at the proposed stage). The flow worked cleanly for Phase 3D's small surface — implementer hit no SPEC QUESTIONs that required ADR revision mid-flight.

**Carry-forward rule:** for Phase 4 (LLM authoring) onward, return to ADR-Accepted-then-handoff. Phase 4 is bigger (new dep on LLM provider, prompt scaffolding, iteration-loop semantics, error-feedback contract); the heavier ceremony is appropriate. See [`../process-notes.md`](../process-notes.md) §1 for the 5-question self-test that picks the flow.
