# Phase 3D — For Dummies

> **In one line:** before Phase 3D, rule bodies in YAML looked like nested s-expressions: `{ mul: [{ ref: "Customers" }, { ref: "AOV" }] }`. After Phase 3D, you can write them like Excel formulas: `"Customers * AOV"`. Both forms work; the formula compiles down to the same tree under the hood.

> **Shipped 2026-05-03** at commit `d5ab355`, tag `phase-3d-friendly-formula-syntax`. See [completion report](../../reports/phase-3d-completion-report.md) for the full audit.

[Technical version → handoff](../../handoffs/phase-3d-handoff.md) · [ADR-0007](../../decisions/0007-phase-3d-friendly-formula-syntax.md) · [completion report](../../reports/phase-3d-completion-report.md)

---

## The analogy: Excel's formula bar comes to YAML

In Excel, you click a cell and type `=A1*B1`. The spreadsheet figures out what to do. You don't write `{ multiply: [{ cell: "A1" }, { cell: "B1" }] }` — that would be insane.

But that's exactly what Phase 3A made you do for YAML rules. Acme's `Revenue = Customers * AOV` rule looked like this:

```yaml
# Phase 3A form (precise but verbose)
- target_measure: Revenue
  body:
    mul:
      - ref: { measure: Customers }
      - ref: { measure: AOV }
```

Five Acme rules in this form took ~50 lines of YAML. After Phase 3D, the same rules look like:

```yaml
# Phase 3D form (Excel-flavored)
- target_measure: Revenue
  body: "Customers * AOV"
```

Same five rules: ~5 lines. And the kicker: **the structured form still works.** Models authored in the old form keep loading correctly. Phase 3D is purely additive — you can use either form, or both in the same file, and the kernel doesn't know the difference.

## What Phase 3D actually shipped

Six concrete pieces of work:

**(1) A new module: `crates/mc-model/src/formula.rs`.** ~250 lines of hand-rolled recursive-descent parser plus ~80 lines of round-trip serializer. **No `pest`, no `nom`, no `lalrpop`** — pulling in a parser library would have added transitive deps and toolchain risk for a grammar small enough to write directly.

**(2) The grammar — exactly what the existing AST supports.** Phase 3D adds NO new operators to the kernel. The grammar is:

- **Operators:** `+`, `-`, `*`, `/`, parentheses, unary `+`/`-`
- **Function calls:** only `if_null(a, b)` (it's the only function in the existing AST)
- **Identifiers:** bare measure names like `Spend`, `CPC`, `Close_Rate`
- **Numbers:** standard `1`, `1.5`, `1.5e2`, etc.
- **Whitespace:** ignored
- **Reserved words:** only `if_null`

Standard precedence: `*` and `/` bind tighter than `+` and `-`; parens override. If you've written math, you already know it.

What's NOT supported: `min`, `max`, `if`, comparisons (`<`, `>`, `==`), conditional expressions (`a > b ? x : y`), string/bool literals, cross-cube references. All of those would require new AST variants in the kernel — out of scope for Phase 3D, which deliberately stayed an authoring layer over the existing engine.

**(3) Two-form schema.** YAML rule bodies are now either a string or the existing structured object:

```rust
// Inside ParsedModel — both forms accepted
pub enum ParsedRuleBodyForm {
    Formula(String),                  // YAML: body: "Customers * AOV"
    Structured(ParsedRuleBody),       // YAML: body: { mul: [...] }
}
```

But — critically — by the time the model finishes validation, the formula has been parsed and the wrapper unwrapped:

```rust
// Inside ValidatedModel — flat AST, no enum wrapper
pub struct ValidatedRule {
    pub body: ParsedRuleBody,         // Always structured AST, no string
}
```

So everything downstream of validation (the cube compiler, the linter, the inspect renderer) works with the same `ParsedRuleBody` type it always has. The whole formula concept is a parsing layer, transparent to the rest of the system.

**(4) Four new diagnostic codes.** When a formula doesn't parse, you get one of:

| Code | Catches |
|---|---|
| MC1003 | Unbalanced parentheses (`(Spend / CPC` — missing close) |
| MC1004 | Unexpected token. **Also catches "unknown function call"** (`min(a, b)` fires MC1004 because only `if_null` is recognized) |
| MC1005 | Expected expression (e.g., trailing operator: `Spend +`) |
| MC1006 | Invalid number literal (e.g., `1..5`, `1e`, `1.2.3`) |

If you reference a measure that doesn't exist (`body: "Spnd / CPC"`), that fires the existing MC2003 — same diagnostic regardless of whether the rule body was authored as structured or formula form.

**(5) Round-trip serialization** (the risky part). `mc model inspect` now renders all rules in formula form, regardless of how they were authored. To do that, the system needs to convert structured trees BACK into formula strings. Sounds easy, but parens are subtle.

The classic trap: if a rule's structured tree is `Mul([a, Div([b, c])])`, the math is `a * (b / c)`. But naively serializing as `"a * b / c"` reparses left-to-right at same precedence as `(a * b) / c` — a different tree. The serializer has to add parens around the right child whenever needed to preserve the AST shape.

The same trap applies to subtraction (`a - (b - c)` ≠ `(a - b) - c`) and division. Phase 3D's serializer handles all of these correctly, plus the special case of unary minus (`-Spend` round-trips through `Sub([Const(0.0), Ref("Spend")])` and back to `"-Spend"`).

The number formatting also matters. Acme's `Gross_Profit` rule has `1 - COGS_Rate` — that `1` is a numeric constant. If serialized as `"1.000000000000000"`, it would be ugly. Phase 3D uses Rust's Ryu shortest-round-trip formatting (`f64::to_string()`), so `0.1` stays `"0.1"` and `1.5` stays `"1.5"`.

The round-trip stability test is the gate: `parse(serialize(parse(s))) == parse(s)` for every Acme rule plus an explicit list of historically-tricky shapes.

**(6) Acme migration.** All 5 rules in `crates/mc-model/examples/acme.yaml` were rewritten in formula form:

| Rule | Phase 3A form (~5 lines each) | Phase 3D form |
|---|---|---|
| Clicks | `{ div: [{ ref: "Spend" }, { ref: "CPC" }] }` | `"Spend / CPC"` |
| Leads | `{ mul: [{ ref: "Clicks" }, { ref: "CVR" }] }` | `"Clicks * CVR"` |
| Customers | `{ mul: [{ ref: "Leads" }, { ref: "Close_Rate" }] }` | `"Leads * Close_Rate"` |
| Revenue | `{ mul: [{ ref: "Customers" }, { ref: "AOV" }] }` | `"Customers * AOV"` |
| Gross_Profit | `{ mul: [{ ref: "Revenue" }, { sub: [{ const: 1 }, { ref: "COGS_Rate" }] }] }` | `"Revenue * (1 - COGS_Rate)"` |

That last one — `Gross_Profit` — is the load-bearing test case. It has unary, parens, a numeric constant, AND the Mul-over-Sub paren rule that the round-trip serializer has to get right. If `Gross_Profit` round-trips, every other Acme rule does too.

The structural-equivalence test (Phase 3A) and demo-equivalence test (Phase 3A) both still pass — proving the migration didn't change the cube's behavior, just the surface authors see.

## Why we care / what would have gone wrong without it

Three things would have stayed friction:

**(1) Phase 4 (LLM authoring) would have struggled.** LLMs author Excel-style formulas reliably. They author nested s-expression-shaped YAML poorly — too many opportunities for typos in the structural keys (`mul` vs `mult` vs `multiply`), too many ways to accidentally produce malformed nesting. Phase 4 will emit YAML models from natural-language prompts; without Phase 3D, every LLM iteration would have wasted cycles on structural-syntax errors. After 3D, the LLM emits the friendlier form and gets to focus on getting the *math* right.

**(2) Authoring would have stayed painful for humans.** Acme's 5 rules in structured form are ~50 lines of YAML. Reviewing those for correctness during a code review or a design conversation requires mentally parsing the tree shape. Reviewing `body: "Revenue * (1 - COGS_Rate)"` is instant. As models grow (production cubes have hundreds of rules), the authoring-friction tax compounds.

**(3) The mental gap with Excel users would have stayed wide.** Planning users come from Excel. They write `=A1*B1` every day. Asking them to author rules as nested objects with structural keys is asking them to learn a foreign language to do something they already know how to do. Phase 3D closes that gap — formula syntax is the lingua franca of planning, and now the YAML speaks it.

## One thing that's easy to get wrong

The biggest temptation when writing a parser is to reach for a parser library (`pest`, `nom`, `lalrpop`). Phase 3D deliberately did **not**. The grammar is small enough (~5 production rules) that a hand-rolled recursive-descent parser is ~250 lines of straightforward Rust. A parser library would have added transitive deps (potentially incompatible with the project's Rust 1.78 toolchain pin), bigger build times, and learning overhead for future maintainers — all to save maybe 50 lines of code.

The other thing easy to misread is **what changed under the hood vs what didn't**:

- **The kernel didn't change.** `mc-core` is byte-for-byte unchanged since Phase 2D.
- **The fixtures didn't change.** `mc-fixtures::build_acme_cube()` is byte-for-byte unchanged since Phase 1A.
- **The AST didn't change.** Same 7 variants in `ParsedRuleBody` (Const, Ref, Add, Sub, Mul, Div, IfNull). Formulas compile DOWN to those existing variants. No new AST node, no new field, no new operator.
- **What changed:** a new module (`formula.rs`) that translates between formula strings and the AST; a schema enum that lets YAML accept either form; and a validate-stage step that calls the parser when a string is encountered.

Phase 3D is the model author's keyboard interface. The engine doesn't know whether a rule was originally typed as `"Customers * AOV"` or `{ mul: [{ ref: "Customers" }, { ref: "AOV" }] }` — by the time it sees the rule, it's just an AST.

## A meta-note: the new flow

Phase 3D was the first phase shipped under the "handoff-first parallel flow" — a process change introduced after Phase 3C. Earlier phases worked like this:

```
ADR drafted → ADR reviewed → ADR Accepted → handoff drafted → kickoff → implementer
```

Implementer waited for the full ADR cycle (~4 hours) before starting. Phase 3D's flow:

```
Direction approved → handoff drafted → kickoff → implementer
                                                        ‖ (in parallel)
                                                        ADR drafted + reviewed + Accepted
```

Implementer started ~30 minutes after kickoff. ADR + metadata work happened concurrently with the implementation. Net result: Phase 3D shipped roughly half a day faster than it would have under the old flow.

This isn't a free lunch — the new flow only works for **small phases where the strategic decisions are derivable from prior ADRs**. Phase 4 (LLM authoring) is bigger and returns to the old flow. The rule lives in [`docs/process-notes.md`](../../process-notes.md) §1.

## What Phase 3D is and isn't

| It is | It isn't |
|---|---|
| An authoring ergonomics layer over the existing AST | A change to the kernel (mc-core untouched) or fixtures (mc-fixtures untouched) |
| Hand-rolled recursive-descent parser (~250 lines) | A dependency on `pest` / `nom` / `lalrpop` |
| Both forms accepted (formula AND structured) | A breaking change — Phase 3A models still load |
| 4 new parse-time diagnostic codes (MC1003–MC1006) | New AST variants, new lint rules, or new lint codes |
| Round-trip serializer for uniform inspect rendering | A round-trip-from-Cube-to-formula path (that's a future phase if needed) |
| Acme migrated to formula form | A migration of test fixtures (those stay structured as backwards-compat tests) |
| MC1004 covers "unknown function" too (no MC1007) | A separate code for every error class — codes stay forever, ship conservatively |
| The first phase under the new "handoff-first parallel flow" | A new default — Phase 4+ returns to ADR-first |

## How long it took

About a day of focused implementation work. The biggest pieces:

- New `formula.rs` module: 690 lines including unit tests (~250 parser + ~80 serializer + ~360 tests)
- Schema additions: `ParsedRuleBodyForm` enum wrapper in `ParsedModel`, flat `ParsedRuleBody` in `ValidatedModel`
- Validate stage: new step 0 that parses formulas before downstream validators run
- 4 new diagnostic codes in `error.rs` (MC1003–MC1006)
- Inspect rendering: uniform formula form for all rules
- 68 new tests across 4 new test files + 8 negative fixtures
- Acme migration: 5 rules rewritten

Test count: 328/0 → **396/0** (+68 tests). Headline gate: Acme's 5 rules are formula strings AND every prior gate (lint zero warnings, demo equivalence empty, goldens 9/9, equivalence test byte-identical) still passes.

Plus a process win: shipped under the new "handoff-first parallel flow" — about half a day faster than the old ADR-first cycle would have been.

---

*Tied to: [phase-3a.md](./phase-3a.md) (the Phase that introduced the structured-tree AST that Phase 3D adds friendlier syntax over), [phase-3c.md](./phase-3c.md) (the previous phase, which removed the Acme-name special case and made model files self-contained), [`../research-notes/totals-vs-formulas.md`](../research-notes/totals-vs-formulas.md) (the conceptual relationship between Excel formulas and the planning model — Phase 3D narrows the syntactic gap, but the structural-vs-formulaic distinction it discusses still holds).*
