# Phase 3D Handoff — Friendly Formula Syntax

> **Audience:** the Claude Code instance running in
> `/Users/edwinlovettiii/Projects/mc-v2/` that picks up Phase 3D.
> **You inherit a green Phase 3C** (commit `8d2691a`, tag
> `phase-3c-fixtures-and-inputs`).
>
> **This phase adds an authoring ergonomics layer over rule bodies.**
> Today, YAML rule bodies are structured s-expression-shaped trees:
> `body: { mul: [{ ref: "Customers" }, { ref: "AOV" }] }`. After
> Phase 3D, they may also be authored as formula strings:
> `body: "Customers * AOV"`. The formula compiles down to the same
> structured tree at validate time. **Both forms remain accepted**
> (additive, backwards-compatible).
>
> **Hard rule:** Phase 3D touches `crates/mc-model/` (new formula
> parser module, schema enum addition, validate integration, Acme
> migration) and `crates/mc-cli/` not at all in normal scope. It does
> NOT touch `crates/mc-core/`, `crates/mc-fixtures/`, `docs/specs/`,
> or any kernel/fixture file. The locked-surfaces guarantee from
> Phase 2D / 3A / 3B / 3C carries forward.
>
> **Note on ADR:** ADR-0007 covering Phase 3D is being drafted in
> parallel with this handoff (per the new "handoff-first parallel
> flow" the project owner adopted). The ADR will land alongside
> implementation; if it surfaces a substantive change to direction,
> you'll get a SPEC QUESTION mid-flight. The decisions in this
> handoff are the binding contract until that happens.

---

## Where Phase 3C ended

- **Phase 3C commit / tag:** `8d2691a` — *phase-3c: model test fixtures + input sets* — tag `phase-3c-fixtures-and-inputs`. Backfill at `a91e85f`.
- **Test status:** 328 / 0 passing across all targets. 10/10 deterministic.
- **Demos:** `cargo run --release --bin mc -- demo` matches brief §4.6. `cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml` produces byte-for-byte identical output.
- **Headline carry-forwards (still hold):** `mc model lint` zero warnings; `mc model test` 9/9 goldens pass at ~30 ms; equivalence between Rust and YAML+CSV paths byte-identical on 2,520 coords.
- **Toolchain:** Rust 1.78. Cargo.lock pins from Phase 1B (`clap`, `clap_lex`, `half`) + Phase 3A (`indexmap → 2.7.0`, `hashbrown → 0.15.5`). Do not bump.
- **`mc-core`, `mc-fixtures` deps unchanged** since Phase 2D (mc-core) and Phase 1A (mc-fixtures).

For the full Phase 3C audit see [`../reports/phase-3c-completion-report.md`](../reports/phase-3c-completion-report.md). For the prior Phase 3D framing see [ADR-0004 Decision 4](../decisions/0004-phase-3a-model-definition-format.md) (originally named this Phase 3C; renamed to Phase 3D per ADR-0006 roadmap impact).

**Note on AST shape (the contract you build against):** see [`crates/mc-model/src/schema.rs`](../../crates/mc-model/src/schema.rs) lines 158–223 for the existing `ParsedRuleBody` enum. The 7 variants today are: `Const`, `Ref`, `Add`, `Sub`, `Mul`, `Div`, `IfNull`. Phase 3D's formula grammar must compile **only** to these existing variants — no new AST nodes.

---

## Phase 3D prompt (verbatim — this is your contract)

> We are starting MarketingCubes Phase 3D: Friendly Formula Syntax.
>
> **Context.** Phase 3A established YAML model authoring with structured-tree rule bodies. Phase 3B added linting + diagnostics. Phase 3C added test fixtures + input sets. Phase 3D closes the last major UX gap before Phase 4 LLM authoring: rule bodies become authorable as formula strings (`Revenue = Customers * AOV`) instead of nested s-expression-shaped YAML (`{ mul: [{ ref: "Customers" }, { ref: "AOV" }] }`).
>
> **Goal.** Ship `mc-model`'s formula parser such that:
>
> 1. The Acme YAML's 5 rules are re-authored as formula strings; the cube produced is byte-identical to the structured-form version that exists today.
> 2. Existing structured-form YAMLs (lint fixtures, validator fixtures, golden fixtures) continue to load unchanged — backwards compat is mandatory.
> 3. A new module `crates/mc-model/src/formula.rs` provides `parse(s: &str) -> Result<ParsedRuleBody, FormulaError>` and `serialize(&ParsedRuleBody) -> String` (round-trip).
> 4. Formula syntax errors get stable diagnostic codes MC1003–MC1006 (parse-time errors); identifier-resolution errors continue to use existing MC2003 (rule references unknown measure).
> 5. `mc model inspect` shows the formula form for rules that have one (rendered via `formula::serialize`); rules authored in structured form show as the structured tree.
> 6. All Phase 3C gates still hold: `mc model lint acme.yaml` zero warnings; `mc model test acme.yaml` 9/9 goldens pass; equivalence test still byte-identical; `mc demo --model acme.yaml` still matches `mc demo`.
>
> **Phase 3D scope** (binding contract):
>
> 1. **Add `crates/mc-model/src/formula.rs`** — a hand-rolled recursive-descent parser. Public API:
>    - `pub fn parse(input: &str) -> Result<ParsedRuleBody, FormulaError>` — parses formula text into the existing AST.
>    - `pub fn serialize(body: &ParsedRuleBody) -> String` — renders the AST back to formula text with proper paren handling for precedence.
>    - `pub struct FormulaError { code, span, message }` — internal error type; converted to `ParseError` (MC1003–MC1006) at the validate-stage boundary.
>
> 2. **Schema additions** in `crates/mc-model/src/schema.rs` — **`ParsedRuleBodyForm` lives in `ParsedModel` ONLY; `ValidatedModel.rules[i].body` flattens to `ParsedRuleBody`** (binding contract per acceptance amendment #23):
>    - Wrap the existing `ParsedRuleBody` enum's usage in `ParsedModel::rules[i].body: ParsedRuleBodyForm` where:
>      ```rust
>      #[derive(Clone, Debug, Deserialize)]
>      #[serde(untagged)]
>      pub enum ParsedRuleBodyForm {
>          Formula(String),                  // YAML: body: "Customers * AOV"
>          Structured(ParsedRuleBody),       // YAML: body: { mul: [...] }
>      }
>      ```
>    - Serde untagged dispatch picks `Formula` if the YAML value is a string, `Structured` if it's a map. Order matters — String first.
>    - **`ValidatedModel::rules[i].body: ParsedRuleBody`** (always flat, no enum wrapper). The validate stage is where `Formula(s)` is parsed into `Structured(_)` AND where the `ParsedRuleBodyForm` wrapper is unwrapped. Downstream stages (`resolve_inputs`, `compile`, `inspect`) get `ParsedRuleBody` directly with NO awareness of formula authoring form.
>    - **Why this matters:** if `ValidatedModel` kept the `ParsedRuleBodyForm` wrapper, every consumer of `rule.body` would need a `match ... Structured(b) => ...` wrap even though only one variant is ever reachable post-validate. Existing call sites in `resolve_inputs` / `compile` / `inspect` need ZERO changes if the wrapper unwraps in validate.
>    - **If `ParsedModel` and `ValidatedModel` aren't currently separate types** (i.e., `ValidatedModel` is just a tagged-as-validated alias for `ParsedModel`), introduce the split as part of Phase 3D — ~20 lines of plumbing. The existing tests confirm the split doesn't break anything.
>    - **Do NOT add a `Formula` variant to `ParsedRuleBody` itself.** The AST stays clean (no string nodes). Formulas compile DOWN to the existing 7 variants.
>
> 3. **Validate-stage integration** in `crates/mc-model/src/validate.rs`:
>    - When `ParsedRule.body` is `ParsedRuleBodyForm::Formula(s)`, call `formula::parse(s)`. On `Ok(body)`, replace with `Structured(body)` for downstream processing. On `Err(e)`, emit `ParseError` with the appropriate MC1003–MC1006 code.
>    - When `body` is `Structured(_)`, no formula-parsing step; existing validation flow takes over.
>    - **The downstream stages (resolve_inputs, compile) see only `Structured(_)` after validate.** They have no awareness of formula form.
>    - **`validate()` signature change is APPROVED (per GPT note #1):** if the existing `validate()` returns `Vec<ValidationError>`, it MAY be changed to return `Vec<Error>` (or equivalent unified error type) because formula syntax errors MC1003–MC1006 are PARSE errors discovered during validation/normalization. Document the API adjustment clearly in the completion report's "Source manifest" section. **This is an `mc-model` API adjustment ONLY — it does NOT change the `Diagnostic` struct shape and does NOT bump `schema_version` from `"1.0"`.** The error variants are folded under one return type; the JSON envelope stays identical.
>
> 4. **Formula grammar** (recursive descent, standard precedence):
>    ```
>    expression  = term (("+" | "-") term)*
>    term        = factor (("*" | "/") factor)*
>    factor      = unary
>                | "(" expression ")"
>                | identifier "(" arglist ")"        // function call (only if_null)
>                | identifier                         // measure ref
>                | number
>    unary       = ("+" | "-") factor
>    arglist     = expression ("," expression)*
>    identifier  = letter (letter | digit | "_")*
>    number      = digits ("." digits)? ("e" ("+" | "-")? digits)?
>    letter      = [A-Za-z_]
>    digit       = [0-9]
>    ```
>
>    - **Operators:** `+`, `-`, `*`, `/`, parens, unary `+`/`-`.
>    - **Function calls:** ONLY `if_null(primary, fallback)` (matches the existing `IfNull` AST variant). No other functions in Phase 3D.
>    - **Identifiers:** bare measure names (e.g., `Spend`, `Close_Rate`). Letter/underscore-start, then alphanumeric/underscore. Case-sensitive (matches measure-declaration case).
>    - **Numbers:** F64 literals. Integer literals (`1`, `100`) auto-promote to F64. Scientific notation (`1.5e2`) supported.
>    - **Whitespace:** ignored between tokens.
>    - **Comments:** none.
>    - **Reserved words:** only `if_null` (the function call name). All other identifiers are measure refs.
>
> 5. **Compilation rules** (formula AST → ParsedRuleBody):
>    - `a + b` → `ParsedRuleBody::Add({ add: [a, b] })`
>    - `a - b` → `ParsedRuleBody::Sub({ sub: [a, b] })`
>    - `a * b` → `ParsedRuleBody::Mul({ mul: [a, b] })`
>    - `a / b` → `ParsedRuleBody::Div({ div: [a, b] })`
>    - `if_null(a, b)` → `ParsedRuleBody::IfNull({ if_null: [a, b] })`
>    - `Spend` → `ParsedRuleBody::Ref({ ref: "Spend" })`
>    - `1.5` → `ParsedRuleBody::Const({ value: ParsedScalar::F64(1.5) })`
>    - **Unary `-x` → `ParsedRuleBody::Sub({ sub: [Const(F64(0.0)), x] })`** (pre-picked per acceptance amendment #22; NOT `Mul([Const(F64(-1.0)), x])`). Reasons: (a) preserves IEEE-754 signed-zero semantics under edge cases; (b) cleaner in serialization; (c) matches mental model "negate = subtract from zero". Test: `parse("-Spend") == Sub([Const(F64(0.0)), Ref("Spend")])`. If a real numerics test surfaces a case where Sub-desugaring fails under the kernel's NaN/null-poison propagation rules where Mul-desugaring wouldn't, write a SPEC QUESTION before flipping.
>    - Unary `+x` → just `x` (no-op)
>
> 6. **Diagnostic codes** (new — register in the diagnostic-code table):
>    - **MC1003** — unbalanced or unexpected parenthesis
>    - **MC1004** — unexpected token. **Per acceptance amendment #25, MC1004 ALSO covers "unknown function call" in Phase 3D** (e.g., `min(a, b)` when only `if_null` is recognized). Do NOT introduce MC1007 in Phase 3D. Document in the completion report's diagnostic-code registry: *"MC1004 covers both unexpected tokens AND unknown function calls in Phase 3D. If Phase 3E+ adds more functions to the formula grammar, MC1007 may be introduced as a separate 'unknown function' code for tighter UX. Until then, MC1004 is the catch-all."*
>    - **MC1005** — expected expression (e.g., formula ends with a trailing operator: `Spend +`)
>    - **MC1006** — invalid number literal (e.g., `1..5`, `1e`, `1.2.3`)
>    - All four are PARSE-TIME errors emitted before validate-stage logic runs. Identifier-resolution errors (formula references a measure that doesn't exist) continue to fire MC2003 — same diagnostic regardless of whether the rule body was authored as structured or formula form.
>    - Each code gets a span: `(line: usize, column: usize)` pointing into the YAML file at the formula string's location, plus an offset within the formula text.
>
>    **Required negative-test fixtures (per acceptance amendment to GPT #2):** explicit minimal YAMLs for each of these failure modes under `crates/mc-model/tests/formula_fixtures/`, each asserting the named MC1xxx code fires:
>    - `unknown_function.yaml` — `body: "min(Spend, CPC)"` → MC1004
>    - `wrong_if_null_arity.yaml` — `body: "if_null(Spend)"` (1 arg) and `body: "if_null(a, b, c)"` (3 args) → MC1004 (treated as malformed function call)
>    - `trailing_operator.yaml` — `body: "Spend +"` → MC1005
>    - `invalid_number.yaml` — `body: "1..5"` or `body: "1e"` → MC1006
>    - `unbalanced_parens.yaml` — `body: "(Spend / CPC"` (missing close) and `body: "Spend / CPC)"` (extra close) → MC1003
>
> 7. **Round-trip serialization** (`formula::serialize(&ParsedRuleBody) -> String`):
>    - `Add([a, b])` → `"<serialize(a)> + <serialize(b)>"` with parens around `a`/`b` if their root operator has lower precedence than `+`
>    - `Mul([a, b])` → `"<serialize(a)> * <serialize(b)>"` similarly with paren handling
>    - **`Const(F64(v))` → `v.to_string()`** (Rust's Ryu-based shortest-roundtrip formatting). **Per acceptance amendment #21, do NOT use `format!("{:.15}", v)` or any fixed-precision format** — `0.1_f64.to_string()` = `"0.1"` (what humans want); `format!("{:.15}", 0.1)` = `"0.100000000000000"` (ugly + breaks snapshot tests). Test: `serialize(Const(F64(1.5))) == "1.5"` and `serialize(Const(F64(0.1))) == "0.1"`.
>    - `Ref(s)` → `s.clone()`
>    - `IfNull([a, b])` → `"if_null(<serialize(a)>, <serialize(b)>)"`
>    - The output of `serialize(parse(s))` doesn't have to equal `s` byte-for-byte (whitespace differences allowed), but `parse(serialize(parse(s)))` MUST equal `parse(s)` (round-trip stable).
>
>    **Paren rule (binding contract, expanded per acceptance amendment #27):** when serializing a binary node `Op(left, right)`, parens are required around `left` (or `right`) when:
>    - The child's root operator has **strictly lower** precedence than `Op` (e.g., `Mul` wrapping `Add`).
>    - **The child is on the RIGHT side AND the child's root operator is at the SAME precedence as `Op` AND `Op` is non-associative on the right.** Specifically: subtraction (`Sub`) and division (`Div`) need parens around their right child if the right child is also at the `+/-` (for Sub) or `*//` (for Div) precedence level. Left-associative reading would otherwise re-parse to a different tree.
>    - **Critical case from GPT note #3:** `Mul([a, Div([b, c])])` must serialize to `"a * (b / c)"`, NOT `"a * b / c"` — the latter reparses left-to-right at same precedence as `Div([Mul([a, b]), c])` = `(a * b) / c`, a different AST.
>
>    **Round-trip is THE risky part — explicit gate (per GPT #5 + GPT note #3):** the round-trip test (`parse(serialize(parse(s))) == parse(s)`) MUST pass for every Acme rule AND for these specific edge-case shapes that historically break round-trip:
>    - **Subtraction associativity**: `Sub([a, Sub([b, c])])` must serialize to `"a - (b - c)"`, NOT `"a - b - c"` (which parses back to `Sub([Sub([a, b]), c])` — different tree).
>    - **Division associativity**: `Div([a, Div([b, c])])` must serialize to `"a / (b / c)"`, NOT `"a / b / c"`.
>    - **Mul with right-child Div** (per GPT note #3): `Mul([a, Div([b, c])])` must serialize to `"a * (b / c)"`, NOT `"a * b / c"`. Same hazard as Sub/Div associativity but easier to miss because `Mul` and `Div` are visually different operators with the same precedence.
>    - **Nested expressions**: `Mul([Add([a, b]), Sub([c, d])])` must serialize to `"(a + b) * (c - d)"` — both sides need parens because `+`/`-` < `*`.
>    - **Unary minus**: `Sub([Const(F64(0.0)), x])` must serialize to `"-<serialize(x)>"` (use the unary syntax, not the literal `0 - x` form). This is the canonical-form check that makes the parse(serialize(parse("-Spend"))) round-trip work.
>    - **Acme `Gross_Profit`**: `body: "Revenue * (1 - COGS_Rate)"` round-trips through Mul([Ref("Revenue"), Sub([Const(F64(1.0)), Ref("COGS_Rate")])]) and back to a string equivalent under parse equality (not necessarily byte-identical text).
>    - **Right-nested Add/Mul** (per GPT note #3): if exact AST round-trip is required for `Add([a, Add([b, c])])` (which is associative so semantically same as `Add([Add([a, b]), c])`), serialize as `"a + (b + c)"` to preserve the AST shape. If the parser canonicalizes to left-associative on parse anyway, document the canonicalization rule explicitly in the completion report.
>
>    Round-trip stability is a hard gate; if any of the above doesn't round-trip, that's a SPEC QUESTION (trigger #1 below).
>
> 8. **`mc model inspect` rendering** in `crates/mc-model/src/inspect.rs` — **uniform formula form is a HARD requirement** (per acceptance amendment #24):
>    - For each rule, show: `<rule_name>  <target_measure>  =  <body>`.
>    - **The body is ALWAYS rendered via `formula::serialize` regardless of authoring form.** The form a rule was AUTHORED in does NOT determine the form it's RENDERED in. This is a hard requirement, not implementer's choice.
>    - **Reason:** the entire UX point of Phase 3D is friendlier rule authoring. Mixed inspect output (some rules formulas, some trees) defeats that purpose for users who don't author the model themselves.
>    - **Snapshot fixture updates:**
>      - `crates/mc-model/tests/expected/inspect_acme.txt` — formula-form rendering for Acme's 5 rules (which were migrated to formula form per item 9).
>      - **Backwards-compat snapshot:** an inspect run on `_acme_with_bad_golden.yaml` (structured-authored) MUST show the rules in formula form too. Add a snapshot test asserting this.
>    - Future caveat: if Phase 3E+ adds an AST variant with no formula representation, that variant gets its own rendering rule and inspect's contract widens. Phase 3D's 7-variant AST is fully formula-renderable.
>
> 9. **Acme migration** in `crates/mc-model/examples/acme.yaml`:
>    - Convert all 5 rules from structured form to formula form:
>      ```yaml
>      rules:
>        - name: clicks_rule
>          target_measure: Clicks
>          scope: AllLeaves
>          description: "..."
>          body: "Spend / CPC"
>          declared_dependencies: ["Spend", "CPC"]
>        # ... 4 more ...
>      ```
>    - The `Gross_Profit` rule needs unary AND parens AND constant: `body: "Revenue * (1 - COGS_Rate)"`.
>    - All other YAML structure unchanged. The structural-equivalence test (Phase 3A), the demo-equivalence diff (Phase 3A), `mc model lint` (Phase 3B), and `mc model test` (Phase 3C) all stay green.
>
> 10. **Backwards compat** (mandatory):
>     - Existing structured-form YAMLs in `crates/mc-model/tests/lint_fixtures/` and `crates/mc-model/tests/fixture_validation_fixtures/` and `crates/mc-model/tests/lint_fixtures/_*.yaml` continue to load unchanged. Do NOT migrate test fixtures to formula form — they double as backwards-compat regression tests.
>     - At least one in-tree fixture explicitly uses the structured form (`_acme_with_bad_golden.yaml` is the natural candidate — it stays structured).
>
> **Hard rules:**
>
> - **`crates/mc-core/` is LOCKED.** No source change, no Cargo.toml change. `git diff phase-3c-fixtures-and-inputs -- crates/mc-core/` returns zero lines.
> - **`crates/mc-fixtures/` is LOCKED.** No source change. `git diff phase-3c-fixtures-and-inputs -- crates/mc-fixtures/` returns zero lines.
> - **`mc-cli` is essentially LOCKED for Phase 3D.** The CLI doesn't gain new flags or subcommands — `mc model {validate, inspect, lint, test}` and `mc demo` all behave identically. The only `mc-cli` touch allowed is if the inspect-render change in Phase 3D scope item 8 requires a small CLI plumbing change; if it does, the diff is < 10 lines.
> - **No new dependencies** in any crate. Hand-rolled parser only. No `pest`, no `nom`, no `lalrpop`, no parser-generator anything.
> - **The `ParsedRuleBody` enum (the AST) is NOT modified.** Same 7 variants. Formulas compile DOWN to it; no new AST node, no new variant, no new field on existing variants.
> - **The `Diagnostic` struct shape is NOT modified.** Adding new codes (MC1003–MC1006) is backwards-compatible; `schema_version` stays at `"1.0"` per ADR-0006 amendment #20.
> - **MC3008 stays permanently retired.** No new lint rule (Phase 3D adds parse-time codes, not lint codes).
> - **Toolchain stays at Rust 1.78.** No `cargo update`. No new dep that requires `edition2024`.
> - **No `unsafe`, no `async`, no `tokio`, no `rayon`, no threads.** Phase 3D is sync.
> - **All 328 existing tests must still pass.** New total ≥ 328 + (Phase 3D test count).
>
> **Acceptance gate (the headline + supporting):**
>
> Headline: **Acme's 5 rules in `crates/mc-model/examples/acme.yaml` are formula strings AND every existing gate still holds.**
>
> Concretely:
> 1. `crates/mc-model/examples/acme.yaml` `rules:` block uses `body: "<formula>"` form for all 5 rules.
> 2. The structural-equivalence test from Phase 3A still passes — the YAML-loaded cube structurally matches `build_acme_cube()`.
> 3. The demo-equivalence diff is still empty: `diff <(./target/release/mc demo) <(./target/release/mc demo --model crates/mc-model/examples/acme.yaml)` exits 0 with no output.
> 4. `mc model lint crates/mc-model/examples/acme.yaml` exits 0 with zero warnings (Phase 3B carry-forward).
> 5. `mc model test crates/mc-model/examples/acme.yaml` exits 0 with 9/9 goldens passing (Phase 3C carry-forward).
> 6. The Phase 3C equivalence test (`tests/equivalence_acme.rs`) still passes — Rust path vs YAML+CSV path byte-identical on 2,520 canonical inputs + 9 goldens.
> 7. `mc model inspect crates/mc-model/examples/acme.yaml` renders ALL rules in formula form (uniform per amendment #24). Snapshot fixture updates accordingly. **A second snapshot test** asserts inspect on `_acme_with_bad_golden.yaml` (structured-authored) ALSO renders rules in formula form — proving the rendering uniformity is independent of authoring form.
> 8. All four formula syntax error codes (MC1003–MC1006) have negative-test fixtures under `crates/mc-model/tests/formula_fixtures/` — at minimum the five fixtures listed in scope item 6 (`unknown_function.yaml`, `wrong_if_null_arity.yaml`, `trailing_operator.yaml`, `invalid_number.yaml`, `unbalanced_parens.yaml`) — each asserting exactly the named MC1xxx code fires.
> 9. **Round-trip stability test (the explicit risky-case list per GPT #5):** `formula::parse(formula::serialize(formula::parse(yaml_body)))` equals `formula::parse(yaml_body)` for:
>    - All 5 Acme rules (including `Gross_Profit`'s `"Revenue * (1 - COGS_Rate)"` — Mul wrapping Sub).
>    - Subtraction associativity edge case: `"a - (b - c)"` and `"(a - b) - c"` round-trip to different ASTs (don't collapse).
>    - Division associativity edge case: `"a / (b / c)"` and `"(a / b) / c"` round-trip to different ASTs.
>    - Nested expressions: `"(a + b) * (c - d)"` round-trips with all parens preserved.
>    - Unary minus: `"-Spend"` round-trips through `Sub([Const(F64(0.0)), Ref("Spend")])` and back to `"-Spend"` (canonical unary form, NOT `"0 - Spend"`).
> 10. **Backwards compat:** at least one structured-form YAML still loads correctly. The existing `_acme_with_bad_golden.yaml` is the canary — running it through `mc_model::parse` produces a `ParsedModel` that's structurally identical to what Phase 3C produces, modulo the new `ParsedRuleBodyForm::Structured(...)` wrapper. After validate, `ValidatedModel.rules[i].body` is `ParsedRuleBody` (flattened per amendment #23) — semantically identical to pre-Phase-3D output.
> 11. **YAML-null-body test (per acceptance amendment #26):** a fixture with `body: null` (or `body:` with no value) MUST fail with an existing MC2xxx schema error (likely MC2010 or MC2002), NOT a new formula error code. Prevents a future surprise where someone fixes serde dispatch and accidentally lets null bodies through to a formula parser that emits a confusing error.
> 12. **`mc-core` and `mc-fixtures` untouched.** Both diffs vs `phase-3c-fixtures-and-inputs` return zero lines.
> 13. All 328 existing tests still pass; new total ≥ 328 + Phase 3D additions.
> 14. JSON envelope `schema_version` stays at `"1.0"`. `tests/schema_stability.rs` still passes.
>
> **Validation gate before reporting done:**
>
> Run, in order:
> - `cargo fmt --check --all` (exit 0)
> - `cargo clippy --workspace --all-targets -- -D warnings` (exit 0)
> - `cargo build --release --workspace` (zero warnings)
> - `cargo test --workspace` (≥ 328 + new tests)
> - `cargo run --release --bin mc -- demo` (matches brief §4.6 — Rust path)
> - `cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml` (byte-identical to Rust path)
> - `cargo run --release --bin mc -- model validate crates/mc-model/examples/acme.yaml` (exits 0)
> - `cargo run --release --bin mc -- model inspect crates/mc-model/examples/acme.yaml` (renders formulas; snapshot updated)
> - `cargo run --release --bin mc -- model lint crates/mc-model/examples/acme.yaml` (exits 0; ZERO warnings)
> - `cargo run --release --bin mc -- model test crates/mc-model/examples/acme.yaml` (9/9 goldens pass)
> - 10 consecutive `cargo test --workspace -q` (deterministic)
> - `git diff phase-3c-fixtures-and-inputs -- crates/mc-core/ crates/mc-fixtures/` (zero lines)
>
> **Documentation requirements:**
> - Append `docs/reports/phase-3d-completion-report.md` per the [`../templates/phase-completion-report.md`](../templates/phase-completion-report.md) template.
> - Update [`../CURRENT_STATE.md`](../CURRENT_STATE.md) to flip Phase 3D from `proposed` → `complete`.
> - Update [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) Phase 3D status row.
> - Document the diagnostic-code registry update (MC1003–MC1006 added) in the completion report.
> - **Do NOT modify ADR-0004, ADR-0005, ADR-0006.** Accepted contracts.
> - **Do NOT modify ADR-0007** (the Phase 3D ADR drafted in parallel) once it lands. If you need to flag a strategic concern surfaced during implementation, write a SPEC QUESTION and pause.
> - **Do NOT modify the brief or engine-semantics doc.** Locked.
>
> **SPEC QUESTION triggers:**
>
> Open a SPEC QUESTION (per CLAUDE.md §11) before continuing if any of these surface:
> 1. The Acme `Gross_Profit` rule (`Revenue * (1 - COGS_Rate)`) doesn't round-trip cleanly through parse → AST → serialize → parse.
> 2. Backwards compat fails — a structured-form YAML in `tests/lint_fixtures/` or `tests/fixture_validation_fixtures/` doesn't load identically to before Phase 3D.
> 3. `serde_yaml`'s untagged enum dispatch picks `Structured` over `Formula` for some reason (e.g., string values that look like maps). The dispatch should be String first → Map second; if it isn't, surface and revise.
> 4. Round-trip serialization needs paren rules more complex than basic precedence (e.g., associativity quirks for `(a - b) - c` vs `a - (b - c)`). Pin the rule explicitly in the completion report.
> 5. The grammar surfaces a real ambiguity that the recursive-descent parser can't disambiguate (e.g., `if_null` colliding with a future measure named `if_null`).
> 6. The pre-picked unary `-x` desugaring (`Sub([Const(F64(0.0)), x])` per acceptance amendment #22) produces measurably different numerics under the kernel's NaN/null-poison rules vs `Mul([Const(F64(-1.0)), x])`. Surface the concrete failure case before flipping the desugaring choice.
> 7. The MC1003–MC1006 namespace is too coarse — a real error class doesn't fit any of the four. Propose MC1007+ if needed.
> 8. The inspect snapshot update produces something visually worse than the structured form for any rule (rare; flag if so).
>
> **Rollback plan (in case complexity explodes):**
>
> If the formula parser balloons beyond ~400 lines (the recursive-descent for the grammar above should be ~200–300 lines), or if round-trip serialization surfaces unexpected paren-handling complexity, **stop and write a SPEC QUESTION**. Two recovery paths:
> 1. **Narrow the grammar for Phase 3D.1**: drop unary, drop `if_null`, ship just `+ - * /` + parens + identifiers + numbers. Acme's 5 rules still all parse. Requires ADR amendment.
> 2. **Defer round-trip serialization to a future phase**: ship parse-only; inspect renders structured form for all rules. Requires ADR amendment.
>
> Either fallback is a Phase 3D.1 amendment, not a Phase 3D scope rewrite.
>
> **Completion report format:**
> ```
> DONE: Phase 3D Friendly Formula Syntax
>
> Build:    cargo build --release --workspace ✓
> Format:   cargo fmt --check --all ✓
> Lint:     cargo clippy --workspace --all-targets -- -D warnings ✓
> Tests:    cargo test --workspace [N] / 0 (was 328 / 0)
> Demo (Rust):     cargo run --release --bin mc -- demo ✓
> Demo (YAML):     cargo run --release --bin mc -- demo --model <acme.yaml> ✓ (Phase 3A diff still empty)
> Validate:        mc model validate <acme.yaml> ✓
> Inspect:         mc model inspect <acme.yaml> ✓ (formulas rendered; snapshot updated)
> Lint:            mc model lint <acme.yaml> ✓ (ZERO warnings — Phase 3B carry-forward)
> Test:            mc model test <acme.yaml> ✓ (9/9 goldens pass)
> Determinism:     10 / 10 identical
> Round-trip:      parse(serialize(parse(yaml))) == parse(yaml) for all 5 Acme rules ✓
> Backwards compat: structured-form fixtures still load identically ✓
> Locked surfaces: mc-core / mc-fixtures 0-line diff vs phase-3c-fixtures-and-inputs ✓
>
> Diagnostic-code registry update:
> - MC1001–MC1002: parse errors (Phase 3B; unchanged)
> - MC1003: NEW — formula unbalanced/unexpected paren
> - MC1004: NEW — formula unexpected token
> - MC1005: NEW — formula expected expression
> - MC1006: NEW — formula invalid number literal
> - MC2001–MC2025: validation errors (Phase 3A/3B/3C; unchanged)
> - MC3001–MC3007 + MC3009–MC3011: lint warnings (Phase 3B; unchanged)
> - MC3008: PERMANENTLY RETIRED (Phase 3B; assertion still passes)
>
> Source manifest:
> - crates/mc-model/src/formula.rs                      (NEW — recursive-descent parser + serializer, ~N lines)
> - crates/mc-model/src/schema.rs                       (modified — added ParsedRuleBodyForm enum wrapper)
> - crates/mc-model/src/validate.rs                     (modified — formula-parse step before existing validators)
> - crates/mc-model/src/error.rs                        (modified — MC1003–MC1006 ParseError variants + code() mapping)
> - crates/mc-model/src/inspect.rs                      (modified — render rules via formula::serialize)
> - crates/mc-model/src/lib.rs                          (modified — export formula module if public)
> - crates/mc-model/examples/acme.yaml                  (modified — 5 rules migrated to formula form)
> - crates/mc-model/tests/formula_parser.rs             (NEW — operator/precedence/paren/unary/identifier/number tests)
> - crates/mc-model/tests/formula_roundtrip.rs          (NEW — parse(serialize(parse(...))) stability)
> - crates/mc-model/tests/formula_validators.rs         (NEW — MC1003–MC1006 negative tests)
> - crates/mc-model/tests/formula_fixtures/             (NEW dir — 4 negative fixtures)
> - crates/mc-model/tests/backwards_compat.rs           (NEW — structured-form fixture still loads identically)
> - crates/mc-model/tests/expected/inspect_acme.txt     (modified — formula-form rendering)
> - docs/reports/phase-3d-completion-report.md          (NEW)
> - docs/CURRENT_STATE.md                               (updated)
> - docs/roadmap/MASTER_PHASE_PLAN.md                   (updated)
>
> Acme migration:
> - 5 rules converted to formula form
> - Gross_Profit uses unary + parens + numeric constant: "Revenue * (1 - COGS_Rate)"
> - Structural-equivalence test still passes
> - Demo-equivalence diff still empty
>
> Implementation summary:
> - <one paragraph: parser shape, precedence handling, unary desugaring choice, round-trip paren strategy>
>
> Deviations:
> - <list any; ideally empty>
> ```
>
> Do NOT commit or tag. The user reviews first.

---

## Context the prompt above does NOT spell out

These are landmarks the receiving instance will need.

### A. The existing AST you compile down to

[`crates/mc-model/src/schema.rs`](../../crates/mc-model/src/schema.rs) lines 158–223 defines `ParsedRuleBody` with 7 variants: `Const`, `Ref`, `Add`, `Sub`, `Mul`, `Div`, `IfNull`. **Phase 3D adds NO new variants.** Read the file before designing the parser; the AST shape is the contract.

The `ParsedScalar` type (lines ~225+) has `F64`, `I64`, `Bool` variants. Phase 3D's number literals all map to `F64`. `I64` and `Bool` are not reachable from formula syntax in Phase 3D.

### B. The recursive-descent parser pattern

```rust
// Pseudocode — actual implementation lives in crates/mc-model/src/formula.rs
struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn parse_expression(&mut self) -> Result<ParsedRuleBody, FormulaError> {
        let mut left = self.parse_term()?;
        while let Some(op) = self.peek_add_sub() {
            self.advance();
            let right = self.parse_term()?;
            left = match op {
                '+' => add(left, right),
                '-' => sub(left, right),
                _ => unreachable!(),
            };
        }
        Ok(left)
    }

    fn parse_term(&mut self) -> Result<ParsedRuleBody, FormulaError> {
        let mut left = self.parse_factor()?;
        while let Some(op) = self.peek_mul_div() {
            self.advance();
            let right = self.parse_factor()?;
            left = match op {
                '*' => mul(left, right),
                '/' => div(left, right),
                _ => unreachable!(),
            };
        }
        Ok(left)
    }

    fn parse_factor(&mut self) -> Result<ParsedRuleBody, FormulaError> {
        self.skip_whitespace();
        match self.peek() {
            Some('(') => { /* parse parenthesized expression */ }
            Some('+') | Some('-') => { /* parse unary */ }
            Some(c) if c.is_ascii_alphabetic() || c == '_' => { /* parse identifier or function call */ }
            Some(c) if c.is_ascii_digit() => { /* parse number */ }
            _ => Err(FormulaError::expected_expression(self.pos)),
        }
    }
}
```

Standard pattern. Track `pos` for error spans; `FormulaError` carries the position relative to the formula string start, which the validate-stage adapter converts to `(line, column)` in the YAML file.

### C. Round-trip paren handling

The serializer needs to add parens when a child's root operator has lower precedence than its parent's:

```
Add(Mul(a, b), c)        →  "a * b + c"        (no parens — Mul > Add)
Mul(Add(a, b), c)        →  "(a + b) * c"      (parens — Add < Mul)
Sub(a, Sub(b, c))        →  "a - (b - c)"      (parens for right associativity safety)
```

The Acme `Gross_Profit` rule serializes as `"Revenue * (1 - COGS_Rate)"` — Mul wraps Sub, so Sub gets parens.

Implementer's call on whether to be aggressive (always paren) or minimal (only when needed). Recommend minimal — produces cleaner output. The round-trip stability test catches mistakes.

### D. The `_acme_with_bad_golden.yaml` backwards-compat property

[`crates/mc-model/tests/lint_fixtures/_acme_with_bad_golden.yaml`](../../crates/mc-model/tests/lint_fixtures/_acme_with_bad_golden.yaml) was added in Phase 3C with structured-form rule bodies. Phase 3D does NOT migrate it. The Phase 3D `tests/backwards_compat.rs` test loads it and asserts the resulting `ParsedModel` is structurally identical to what Phase 3C produces (modulo the new `ParsedRuleBodyForm::Structured(...)` wrapper).

### E. The diagnostic-code namespace decision

Phase 3D extends MC1xxx (parse errors) with codes 1003–1006. This is consistent with the policy:

- MC1xxx — parse errors (text input is malformed; YAML or now formula)
- MC2xxx — validation errors (text parsed but model is structurally wrong)
- MC3xxx — lint warnings
- MC4xxx — reserved

Identifier resolution (`Spend` is parseable but doesn't reference a known measure) is a validation-stage concern, not a parse-stage concern. So `formula references unknown measure` reuses MC2003 (existing — "rule body references unknown measure"). The error message includes the formula context, but the code is unchanged.

If a future Phase needs new validation codes for formula-specific concerns (e.g., "formula has a circular dependency" — though Phase 3A's MC2008 already covers this), they get MC2026+.

---

## Pointers to existing files you will most likely touch

| Why | File | Action |
|---|---|---|
| Parser + serializer | `crates/mc-model/src/formula.rs` | new — ~250–350 lines hand-rolled recursive descent |
| Schema enum wrap | [`crates/mc-model/src/schema.rs`](../../crates/mc-model/src/schema.rs) | modify — add `ParsedRuleBodyForm` enum wrapping existing `ParsedRuleBody` for `ParsedRule.body` field |
| Validate-stage formula-parse step | [`crates/mc-model/src/validate.rs`](../../crates/mc-model/src/validate.rs) | modify — early in validation, parse `Formula(s)` variants into `Structured(_)` |
| New error variants + code mapping | [`crates/mc-model/src/error.rs`](../../crates/mc-model/src/error.rs) | modify — add MC1003–MC1006 ParseError variants + code() arms |
| Inspect rendering update | [`crates/mc-model/src/inspect.rs`](../../crates/mc-model/src/inspect.rs) | modify — render rules via `formula::serialize` |
| Public API surface | [`crates/mc-model/src/lib.rs`](../../crates/mc-model/src/lib.rs) | modify — declare `mod formula;` (private OR pub depending on whether tests call it directly) |
| The Acme YAML | [`crates/mc-model/examples/acme.yaml`](../../crates/mc-model/examples/acme.yaml) | modify — convert 5 rule bodies to formula form |
| Parser unit tests | `crates/mc-model/tests/formula_parser.rs` | new — operators, precedence, parens, unary, identifiers, numbers, edge cases |
| Round-trip stability | `crates/mc-model/tests/formula_roundtrip.rs` | new — `parse(serialize(parse(s))) == parse(s)` for Acme rules + edge cases |
| MC1003–MC1006 negative tests | `crates/mc-model/tests/formula_validators.rs` | new — one test per code |
| Negative fixture YAMLs | `crates/mc-model/tests/formula_fixtures/` | new dir — 4 minimal YAMLs |
| Backwards compat | `crates/mc-model/tests/backwards_compat.rs` | new — structured-form fixture loads identically |
| Snapshot fixture for inspect | [`crates/mc-model/tests/expected/inspect_acme.txt`](../../crates/mc-model/tests/expected/inspect_acme.txt) | modify — formula-form rendering for Acme's 5 rules |
| Phase 3D completion report | `docs/reports/phase-3d-completion-report.md` | new file (use [`../templates/phase-completion-report.md`](../templates/phase-completion-report.md)) |
| Status flips | [`../CURRENT_STATE.md`](../CURRENT_STATE.md), [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) | flip Phase 3D from `proposed` → `complete` |

**Do not touch:**

- **`crates/mc-core/`** — entire crate locked.
- **`crates/mc-fixtures/`** — entire crate locked.
- **`docs/specs/`** — locked.
- **`docs/decisions/0004-*` through `0006-*` (and 0007 once it lands)** — Accepted; amendments go in `0006-amendment-N.md` etc.
- **`rust-toolchain.toml`** — pinned at 1.78.
- **`Cargo.lock` (existing pins)** — `clap`, `clap_lex`, `half`, `indexmap`, `hashbrown` all stay.
- **PERF.md** — Phase 3D doesn't touch performance documentation. The kernel didn't change.
- **The `ParsedRuleBody` enum's variant set** — same 7 variants. Formulas compile DOWN to it.
- **The `Diagnostic` struct shape** — adding codes is fine; struct fields are not.
- **MC3008** — permanently retired.
- **`crates/mc-model/examples/acme.inputs.csv`** — Phase 3C deliverable; unchanged.
- **`crates/mc-model/tests/lint_fixtures/`** and **`crates/mc-model/tests/fixture_validation_fixtures/`** — these structured-form YAMLs double as backwards-compat tests; do NOT migrate to formula form.

---

## Reproducible commands you can rely on

```bash
cd /Users/edwinlovettiii/Projects/mc-v2

source $HOME/.cargo/env

# Pre-3D gate — must remain green throughout
cargo build --release --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                                               # 328 / 0
cargo run --release --bin mc -- demo
cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml
cargo run --release --bin mc -- model lint crates/mc-model/examples/acme.yaml    # zero warnings
cargo run --release --bin mc -- model test crates/mc-model/examples/acme.yaml    # 9/9 goldens

# Demo equivalence — must remain empty throughout
diff <(cargo run --release --bin mc -- demo) \
     <(cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml)
# expected: zero output

# Iteration loop:
cargo build -p mc-model
cargo test -p mc-model
cargo test -p mc-model -- formula_parser           # parser unit tests
cargo test -p mc-model -- formula_roundtrip        # parse(serialize(parse(...))) stability
cargo test -p mc-model -- formula_validators       # MC1003–MC1006 negative tests
cargo test -p mc-model -- backwards_compat         # structured-form still loads

# Verify locked surfaces:
git diff phase-3c-fixtures-and-inputs -- crates/mc-core/ crates/mc-fixtures/
# expected: zero output

# Determinism gate (10 runs, identical pass/fail):
for i in $(seq 1 10); do cargo test --workspace -q || echo "FAIL run $i"; done
```

---

## Final checklist before you call Phase 3D done

- [ ] `crates/mc-model/src/formula.rs` exists with `parse()` + `serialize()` public API.
- [ ] `crates/mc-model/src/schema.rs` adds `ParsedRuleBodyForm { Formula(String), Structured(ParsedRuleBody) }` wrapping the existing AST **in `ParsedModel` ONLY** (per amendment #23).
- [ ] **`ValidatedModel::rules[i].body: ParsedRuleBody`** (flattened, no enum wrapper). Validate stage unwraps `ParsedRuleBodyForm` AND parses `Formula(s)` into `Structured(_)` AND flattens to `ParsedRuleBody`. Downstream stages see no `ParsedRuleBodyForm`.
- [ ] All 4 formula syntax error codes (MC1003–MC1006) implemented with negative-test fixtures (5 fixtures per scope item 6).
- [ ] **MC1004 documented as the "unknown function" code in Phase 3D** (per amendment #25). MC1007 NOT introduced. Completion-report registry section explains the policy.
- [ ] **Unary minus desugars to `Sub([Const(F64(0.0)), x])`** (pre-picked per amendment #22). NOT `Mul([Const(F64(-1.0)), x])`.
- [ ] **Number literals serialize via `f64::to_string()` (Ryu shortest-roundtrip)** per amendment #21. NOT `format!("{:.15}", v)`. Unit tests assert `serialize(Const(F64(1.5))) == "1.5"` and `serialize(Const(F64(0.1))) == "0.1"`.
- [ ] Acme's 5 rules in `acme.yaml` are formula strings; all carry-forward gates still pass.
- [ ] Round-trip stability test passes for all 5 Acme rules + the explicit risky-case list (subtraction associativity, division associativity, nested expressions, unary minus, Gross_Profit).
- [ ] Backwards-compat test passes — at least one structured-form fixture (`_acme_with_bad_golden.yaml`) loads identically.
- [ ] **YAML-null-body test (per amendment #26)** — `body: null` fires existing MC2xxx schema error, NOT a new formula code.
- [ ] `mc model inspect` renders ALL rules in formula form (uniform per amendment #24). TWO snapshot fixtures: `inspect_acme.txt` (formula-authored) AND a new snapshot for `_acme_with_bad_golden.yaml` (structured-authored, also rendered as formulas).
- [ ] `ParsedRuleBody` enum variant set unchanged — same 7 variants.
- [ ] `Diagnostic` struct shape unchanged; `schema_version` stays at `"1.0"`.
- [ ] No new dependencies in any crate.
- [ ] `mc-core` Cargo.toml + src/ unchanged. `mc-fixtures` src/ + Cargo.toml unchanged.
- [ ] `rust-toolchain.toml` not bumped. Cargo.lock pins intact.
- [ ] No `unwrap()` / `expect()` / `panic!()` in `crates/mc-model/src/` (test/example/CLI exempt where the existing carve-out applies).
- [ ] No `unsafe`. No `async` / `tokio` / `rayon` / threads.
- [ ] All 328 existing tests still pass; new total ≥ 328 + Phase 3D additions.
- [ ] 10 consecutive `cargo test --workspace -q` runs identical.
- [ ] MC3008 still retired (assertion test passes).
- [ ] Completion report at `docs/reports/phase-3d-completion-report.md` written from template.
- [ ] CURRENT_STATE.md and MASTER_PHASE_PLAN.md updated to flip Phase 3D from `proposed` → `complete`.
- [ ] **You did NOT commit, tag, or push.** The user does that after reading the review.
- [ ] **You did NOT start Phase 4 (LLM authoring), Phase 5 (actuals), or Phase 6 (UI).**

If you are uncertain at any point, the resolution order is:

1. The Phase 3D prompt above.
2. **ADR-0007** (Phase 3D ADR) — being drafted in parallel; check if it's landed and read it. If not landed, this handoff is binding.
3. ADR-0004 / ADR-0005 / ADR-0006 — inherited contracts.
4. The brief and `engine-semantics.md`.
5. Phase 3C completion report (recent context).
6. Earlier completion reports.
7. `CLAUDE.md`.
8. `docs/roadmap/MASTER_PHASE_PLAN.md`.
9. Anything else.

If those don't resolve it: stop, write a SPEC QUESTION per CLAUDE.md §11, and wait. Don't guess.

---

## Operating principles (carry-forward from Phase 3A / 3B / 3C)

**Read this handoff fully before writing any code.** Phase 3D's contract is the body of this handoff. ADR-0007 is being drafted in parallel; if it lands during your work and contradicts this handoff, the ADR wins and this handoff gets revised — but in the meantime, this handoff is binding.

**Source-bounded to `crates/mc-model/`.** Phase 3D doesn't change the kernel, doesn't change the fixtures, doesn't add CLI commands. The new code is the formula parser/serializer + a schema enum wrap + a validate-stage step + the Acme migration + tests + snapshot updates.

**The acceptance gate is "all carry-forwards hold AND Acme is formula-authored."** Every gate from Phase 3A / 3B / 3C still passes; plus Acme's 5 rules are formula strings. If any carry-forward breaks, you don't ship.

**The AST is the contract.** Same 7 `ParsedRuleBody` variants. Formulas compile DOWN to it. No new variants, no new fields. The whole point of the structured-tree representation surviving is that Phase 4 (LLM authoring) consumes it as the canonical shape; Phase 3D just adds an alternate authoring surface.

**Diagnostic codes are forever.** MC1003–MC1006 ship with their declared meaning and never change. New codes go to MC1007+ (parse) or MC2026+ (validation) or MC3012+ (lint). MC3008 stays retired.

**Hand-rolled wins.** No `pest`, no `nom`, no `lalrpop`. The recursive-descent parser is ~250–350 lines for this grammar. Pulling in a parser library would add transitive deps and toolchain risk; the grammar is small enough not to warrant it.

**Backwards compat is a hard gate, not a nice-to-have.** Existing structured-form YAMLs (Phase 3A example, Phase 3B lint fixtures, Phase 3C validator fixtures) all load unchanged. The `_acme_with_bad_golden.yaml` backwards-compat property is the canary.

**Do not pick the next phase.** Phase 3D's deliverable is the formula parser + Acme migration. If the work surfaces opportunities for Phase 4 (LLM authoring), Phase 5 (actuals), or Phase 6 (UI), note them in the completion report's "follow-up candidates" section — do not start them.

---

*Phase 3D handoff drafted 2026-05-03 immediately after [Phase 3C](../reports/phase-3c-completion-report.md) shipped at `8d2691a`. Revised the same day with 11 acceptance amendments — GPT's 5 clarifications + Claude Desktop's 6 supplemental refinements (#21–26) — folded into the handoff body before kickoff. Per the new "handoff-first parallel flow" the project owner adopted **for this small phase only**, ADR-0007 is being drafted in parallel with this handoff; the ADR will land alongside implementation. If the ADR review surfaces a substantive change, the implementer will get a SPEC QUESTION; otherwise this handoff is the binding contract.*

**Process note (carry-forward — NOT a Phase 3D blocker):** the handoff-first parallel flow is appropriate for Phase 3D because the scope is small and well-implied by ADR-0004 / 0005 / 0006. **For Phase 4 (LLM authoring) onward, return to ADR-Accepted-then-handoff** — larger phases benefit from the strategic alignment a Proposed-Accepted ADR cycle forces before any handoff is written. This learning should land in `CLAUDE.md` or a project process-notes file so future phases don't drift to handoff-first as a default.
