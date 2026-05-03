---
name: mosaic-formulas
description: Phase 3D friendly formula syntax for Mosaic rule bodies — supported operators (+ - * /, parens, unary +/-), the single supported function (if_null), identifier rules (case-sensitive measure names), number literals (F64; integer auto-promotion; scientific notation), unary-minus desugaring (Sub([Const(0.0), x])), and the four parse-error codes MC1003-MC1006. Use whenever the user is writing a rule body, asks why a formula doesn't parse, or wants to know what operators are available.
---

# Phase 3D Friendly Formula Syntax

Phase 3D added formula strings as an alternative to the structured rule-body tree. Both forms are accepted indefinitely; pick whichever is clearer for the rule.

```yaml
# Formula form (recommended for human authors):
- target_measure: "Revenue"
  body: "Customers * AOV"
  declared_dependencies: ["Customers", "AOV"]

# Structured form (still supported):
- target_measure: "Revenue"
  body:
    mul:
      - { ref: "Customers" }
      - { ref: "AOV" }
  declared_dependencies: ["Customers", "AOV"]
```

Both compile to the same AST and produce the same cube behavior.

## Grammar (Phase 3D)

```
expr        ::= term (('+' | '-') term)*
term        ::= factor (('*' | '/') factor)*
factor      ::= unary | primary
unary       ::= ('+' | '-') factor
primary     ::= number | ref | call | '(' expr ')'
ref         ::= identifier
call        ::= 'if_null' '(' expr ',' expr ')'
identifier  ::= [A-Za-z_][A-Za-z0-9_]*
number      ::= F64 literal (decimal, scientific, no underscores, no hex)
```

That's the whole grammar. There are exactly **two function calls** allowed (`if_null` is the only registered name); see "Functions" below. Anything else is an unknown identifier or syntax error.

## Operators (in precedence order, highest first)

1. **Parentheses** `( ... )` — explicit grouping.
2. **Unary `+`** / **unary `-`** — at start of expression or after `(`.
3. **Multiplication `*`** / **Division `/`** — left-associative.
4. **Addition `+`** / **Subtraction `-`** — left-associative.

Examples:

```
Spend / CPC                            # division
Spend * (1 - COGS_Rate)                # multiplication, parens, subtraction
-Spend                                  # unary minus
Spend + Clicks * CPC                    # multiplication binds tighter than addition
(Spend + Clicks) * CPC                  # parens override
```

## Identifiers (measure references)

- Identifiers are **case-sensitive measure names**. `Spend` ≠ `spend` ≠ `SPEND`.
- They must match a measure declared in `measures:` (or fire MC2005 at validation).
- Mosaic does not have keywords reserved beyond `if_null`; `min`, `max`, `if`, `null`, `true`, `false` are all parsed as identifiers (and fire MC2005 if no such measure exists).

## Number literals

- F64 (double-precision float). Integer literals like `1000` auto-promote.
- **No underscores:** `1_000` fires MC1006. Write `1000`.
- **No hex:** `0x1A` fires MC1006.
- **Scientific notation OK:** `1.5e-3`, `2E10`.
- **No leading dot:** `.5` may not parse — write `0.5`.
- Negative numbers: write as unary minus over a positive (`-3.0` parses as `Sub([Const(0.0), Const(3.0)])`).

## Unary minus desugaring (binding)

`-x` desugars to `Sub([Const(F64(0.0)), x])` per ADR-0007 amendment #22. This is a parse-time transform; in the AST you'll see a `Sub` node, not a dedicated `Neg`.

```
Input:    -Spend
AST:      Sub
            Const(0.0)
            Ref(Spend)
```

This matters when reading inspect output or debugging a rule's structure: `inspect` re-renders rules using minimal-paren round-trip, and `-Spend` round-trips back to `(0 - Spend)` or similar — that's expected, not a bug.

## Functions

**Phase 3D supports exactly one function:** `if_null(primary, fallback)`.

```yaml
- target_measure: "Best_Estimate"
  body: "if_null(Actual, Forecast)"
  declared_dependencies: ["Actual", "Forecast"]
```

Semantics: if `primary` evaluates to `Null` (e.g., a missing input cell, or a derived cell whose dependency was Null), emit `fallback`. Otherwise emit `primary`.

**Anything else — `min`, `max`, `if`, `case`, `coalesce`, `iif`, `switch` — fires MC1004** ("unexpected token OR unknown function" per ADR-0007 amendment #25; both meanings collapsed into one code in Phase 3D).

If you find yourself wanting `min(a, b)`, restructure: either make it a separate input with the cap pre-applied, or split into multiple rules each handling part of the case.

## Parser round-trip (the inspect layer)

`mc model inspect` uses a minimal-paren serializer to render rules in formula form regardless of how they were authored. Inspect output is canonical: a structured-form rule rendered through inspect comes out as the equivalent formula string.

This means you can author in either form, then re-author from inspect output without changing semantics. The serializer rules:

- Adds parens only when precedence requires them (`Mul([a, Div([b, c])])` → `"a * (b / c)"`).
- Doesn't add parens around left-associative chains (`Add([a, b, c])` → `"a + b + c"`).
- Renders unary minus as `"(0 - x)"` since it desugared to that AST shape.

Per ADR-0007 amendment #27, the parens around `(b / c)` in `a * (b / c)` are deliberate: the AST `Mul([a, Div([b, c])])` is **not** equivalent to `Mul([a, b]) / c` (left-to-right multiplication × division), so the parens preserve the structure.

## Diagnostic codes (Phase 3D parse layer)

All four MC1xxx codes for formulas:

| Code | Fires |
|---|---|
| **MC1003** | Unbalanced parens, OR a paren in an unexpected position |
| **MC1004** | Unexpected token (stray `.`, `,`, `=`, etc.), OR unknown function call (anything other than `if_null`) |
| **MC1005** | Expected an expression but didn't find one (trailing operator, leading binary operator, two operators in a row) |
| **MC1006** | Number literal can't be parsed as F64 |

For the full diagnostic-code registry and fix patterns, see `skills/debugging/SKILL.md`.

## Acme rule examples

The 5 Acme rules in formula form:

```yaml
- name: "rule_clicks"
  body: "Spend / CPC"
  declared_dependencies: ["Spend", "CPC"]

- name: "rule_leads"
  body: "Clicks * CVR"
  declared_dependencies: ["Clicks", "CVR"]

- name: "rule_customers"
  body: "Leads * Close_Rate"
  declared_dependencies: ["Leads", "Close_Rate"]

- name: "rule_revenue"
  body: "Customers * AOV"
  declared_dependencies: ["Customers", "AOV"]

- name: "rule_gross_profit"
  body: "Revenue * (1 - COGS_Rate)"
  declared_dependencies: ["Revenue", "COGS_Rate"]
```

## When to author structured form instead

- When generating rules programmatically (a tool emits trees, not strings).
- When you need to assert AST structure in a test (a structured fixture is byte-equal across runs).
- When commenting individual sub-expressions matters and YAML inline comments aren't enough.

The structured form maps 1:1 to formula form. There's no "advanced" feature in structured form that formula doesn't support — the AST is the same shape.

## Anti-patterns (DON'T)

- **Don't try `min`, `max`, `if`, comparisons, string ops, or any non-listed operator.** They all fire MC1004. The supported set is fixed in Phase 3D; new operators require a future ADR.
- **Don't use number-literal underscores.** `1_000` fires MC1006. Write `1000`.
- **Don't case-vary measure names.** `spend` ≠ `Spend`. Use the exact name from `measures:`.
- **Don't omit declared_dependencies.** The runtime kernel rejects undeclared reads. Listing them in the YAML is mandatory.
- **Don't expect `if_null` to do more than one fallback.** It's binary: `if_null(primary, fallback)`. For multi-step fallback chains, nest: `if_null(A, if_null(B, C))`.
