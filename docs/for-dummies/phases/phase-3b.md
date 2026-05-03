# Phase 3B — For Dummies

> **In one line:** Phase 3A made it possible to author cubes in YAML. Phase 3B added the four mental verbs every YAML author needs — *check it, look at it, critique it, run it* — plus a stable diagnostic vocabulary so future LLMs and UIs can speak the same error language.

> **Shipped 2026-05-03** at commit `f4f7fa8`, tag `phase-3b-lint-and-diagnostics`. See [completion report](../../reports/phase-3b-completion-report.md) for the full audit.

[Technical version → handoff](../../handoffs/phase-3b-handoff.md) · [ADR-0005](../../decisions/0005-phase-3b-model-qa-linter-diagnostics.md) · [completion report](../../reports/phase-3b-completion-report.md)

---

## The analogy: spell-check, grammar-check, style-check, run

You write an essay in a word processor. The word processor gives you four kinds of feedback:

- **Spell-check** — "the word `recieve` doesn't exist." This is *blocking* in the sense that nobody wants to ship typos. It corresponds to **parse errors** in MarketingCubes.
- **Grammar-check** — "this sentence has no verb." Also blocking — broken grammar means the sentence can't be parsed by a reader. Corresponds to **validation errors**.
- **Style-check** — "this sentence is in passive voice; consider active." Advisory. The essay still works; you might want to clean it up. Corresponds to **lint warnings**.
- **Read it aloud** — "does it sound right?" The final check that the essay actually says what you meant. Corresponds to **golden tests** — does the model produce the values you expected?

Phase 3B is the four-verb word processor for YAML cube models. Before Phase 3B, you had spell-check + grammar-check (parse + validate from Phase 3A) and "Save the file and run the demo" (a single hammer for everything). After Phase 3B, you have all four verbs as separate, named CLI commands.

## What Phase 3B actually shipped

Five concrete pieces of work:

**(1) Four CLI commands.** Each is one verb on the model file:

```bash
mc model validate <path>     # spell + grammar check (parse + validate)
mc model inspect  <path>     # show me the model's shape at-a-glance
mc model lint     <path>     # style critique (10 rules)
mc model test     <path>     # run the inline goldens, see if values match
```

Each command exits 0 on success, non-zero on failure. CI scripts can pipe their output into JSON via `--format json`.

**(2) Ten lint rules.** Each gets a stable code (MC3001 through MC3011, with MC3008 deliberately retired — see below):

- MC3001/02/03 — "your dimension/measure/rule has no description" (warning)
- MC3004 — "your model has no golden tests" (warning)
- MC3005 — "you declared an element but never put it in a hierarchy" (warning)
- MC3006 — "your rule chain is more than 5 levels deep" (info — long chains are hard to reason about)
- MC3007 — "this looks like a ratio measure but you're summing it" (warning — `Sum` of CPCs is meaningless)
- MC3009/10 — "this measure isn't used by any rule or golden test" (info)
- MC3011 — "your hierarchy has multiple roots" (warning — usually unintended)

These are *advisory*. They never block loading. You can author a malformed-but-still-buildable model, see the warnings, choose which to fix.

**(3) One validation error promotion.** A measure declared `WeightedAverage` without a `weight_measure:` field is now a hard error (MC2011) — it blocks loading. Originally this was a lint warning (MC3008), but the project owner correctly observed that a weighted average without a weight is structurally broken — the kernel can't meaningfully compute it. So MC3008 got promoted to a real validation error and **MC3008 the slot is permanently retired**. Future lint rules use MC3012 onward.

The retirement-not-renumbering pattern matters: if some external tool (a future LLM, a CI dashboard, a documentation page) had pinned to the meaning of `MC3008`, silently reusing the code for a different rule would break that consumer. Stable codes are forever. Retired codes are forever.

**(4) Stable structured diagnostics.** Every diagnostic that any of the four commands emits has the same shape:

```
{
  "code": "MC3001",                            // stable across releases
  "severity": "Warning",                        // Error | Warning | Info
  "path": {
    "file": "crates/mc-model/examples/acme.yaml",
    "yaml_pointer": "/dimensions/2",            // mechanical pointer
    "model_path": "dimensions.Time"             // human-friendly pointer
  },
  "message": "Dimension 'Time' has no description",
  "suggestion": "Add a one-line description explaining what the dim represents"
}
```

The JSON output is wrapped in a versioned envelope: `{ "schema_version": "1.0", "diagnostics": [...] }`. Phase 4 (LLM authoring) and Phase 6 (UI editor) both pin to schema version `"1.0"` when they consume this. Any breaking change to the shape would bump the version; today, version `"1.0"` is the contract.

Diagnostics come out in a deterministic order: errors first, then warnings, then info; within a severity, alphabetical by code; within a code, by location in the file. **Same input → byte-identical output, every run.** This makes snapshot tests and LLM iteration loops both possible.

**(5) Acme YAML cleanup to demonstrate "the right way."** Acme is the canonical example everyone copy-pastes from. Before Phase 3B, Acme had no descriptions on any dim/measure/rule — fine for the demo, but it would have shown 22 lint warnings. Phase 3B added 22 short `description:` fields to Acme so `mc model lint acme.yaml` exits 0 with **zero** warnings. The structural shape of Acme didn't change; only the metadata.

The bar that closed: every future model author opens `examples/acme.yaml`, copies its shape, and inherits its quality. If Acme had known warnings, every author would copy those warnings forward. Phase 3B closed that escape hatch.

## What's in `mc model inspect`

Run it on the Acme YAML and you get something like:

```
Model: Acme_MarketingFinance (format v1)
  Description: The brief §4 reference cube for end-to-end testing.
  Author: MarketingCubes V2
  Created: 2026-05-02

Dimensions: 6
  - Scenario  (Scenario)  — 3 elements (3 leaves, 0 consolidated)
  - Version   (Version)   — 3 elements (3 leaves, 0 consolidated)
  - Time      (Standard)  — 17 elements (12 leaves, 5 consolidated; 1 hierarchy, depth 2)
  - Channel   (Standard)  — 8 elements  (5 leaves, 3 consolidated; 1 hierarchy, depth 2)
  - Market    (Standard)  — 15 elements (7 leaves, 8 consolidated; 1 hierarchy, depth 3)
  - Measure   (Measure)   — 11 elements (6 input, 5 derived)

Cardinality (Cartesian product): 201,960 coords
Longest rule chain: 5 deep (Spend → Clicks → Leads → Customers → Revenue → Gross_Profit)
Goldens: 9 inline
Diagnostics: 0 errors, 0 warnings.
```

This is the "model overview at a glance." You see the shape of the cube without reading 264 lines of YAML. If you're handed an unfamiliar model, `mc model inspect` is your first stop.

## Why we care / what would have gone wrong without it

Three things would have broken if Phase 3B didn't ship:

**(1) Phase 4 (LLM authoring) would have free-form errors to iterate against.** Imagine an LLM emits a YAML with a typo. The system says: `"Error: cannot build cube"`. The LLM has no idea what to fix. With Phase 3B's stable codes + JSON envelope, the system says: `{"code": "MC2003", "path": "rules.rule_clicks.body", "message": "Rule body references unknown measure 'Spnd'", "suggestion": "Did you mean 'Spend'?"}`. The LLM has a code to look up, a location to point to, and a suggestion to apply. **One round-trip vs ten.** Phase 4's success rate is roughly proportional to how well-structured Phase 3B's diagnostics are.

**(2) Phase 6 (UI editor) would have nothing to render in the gutter.** A web editor needs structured diagnostics to highlight the right line, color it the right severity, show a tooltip with the suggestion. Free-form error strings can't drive that UI. Phase 3B's `Diagnostic` shape is the contract the UI will render against.

**(3) Quality would silently rot.** Without lint, a planning team would author a model where every measure has aggregation `Sum` (because Sum is the easiest), and the consolidated values for ratio measures (CPC, CVR) would silently produce garbage. By the time anyone notices, the bad pattern is everywhere. MC3007 catches this on day one: *"This measure looks like a ratio but you're summing it. Did you mean WeightedAverage?"*

## One thing that's easy to get wrong

The biggest temptation when adding a feature like "snapshot tests for CLI output" is to reach for a popular crate like `insta`. Phase 3B deliberately did NOT pull in `insta`. Instead, the implementer wrote a 30-line `assert_snapshot()` helper that compares actual output against expected text in `tests/expected/`. That's it.

Why? Two reasons:

- **Minimum dep churn.** Every dependency added is a future maintenance burden — version bumps, transitive vulns, build-time cost, possible toolchain conflicts (Phase 3A almost had to bump Rust 1.78 → 1.85 over an `indexmap` transitive). The hand-rolled snapshot helper has zero of those costs.
- **The snapshot mechanism is trivial.** "Compare two strings; print a diff if they differ" is a few lines of code. Pulling in a crate to do it is over-engineering.

The other thing easy to misread is **what `mc demo` and `mc model test` do differently**. They look similar — both load a YAML, both run the cube. But:

- `mc demo --model <path>` is "run the cube and print the brief §4.6 demo output." Its job is to show that the cube *works*. It does NOT run goldens. If the YAML's goldens are wrong, `mc demo` still exits 0.
- `mc model test <path>` is "run the cube and check the inline goldens against actual values." Its job is to show that the cube produces the *right* numbers. If goldens fail, this exits non-zero.

CI scripts that just want "did the demo execute correctly?" use `mc demo`. CI scripts that want "did the model produce expected values?" use `mc model test`. Bundling them would mean every demo-run script trips on golden mismatches that may be unrelated.

## What Phase 3B is and isn't

| It is | It isn't |
|---|---|
| A read-only quality + diagnostics layer over `mc-model` | Any change to the kernel or fixtures (both untouched, 0 lines diff vs Phase 3A) |
| Four CLI verbs (validate / inspect / lint / test) | A formula language (`Revenue = Customers * AOV` — that's Phase 3D) |
| 10 lint rules + 1 promoted validator | LLM authoring (Phase 4) |
| Stable diagnostic codes + JSON envelope for downstream consumers | A web UI (Phase 6) |
| Hand-rolled snapshot tests; no `insta` | An auto-fix tool (`mc model fix` — possible future, not Phase 3B) |
| MC3008 permanently retired (CVE-style retirement) | Reusable code slots — once a code is taken or retired, it stays that way |

## How long it took

Roughly one focused day. The biggest pieces:

- The lint module: ~430 lines for 10 rules + the sort/format machinery
- The diagnostic types: ~380 lines for `Diagnostic`, `Severity`, `ModelPath`, JSON envelope (hand-rolled)
- The inspect module: ~480 lines for the structured summary
- The CLI subcommand routing: ~130 lines added to `mc-cli/src/main.rs`
- The Acme YAML cleanup: 22 short description fields

Plus 41 new tests (5 new test files + 14 snapshot fixtures + 11 lint-rule fixtures). Test count went from 252/0 → **293/0**.

The implementer flagged 8 small deviations in the completion report — all minor, all rationalized. The most interesting one to keep an eye on for next phase: `mc model test` currently has a special case where it loads canonical inputs only when `metadata.name == "Acme_MarketingFinance"`. That's a known-temporary scaffold — Phase 3C is being scoped specifically to remove it (model files will declare their own input fixtures, and the special case goes away).

---

*Tied to: [phase-3a.md](./phase-3a.md) (the previous phase, which made YAML authoring possible), [`../research-notes/totals-vs-formulas.md`](../research-notes/totals-vs-formulas.md) (touches the same "what's the difference between a model and a calculation?" mental model).*
