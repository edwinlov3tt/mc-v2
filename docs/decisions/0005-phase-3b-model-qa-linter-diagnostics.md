# ADR-0005: Phase 3B — Model QA, Linter, and Diagnostics

**Status:** Accepted (with project-owner amendments — see "Acceptance amendments" section below)
**Date:** 2026-05-02 (Proposed); 2026-05-02 (Accepted, same day)
**Deciders:** project owner
**Phase:** 3B precondition (per [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md))

> Phase 3A shipped at `603c537` (tag `phase-3a-model-definition-layer`), giving the project the first deterministic, validated, YAML-authored cube definition. This ADR proposes Phase 3B as the next phase: a **read-only quality and diagnostics layer** over `mc-model` that makes authoring (human, and later LLM) safer *before* the harder downstream work begins. The Phase 3B handoff at [`../handoffs/phase-3b-handoff.md`](../handoffs/phase-3b-handoff.md) is the implementation contract; this ADR is the strategic context behind it.
>
> **Note on the ADR-0005 slot.** ADR-0004 Decision 3 reserved "ADR-0005" as the slot for a Rust toolchain bump if Phase 3A's parser dep required `edition2024`. Phase 3A shipped without needing the bump (used ADR-0004 Decision 3's transitive-pin escape hatch — `indexmap → 2.7.0`), so that slot was never used. Phase 3B inherits the next-available number; if/when a real toolchain bump is needed, it'll take whatever number is free at that point.

---

## Context

Phase 3A landed the parse → validate → compile pipeline. The deliverables today:

- `mc-model::load(&path) -> Result<Cube, Vec<Error>>` returns either a built cube or a list of errors with `file:line:column` context.
- `ParsedModel` and `ValidatedModel` are the LLM-ready intermediate types.
- 10 validators block malformed models with structured `ValidationError`s.
- Inline `golden_tests:` block in the YAML pins specific coordinate values.

What `mc-model` does NOT have yet:

- A way for an author to **inspect** a model — see at a glance "what's in this YAML, is it the shape I think it is?" Today's UX is "if `validate` returns Ok, the model is buildable; otherwise here's a list of errors." There's no positive-shape feedback.
- Any **quality signal** beyond "is the model buildable?" A model can be buildable AND clean OR buildable AND riddled with anti-patterns (no descriptions, ratio measures using `Sum`, orphan elements not in any hierarchy, etc.). Today there's no way to surface the second.
- A **stable diagnostic vocabulary** for LLM consumption. When Phase 4 lands and the LLM emits malformed YAML, the LLM needs structured codes + suggestions to iterate against. Today's errors are structured but not coded; lint warnings don't exist.
- A **CLI surface** for any of the above. `mc demo` runs the cube. There's no `mc model validate` / `mc model inspect` / `mc model lint`.

Phase 3B fills these four gaps **without changing the kernel, without adding a formula language, without LLM scaffolding, and without UI**. It's the smallest bridge between "Phase 3A made authoring possible" and the much bigger Phase 4 / 5 / 6 work that needs a stable diagnostic surface to build on.

The strategic argument for doing Phase 3B *next* (rather than jumping to Phase 3C friendly-formula syntax, Phase 4 LLM authoring, or Phase 5 actuals):

- **3B unblocks the others.** Every later phase (LLM authoring especially) consumes diagnostics. Phase 4 without stable codes = the LLM gets free-form error strings and can't reliably iterate. Phase 6 UI without inspect = the editor has nothing to render the model schema against. Phase 5 actuals import without lint = badly-shaped models silently produce wrong totals when actuals land.
- **3B is small.** Read-only, no kernel change, surface area bounded to a CLI subcommand surface + a lint module in `mc-model`. Estimated 1–2 days of focused work.
- **3B is reversible.** If the lint rules turn out to be wrong, the cost of removing/changing them is one revision; it doesn't lock in any kernel behavior or schema commitment.

This ADR scopes that bridge. The 9 decisions below are listed in dependency order.

---

## Decisions needed

### Decision 1: the four error categories — what do they mean and when do they fire?

**Question:** How are parse errors, validation errors, golden test failures, and lint warnings semantically distinct?

**Decision (Accepted):** Four strictness layers, each running at a different point in the pipeline, with different blocking semantics:

| Layer | When it runs | Blocks `load()`? | Blocks `mc demo`? | Origin | Example |
|---|---|---|---|---|---|
| **Parse error** | YAML deserialization (stage 1 of pipeline) | **Yes** | **Yes** | Bad YAML syntax | Missing colon, unexpected indent, malformed scalar |
| **Validation error** | `validate()` (stage 2) | **Yes** | **Yes** | Model is structurally wrong | Duplicate dim names, rule references unknown measure, hierarchy cycle, **weighted-average measure missing weight (MC2011 — promoted from lint per acceptance amendment #4)** |
| **Golden test failure** | `mc model test <path>`, OR `cargo test -p mc-model -- goldens` (NEVER `mc demo`) | No (load proceeds) | **No** — `mc demo --model` does NOT run goldens (per acceptance amendment #12) | Model loads but produces wrong values | Expected `Spend = 11500.0`, got `11400.0` (input was wrong, or kernel regressed) |
| **Lint warning** | `mc model lint` (Phase 3B addition) | No (`mc_model::load()` IGNORES lint output entirely) | No | Model loads + golden tests pass, but quality is off | Measure has no description; orphan element; ratio measure uses `Sum` aggregation |

**Why four levels, not three:** Phase 3A already shipped the first three (parse/validation/golden). Phase 3B adds the fourth (lint). Lint is *advisory* — it never blocks loading, never blocks running, never blocks `mc demo`, never blocks `mc model test`. It surfaces quality issues an author can choose to fix (or not). This matches the rustc/clippy/eslint pattern: hard errors block; lint advises.

**Separation of concerns (per acceptance amendment #12):** `mc demo` is for *running the cube*; its job is "make it go" — it loads, validates, prints brief §4.6 output, exits. `mc model test` is for *checking the cube against expected values*; its job is "did it match?" — it runs goldens and reports mismatches. Overloading `mc demo` with golden-test responsibility would mean CI scripts wanting to just run the demo would trip on golden failures unrelated to whether the demo executed correctly.

**Downstream:** every Phase 3B diagnostic is one of these four. The CLI surface (Decision 3) and the diagnostic structure (Decision 7) make the layer explicit so an author knows whether they have to fix something or just consider fixing it.

### Decision 2: should lint warnings block model loading?

**Question:** When `mc-model::load(&path)` runs, do lint warnings cause the load to fail?

**Decision (Accepted): No. Validation errors block loading; lint warnings are advisory.**

Rationale:

- Lint rules are *opinions* about quality, not statements about correctness. A model with no measure descriptions is uglier than one with descriptions, but it builds the same cube and produces the same numbers.
- Authoring is iterative. An author writing a model from scratch should be able to load it incrementally — get the structure working first, then fill in descriptions, then tighten up the aggregation choices. Blocking on lint kills that workflow.
- LLM iteration (Phase 4) needs the same property: an LLM should be able to emit a working model first, then refine. If lint blocks loading, the LLM gets a hard failure and has to fix everything before re-trying.

**Opt-in strict mode (CLI only):** add a `--deny-warnings` flag to `mc model lint` (mirroring `cargo clippy -- -D warnings`). For CI workflows where "no warnings" is enforced, the flag elevates lint warnings to errors at the *CLI exit-code level* — but `mc-model::load()` itself is unchanged. The flag has NO effect on any other CLI command (`validate`, `inspect`, `test`, `demo`); it is a `mc model lint` modifier exclusively.

**Hard rule (binding contract):** `mc_model::load()` IGNORES lint output entirely. Lint runs through a separate code path (`mc_model::lint(&ValidatedModel) -> Vec<Diagnostic>`) that callers invoke explicitly when they want lint feedback. The two concerns are decoupled at the library boundary so consumers (CLI, future UI, future LLM-authoring layer) can treat them independently. The implementer must NOT add a `lint_on_load: bool` flag or any similar coupling — the absence of coupling IS the contract.

**Downstream:** Phase 4's LLM-authoring loop runs `validate` and `lint` separately and can present each as a different category of feedback — *"these errors mean your model is broken; these warnings mean it's working but could be cleaner."*

### Decision 3: CLI commands

**Question:** What new CLI commands does Phase 3B add?

**Decision (Accepted):** Four new subcommands under a `mc model` group:

| Command | Purpose | Exit code |
|---|---|---|
| `mc model validate <path>` | Runs parse + validate. Prints any parse/validation errors with `file:line:column`. Silent on success. | `0` if model loads cleanly; non-zero if any parse or validation error |
| `mc model inspect <path>` | Runs parse + validate, then prints a structured summary (Decision 4). Errors printed alongside the summary if any. | `0` if model loads cleanly; non-zero if errors |
| `mc model lint <path>` | Runs parse + validate + lint. Prints lint warnings. | `0` always, unless `--deny-warnings` is set (then non-zero on any lint warning); non-zero if parse/validation errors regardless of flag |
| **`mc model test <path>`** *(per acceptance amendment #1)* | **Runs parse + validate + compile + executes inline `golden_tests:` block.** Prints which goldens passed and which failed (with expected vs actual values). | **`0` if model loads cleanly AND every golden passes; non-zero on any parse / validation / golden failure.** |

Plus a workhorse modifier:

- `--format text|json` — `text` is human-readable (default); `json` emits structured `Diagnostic[]` (or test results, for `mc model test`) for programmatic consumption (Phase 4 LLM, Phase 6 UI). Default is `text`.

**Naming convention:** `mc model <verb> <path>` matches the noun-verb style used by `kubectl`, `gh`, and other CLIs the user is likely already familiar with. Sub-grouping under `model` (rather than top-level `mc validate`, `mc inspect`, `mc lint`, `mc test`) keeps room for future `mc demo`, `mc bench`, etc. without name collisions.

**Separation of concerns (per acceptance amendment #12):** `mc model test` owns golden-test execution exclusively. `mc demo --model <path>` does NOT run goldens — it loads, validates, runs the cube, prints brief §4.6 output, and exits. CI scripts that just want "did the demo execute correctly?" use `mc demo`; scripts that want "did the model produce the right values?" use `mc model test`. The two responsibilities are independent.

**Out of scope for Phase 3B's CLI:**

- `mc model fix` (auto-apply lint suggestions). Save for a later phase; auto-fix is a complexity step that needs its own design.
- `mc model diff <a> <b>` (structural diff between two model files). Useful but separate scope; would belong in Phase 3C or a UI-adjacent phase.
- `mc model export` (`Cube → YAML` round-trip). Phase 3A is one-way; the reverse direction is a separate future phase.

**Downstream:** the CLI's text output is what most authors use day-to-day. The JSON output is what Phase 4 (LLM authoring) consumes for its feedback loop. Phase 6 (UI) reads the JSON to render diagnostics in the editor gutter.

### Decision 4: what `inspect` shows

**Question:** What fields does `mc model inspect <path>` print?

**Decision (Accepted):** A structured summary covering the model's high-level shape without dumping every element. Two views: the **default summary** (one screen) and the **`--verbose`** mode (everything).

**Default summary (one screen):**

```
Model: Acme_MarketingFinance (format v1)
  Description: Brief §4 reference cube
  Author: MarketingCubes V2
  Created: 2026-05-02

Dimensions: 6
  - Scenario  (Scenario)  — 3 elements (3 leaves, 0 consolidated)
  - Version   (Version)   — 3 elements (3 leaves, 0 consolidated)
  - Time      (Standard)  — 17 elements (12 leaves, 5 consolidated; 1 default hierarchy, depth 2)
  - Channel   (Standard)  — 8 elements  (5 leaves, 3 consolidated; 1 default hierarchy, depth 2)
  - Market    (Standard)  — 15 elements (7 leaves, 8 consolidated; 1 default hierarchy, depth 3)
  - Measure   (Measure)   — 11 elements (6 input, 5 derived)

Hierarchies: 3 (one per Time/Channel/Market)
  Time:    Mon → Quarter → FY              (12 leaves → 4 quarters → 1 FY)
  Channel: Channel → Group → All_Channels  (5 leaves → 2 groups → 1 root)
  Market:  City → State → Region → USA     (7 cities → 5 states → 2 regions → 1 USA)

Measures: 11
  Input:    Spend, CPC, CVR, Close_Rate, AOV, COGS_Rate
  Derived:  Clicks, Leads, Customers, Revenue, Gross_Profit
  Aggregations: Sum (5), WeightedAverage (6)

Rules: 5
  rule_clicks         Clicks         = Spend / CPC
  rule_leads          Leads          = Clicks * CVR
  rule_customers      Customers      = Leads * Close_Rate
  rule_revenue        Revenue        = Customers * AOV
  rule_gross_profit   Gross_Profit   = Revenue * (1 - COGS_Rate)
  Longest rule chain depth: 5 (Spend → Clicks → Leads → Customers → Revenue → Gross_Profit)

Cardinality (Cartesian product across all dim elements): 201,960 coords
  At Phase 2C scale curve: 1× = 201K, 10× ≈ 1M, 50× ≈ 4.8M, 100× ≈ 9.5M

Golden tests: 9 inline
  All anchored to brief §4.5.1.

Diagnostics: 0 errors, 0 warnings.
```

**Why this set:** the eight items the user requested (name/version, dim count, elements per dim, hierarchy summaries, measure counts by role, rules count, goldens count, errors/warnings summary) are all present, plus three additions that matter for the Phase 4/5 use cases:

- **Cardinality** — the same number ADR-0004 Decision 9 + Phase 2D handoff §B used; immediately tells an author whether their model would scale into the 50×/100× cliff regime.
- **Rule chain depth** — pre-warns about the deep-chain anti-pattern (cold consolidations get expensive at depth ≥ 5; ADR-0003 §6.10).
- **Aggregation distribution** — quick "how many measures use Sum vs WeightedAverage vs Min vs Max" at a glance; useful for spotting the "everything uses Sum, including the ratio measures that shouldn't" anti-pattern.

**`--verbose` mode** dumps every element (with id + name), every hierarchy edge, every rule body, and every golden test. Useful for debugging; not the default because it's a wall of text.

**Downstream:** the `inspect` output is the model's human-readable equivalent of `cargo metadata`. Future tooling (Phase 6 UI, Phase 4 LLM authoring) parses the JSON variant; humans read the text variant.

### Decision 5: lint rule starting set

**Question:** What lint rules ship in Phase 3B?

**Decision (Accepted):** the 10 starting rules listed below — **MC3008 retired** (per acceptance amendment #11; promoted to validation as MC2011 per acceptance amendment #4) and **naming-convention rule removed** (per acceptance amendment #5; deferred until the project commits to a concrete convention via a future style-guide phase). Each rule gets a **stable code** (Decision 7), a **default severity**, and a **suggestion** where one is applicable. The list is opinionated but not exhaustive — Phase 3B ships these 10; future sub-phases can add more.

| Code | Severity | Rule | What it catches | Suggestion |
|---|---|---|---|---|
| **MC3001** | warning | Missing description on dimension | A dimension has no `description:` field | Add a one-line description explaining what the dim represents |
| **MC3002** | warning | Missing description on measure | A measure has no `description:` field | Add a one-line description explaining what the measure represents and its unit |
| **MC3003** | warning | Missing description on rule | A rule has no `description:` field | Add a one-line description explaining the business meaning of the rule |
| **MC3004** | warning | Model has no golden tests | The `golden_tests:` block is empty or missing | Add at least one golden test pinning a known-good value (start with brief §4.5.1 anchors or equivalent) |
| **MC3005** | warning | Orphan element (not in default hierarchy) | An element exists in a dimension but is not a member of the default hierarchy (neither a leaf nor a consolidated node reached by edges) | Either add the element to the default hierarchy, or remove it if it's unused |
| **MC3006** | info | Long rule chain depth | A rule body is part of a chain ≥ 5 deep | Consider whether intermediate measures could be inlined or whether the chain reflects unnecessary indirection. Long chains increase model complexity (harder to reason about) and may incur measurable cold-derived-read cost (per PERF.md §6, cold derived reads scale roughly linearly with chain depth at ~600 ns/level on Acme — frame this as a complexity / explainability concern first, performance second) |
| **MC3007** | warning | Ratio measure using `Sum` aggregation | A measure with a name suggesting a ratio (`*_rate`, `*_ratio`, `*_pct`, `cpc`, `cvr`, `aov`, `cpa`, `roas`) is declared with `aggregation: Sum` | Ratios should typically use `WeightedAverage` (CPC weighted by Spend, CVR weighted by Clicks, etc.) — `Sum` produces meaningless values when consolidated. Verify the aggregation rule matches the measure's intent |
| ~~**MC3008**~~ | *(retired)* | *(retired — promoted to MC2011 in Phase 3B per acceptance amendment #4)* | *(formerly: weighted-average measure missing weight; now caught at validation as MC2011 — blocks loading rather than emitting a warning)* | *(see MC2011 below)* |
| **MC3009** | info | Unused input measure | An input measure is not referenced by any rule and not present in any golden test | If intentional, add a description noting it's a placeholder for future use; otherwise consider removing |
| **MC3010** | info | Unused derived measure | A derived measure is not referenced by any other rule and not present in any golden test | Same as MC3009 — likely safe to remove unless it's a top-level output the user reads directly |
| **MC3011** | warning | Hierarchy root ambiguity | A default hierarchy has multiple elements with no parent edge (i.e., multiple roots) | A hierarchy should typically have exactly one root (e.g., All_Channels, USA, FY). Multiple roots usually indicate missing edges or an unintended structural shape |

**MC3008 retirement is permanent (per acceptance amendment #11).** Do NOT renumber MC3009/MC3010/MC3011 down to MC3008/MC3009/MC3010. The MC3008 code-slot is permanently vacant. Stable diagnostic codes are the load-bearing piece for Phase 4 LLM consumption (Decision 7); reusing MC3008 for a different rule later would silently break any consumer pinned to a code-to-meaning map. CVE-style retirement is cheaper than reuse. **Implementation requirement:** the `mc-model::lint` module ships with an assertion (in `tests/`) that no active lint emits the code `"MC3008"`. The code is reserved-as-retired in the diagnostic-code registry.

**MC2011 (weighted-average missing weight) — promoted to validation per acceptance amendment #4.**

| Code | Severity | Rule | What it catches | Suggestion |
|---|---|---|---|---|
| **MC2011** | error (blocks loading) | Weighted-average measure missing weight measure | A measure declared `aggregation: WeightedAverage` does not declare a `weight_measure:` field | Add `weight_measure: <measure_name>` (e.g., CPC's weight is Spend) |

This is a *validation error* (MC2xxx), not a lint warning (MC3xxx). It runs in `mc_model::validate` between `ParsedModel` and `ValidatedModel`, blocks `mc_model::load()`, and surfaces with the same structured-error treatment as the other validators from ADR-0004 Decision 6. A weighted-average measure without a weight is structurally incorrect — the kernel cannot meaningfully compute consolidation; promoting to validation matches that severity.

**MC3007's heuristic (name-based ratio detection):** intentionally fuzzy. The rule fires on a name pattern, not a structural check, because the model has no first-class "ratio" concept. False positives are possible (a measure named `customer_score_rate` that genuinely sums); the suggestion text says "verify the aggregation rule matches the measure's intent" rather than "you definitely got this wrong." Future phases (Phase 3C friendly formulas?) might add a structural way to declare a ratio, at which point MC3007's heuristic can be replaced with a structural check.

**MC3006's framing (per acceptance amendment #8):** model-complexity/explainability is the primary concern; performance is the secondary concern, cited honestly. Long rule chains are harder for humans (and LLMs) to reason about — that's the load-bearing argument for the lint. The performance citation is real but secondary: PERF.md §6 shows cold derived reads scale roughly linearly with chain depth (~600 ns/level on Acme), but at typical scales the perf cost is dwarfed by other factors. The lint message should lead with "consider whether intermediate measures could be inlined" (a complexity argument), then mention perf as supporting evidence.

**Severity defaults:** most are `warning`. Two are `info` (long chain, unused measures — these are stylistic, not bugs). The hard-error case (formerly MC3008) is now MC2011 in the validator.

**Naming-convention lint deferred (per acceptance amendment #5).** No naming-convention rule ships in Phase 3B. The reserved slot is dropped from this ADR. When the project commits to a concrete naming convention (probably in a future style-guide phase), a follow-on ADR or amendment proposes the corresponding lint rules with stable codes from the next-available MC3xxx range (i.e., MC3012 onward, NOT reusing MC3008).

**Downstream:** future Phase 3B.1 / 3B.2 sub-phases (or a Phase 3D linter expansion) add more rules using new codes from MC3012+. The 10-rule starting set is the floor, not a ceiling.

### Decision 6: explicitly out of scope

**Question:** What is *not* Phase 3B?

**Decision (Accepted):** the following are out of scope for Phase 3B. Each is named here so the implementer can't rationalize "while we're at it…":

| Out of scope | Phase | Notes |
|---|---|---|
| **Formula strings** (`Revenue = Customers * AOV`) | Phase 3C | Per ADR-0004 Decision 4 |
| **Custom DSL** | Future, if ever | Per ADR-0004 Decision 1; Phase 3B is YAML-only |
| **LLM authoring** | Phase 4 | Phase 3B's diagnostics are designed *for* Phase 4 to consume, but Phase 3B doesn't ship any LLM scaffolding |
| **UI editor** | Phase 6 | Phase 3B's `--format json` is designed *for* Phase 6 to consume, but Phase 3B doesn't ship any UI |
| **Actuals import** (CSV / API) | Phase 5 | Phase 3B is read-only model analysis; no data loading |
| **DuckDB / external storage** | Phase 5+ | `HashMapStore` remains the only store |
| **Multi-cube models** | Future Phase 3 sub-phase | Per ADR-0004 Decision 5; one cube per file |
| **`mc-core` changes** | Future, if ever needed | Phase 3B is read-only over `mc-model`; the kernel is locked |
| **Cube → YAML round-trip** | Future Phase 3 sub-phase | Phase 3A is one-way; reverse direction is its own scope |
| **Auto-fix (`mc model fix`)** | Future, post-Phase-3B | Auto-apply suggestions is its own complexity; not Phase 3B |
| **Snapshot diff (`mc model diff a b`)** | Future | Useful but separate scope |

**Hard rule:** no source change in `crates/mc-core/`. No new dep in `mc-core`. No change to `mc-fixtures::build_acme_cube()`. The Phase 2D / Phase 3A locks stay locked.

**Downstream:** the Phase 3B handoff opens with this list as a visible "do not touch."

### Decision 7: diagnostic structure for Phase 4 LLM feedback

**Question:** How should diagnostics be structured so Phase 4 (LLM authoring) can consume them effectively?

**Decision (Accepted):** a stable, structured `Diagnostic` type with five fields, emittable as JSON via the `--format json` CLI flag, wrapped in a versioned envelope (per acceptance amendment #13), with a deterministic emission order (per acceptance amendment #14).

```rust
pub struct Diagnostic {
    pub code: DiagnosticCode,            // stable, e.g., "MC3001"
    pub severity: Severity,              // Error | Warning | Info
    pub path: ModelPath,                 // structured pointer into the YAML model
    pub message: String,                 // one-line human-readable summary
    pub suggestion: Option<String>,      // optional actionable fix
}

pub enum Severity { Error, Warning, Info }

pub struct ModelPath {
    pub file: PathBuf,                       // path to the YAML file
    pub span: Option<Span>,                  // line:column where supported
    pub yaml_pointer: String,                // RFC-6901-style JSON pointer into the parsed YAML
                                             //   e.g., "/measures/3/aggregation"
    pub model_path: String,                  // model-aware path
                                             //   e.g., "measures.CPC.aggregation"
}
```

**Stable diagnostic codes** (the load-bearing piece for Phase 4): every parse error, validation error, and lint rule has a code that does not change across releases. Codes are namespaced:

- **MC1xxx** — parse errors (currently 1 logical category — "YAML syntax invalid"; codes assigned at handoff time)
- **MC2xxx** — validation errors. Phase 3A's Decision-6 validators get codes MC2001–MC2010 (ten codes for the ten validators). **MC2011** (weighted-average measure missing weight) added in Phase 3B per acceptance amendment #4.
- **MC3xxx** — lint warnings. Phase 3B ships 10 rules: MC3001–MC3007, MC3009–MC3011. **MC3008 is permanently retired (per acceptance amendment #11)** — promoted to MC2011 in Phase 3B; the code-slot stays vacant; future lint rules use MC3012+.
- **MC4xxx** — reserved for future categories (e.g., performance hints, security warnings).

**JSON envelope (binding contract — per acceptance amendment #13):** the `--format json` output is wrapped in a versioned envelope so downstream consumers (Phase 4 LLM scaffolding, Phase 6 UI editor) can pin to a known schema:

```json
{
  "schema_version": "1.0",
  "diagnostics": [
    {
      "code": "MC3001",
      "severity": "Warning",
      "path": {
        "file": "crates/mc-model/examples/acme.yaml",
        "span": {"line": 47, "column": 5},
        "yaml_pointer": "/dimensions/2",
        "model_path": "dimensions.Time"
      },
      "message": "Dimension 'Time' has no description",
      "suggestion": "Add a one-line description explaining what the dim represents"
    }
  ]
}
```

The `schema_version` field is **mandatory and unconditional** — it appears in every JSON emission, including empty-diagnostic cases (`{"schema_version": "1.0", "diagnostics": []}`). Phase 3B ships at `"1.0"`. Any breaking change to the diagnostic shape (renaming a field, changing a severity enum variant, etc.) bumps the version. Phase 4 LLM scaffolding and Phase 6 UI both pin to `"1.0"` when they consume this output. **Implementation requirement:** a JSON fixture under `crates/mc-model/tests/expected/` asserts the `schema_version` field is present and equals `"1.0"`.

**Deterministic emission order (binding contract — per acceptance amendment #14):** diagnostics are sorted by the following total ordering BEFORE either the text or JSON formatter runs:

1. **`severity` desc** — errors first, then warnings, then info
2. **`code` asc** — within a severity, MC2001 before MC2002 before MC3001 before MC3009
3. **`yaml_pointer` asc** — within a code, earlier nodes before later nodes
4. **`message` asc** — within a yaml_pointer, lexicographic message order (final tiebreaker)

This ordering applies uniformly to every emission path (`mc model validate`, `mc model lint`, `mc model inspect`'s diagnostics section, JSON output, programmatic `Vec<Diagnostic>` returned from library APIs). **Why specify it here:** lint rules iterating over `HashMap` will produce non-deterministic order otherwise, which makes snapshot tests (per Decision 8) flake. Naming the contract here means Decision 8's determinism gate is satisfiable by construction. **Implementation requirement:** a fixture that triggers ≥ 3 diagnostics asserts byte-exact output across 10 consecutive runs.

**Why two paths (yaml_pointer + model_path):** the `yaml_pointer` is mechanical and derives directly from the parsed YAML structure (RFC 6901 standard, future UI-friendly). The `model_path` is human-friendly and derives from the validated model (resolves indices to names — "measures.CPC.aggregation" instead of "/measures/3/aggregation"). LLMs benefit from the model_path because it speaks in the language of the schema, not the language of array indices.

**Why severity is an enum, not just a string:** machine-comparable. CI tools can filter "show me only errors" or "show me errors and warnings but not info" without parsing strings.

**Why suggestion is optional:** not every diagnostic has an actionable fix. "Hierarchy cycle detected at A→B→C→A" doesn't have a one-liner suggestion — the user has to think about which edge to remove. Forcing a suggestion field would produce empty or unhelpful filler.

**Downstream:** Phase 4's prompt scaffolding consumes `--format json` output, pins to `schema_version: "1.0"`, and iterates the diagnostics back to the LLM. Phase 6's UI editor parses the same JSON. The text format (default for human-facing CLI) is rendered from the same sorted `Vec<Diagnostic>` via a `Display` impl — so text and JSON never disagree on what fired or in what order.

### Decision 8: success gate

**Question:** What does Phase 3B "complete" mean?

**Decision (Accepted):** Phase 3B is complete when **all** of the following hold:

1. **Acme validates clean.** `mc model validate crates/mc-model/examples/acme.yaml` exits 0; no parse or validation errors.
2. **Acme lints clean with ZERO warnings (per acceptance amendment #15).** `mc model lint crates/mc-model/examples/acme.yaml` produces zero diagnostics — no `--allow` flags, no documented exceptions. The escape hatch from the original draft ("acceptable warnings if documented") is closed: `examples/acme.yaml` is the project's gold-standard reference; if Acme has known lint warnings, every future model author copy-pastes those warnings forward. **Lint-trap demonstrations belong in `crates/mc-model/tests/lint_fixtures/MC30xx_demonstration.yaml`, NOT in `examples/acme.yaml`.** If the project owner explicitly wants Acme to demonstrate a lint trap (e.g., MC3007 ratio-Sum), the success gate gets re-opened in writing — not implicitly via "documented exceptions."
3. **Intentionally-flawed model fixtures trigger each lint.** `crates/mc-model/tests/lint_fixtures/` contains one minimal YAML file per lint rule (MC3001, MC3002, MC3003, MC3004, MC3005, MC3006, MC3007, MC3009, MC3010, MC3011 — note MC3008 is intentionally absent per amendment #11), each crafted to trigger exactly that rule. A test asserts each rule fires on its corresponding fixture and no other rule fires spuriously.
4. **MC3008 retirement assertion (per acceptance amendment #11).** A test in `crates/mc-model/tests/` asserts that no active lint rule emits the diagnostic code `"MC3008"`. The code is permanently reserved-as-retired; future lint additions use MC3012+.
5. **MC2011 (weighted-average missing weight) blocks loading.** A negative test fixture with a `WeightedAverage` measure missing `weight_measure:` causes `mc_model::load()` to return `Err`, with the error including code `"MC2011"`.
6. **CLI output is stable and snapshot-testable.** Use **hand-rolled snapshot fixture comparison** (per acceptance amendment #7) to lock the text-format output of `mc model inspect crates/mc-model/examples/acme.yaml` and `mc model lint <each-lint-fixture>`. Phase 3B's UI is the CLI output; treating it as a contract guards against accidental regressions in the text format. **`insta` is NOT pulled in by default** — see Decision 9 + the next note for the policy.
7. **JSON output envelope assertion (per acceptance amendment #13).** A JSON fixture under `crates/mc-model/tests/expected/` asserts:
   - The envelope has `schema_version: "1.0"` (mandatory, including in empty-diagnostic cases).
   - The envelope has a `diagnostics` array.
   - Each diagnostic has `code`, `severity`, `path`, `message`, `suggestion` fields per Decision 7's struct shape.
8. **Deterministic emission order (per acceptance amendment #14).** A fixture that triggers ≥ 3 diagnostics asserts byte-exact output across 10 consecutive runs. The test must verify the sort order is `(severity desc, code asc, yaml_pointer asc, message asc)`.
9. **`mc demo --model` does NOT run goldens (per acceptance amendment #12).** An integration test asserts `mc demo --model <fixture-with-bad-goldens>.yaml` exits 0 — i.e., the demo runs successfully even when the model's golden tests would fail. Goldens are exclusively `mc model test`'s responsibility.
10. **All existing 252 tests still pass.** `cargo test --workspace` ≥ 252 + (new lint + validator + CLI + snapshot test count). New tests are additive.
11. **`mc-core` untouched.** `git diff phase-3a-model-definition-layer -- crates/mc-core/` returns zero lines.
12. **`mc-fixtures` untouched.** `git diff phase-3a-model-definition-layer -- crates/mc-fixtures/src/ crates/mc-fixtures/Cargo.toml` returns zero lines.
13. **Determinism gate holds.** 10 consecutive `cargo test --workspace -q` runs identical, including the new lint, validator, snapshot, and JSON envelope tests.
14. **All four CLI commands work end-to-end.** `mc model validate`, `mc model inspect`, `mc model lint`, `mc model test` each demonstrably run on the Acme YAML and on at least one negative fixture.
15. **`mc model test crates/mc-model/examples/acme.yaml` exits 0** with all 9 inline goldens passing (carryover from Phase 3A's golden-test count).

Phase 3B does NOT need to flip Phase 3A's tag, change PERF.md, or modify any spec doc. The kernel is locked; the model schema is locked; the canonical Acme model definition is locked except for any cleanup needed to satisfy gate #2 (and any such cleanup must be `WritebackResult.invalidated`-style: same field types and structure, only the *contents* of `description:` fields change).

**Snapshot-test policy (per acceptance amendment #7):** **prefer hand-rolled snapshot fixture comparison over `insta`.** A small `assert_snapshot(actual: &str, fixture_path: &Path)` helper in `crates/mc-model/tests/` reading expected output from `tests/expected/<test-name>.txt` and producing a clean diff on mismatch is sufficient. `insta` is allowed ONLY if the Phase 3B handoff requires Claude Code to prove it builds on Rust 1.78 without any toolchain bump or transitive-dep churn — and even then, it's a workspace dev-dependency only (NEVER in `mc-core`). The hand-rolled approach avoids the dep entirely and is the default expectation; `insta` is the escape hatch, not the recipe.

### Decision 9: Rust toolchain bump

**Question:** Does Phase 3B require a Rust toolchain bump?

**Decision (Accepted): No.**

Rationale:

- Phase 3A shipped on Rust 1.78 with the `indexmap → 2.7.0` transitive pin (ADR-0004 Decision 3 escape hatch). That pin stays.
- Phase 3B's only new functional surface is in `mc-model` (lint module + CLI subcommands in `mc-cli`). No new heavy parsing dep is needed; `serde_yaml 0.9.34` already on the workspace handles everything Decision 4's `inspect` needs. JSON output (Decision 7) reuses `serde_json` if it lands transitively — `serde_yaml` already pulls `serde`, so a small `serde_json` add (or hand-rolling the JSON formatter, since the envelope shape is fixed) is the only new dep candidate.
- Snapshot testing (per Decisions 8 + amendment #7): **default is hand-rolled fixture comparison** (no new dep). `insta` is allowed only as an escape hatch, and only if Claude Code proves it builds cleanly on Rust 1.78 with no transitive churn (per acceptance amendment #7). The hand-rolled path is preferred precisely because it's zero-dep.

If a Phase 3B sub-task surfaces a bona fide need for a toolchain bump, **stop and write a SPEC QUESTION**. The decision doesn't belong in Phase 3B's scope — it belongs in its own ADR.

**Downstream:** the Phase 3B handoff inherits the same "no toolchain bump" rule that ADR-0004 Decision 3 established for Phase 3A. The escape hatches (transitive pinning, library swap, hand-rolled implementation) are tried before any toolchain decision.

---

## What this unlocks

Phase 3B's deliverable is the diagnostic + inspection foundation for:

- **Phase 3C — Friendly formula syntax.** When formula strings (`Revenue = Customers * AOV`) compile down to structured trees, the lint surface from Phase 3B tells the formula compiler what counts as a stylistic warning vs a hard error. The diagnostic codes carry across.
- **Phase 4 — LLM-assisted authoring.** This is the load-bearing consumer. Phase 4's iteration loop is *parse → validate → lint → re-prompt the LLM with structured feedback*. Without Phase 3B's stable codes + JSON output, the LLM has nothing actionable to iterate on.
- **Phase 5 — Actuals / data integration.** Loading actuals into a badly-shaped model (e.g., MC3007 ratio-with-Sum-aggregation) produces silently wrong consolidated values. Phase 3B's lint catches the shape problem *before* actuals land.
- **Phase 6 — UI editor.** The `--format json` Diagnostic stream is what the editor renders in the gutter. The `inspect` summary is what the editor renders as the model's "overview" panel.

Every phase after 3B has a hook into 3B's diagnostic surface. Doing 3B *next* — before Phase 3C, before Phase 4, before Phase 5/6 — is the strongest leverage move because it's the smallest piece of work that unblocks the largest amount of downstream work.

---

## Accepted decisions — TL;DR

Phase 3B ships against:

1. **Four-layer error stack:** parse < validation < golden < lint, with parse + validation blocking, golden blocking only on `mc model test`, lint always advisory. `mc demo --model` does NOT run goldens (Decision 1, per acceptance amendment #12).
2. **Lint warnings do NOT block load.** `mc_model::load()` IGNORES lint output entirely (binding contract); opt-in `--deny-warnings` flag elevates lint to non-zero exit code at the `mc model lint` CLI layer only (Decision 2).
3. **Four new CLI commands:** `mc model validate`, `mc model inspect`, `mc model lint`, **`mc model test`** (the fourth, per acceptance amendment #1) — plus a `--format text|json` modifier (Decision 3).
4. **`inspect` shows** the user's 8 requested fields plus cardinality, longest rule chain depth, and aggregation distribution (Decision 4).
5. **10 starting lint rules** (MC3001–MC3007, MC3009–MC3011) covering descriptions, golden tests, orphan elements, rule chain depth (framed as model-complexity primary, performance secondary per amendment #8), ratio-Sum trap, unused measures, and hierarchy root ambiguity. **MC3008 is permanently retired** (per amendment #11; promoted to MC2011 as a validation error per amendment #4). **Naming-convention rule deferred** to a future style-guide phase (per amendment #5). One promoted validator: **MC2011** (weighted-average measure missing weight) blocks loading (Decision 5).
6. **Strict out-of-scope:** no formula strings, no LLM authoring, no UI, no actuals, no `mc-core` changes, no auto-fix (Decision 6).
7. **Stable diagnostic codes** (MC1xxx parse, MC2xxx validation incl. MC2011, MC3xxx lint with MC3008 retired, MC4xxx reserved) with structured `{ code, severity, path, message, suggestion }` shape. **JSON output wrapped in versioned envelope `{ "schema_version": "1.0", "diagnostics": [...] }`** (per amendment #13). **Deterministic emission order** sorted by `(severity desc, code asc, yaml_pointer asc, message asc)` (per amendment #14). JSON-emittable for Phase 4 / 6 consumption (Decision 7).
8. **Fifteen-item success gate** including Acme cleanly lints with **ZERO documented warnings** (escape hatch closed per amendment #15), intentionally-flawed fixtures trigger each rule, MC3008-retired assertion, MC2011-blocks-loading test, hand-rolled snapshot fixtures lock CLI output, JSON envelope schema_version assertion, deterministic emission test, demo-without-goldens test, ≥ 252 tests still pass, `mc-core` and `mc-fixtures` untouched (Decision 8).
9. **No Rust toolchain bump.** Hand-rolled snapshot comparison preferred over `insta`; `insta` allowed only as escape hatch with proof of Rust 1.78 compatibility (Decisions 8 + 9, per amendment #7).

The Acme YAML stays the canonical example. Phase 3B may add `description:` fields and any other minimal cleanup needed to make `mc model lint acme.yaml` exit 0 with zero warnings.

---

## Acceptance amendments

This ADR was Proposed and Accepted on 2026-05-02 with project-owner amendments on top of the proposed defaults. Two reviews contributed: GPT (10 amendments) and Claude Desktop (5 supplemental amendments). All 15 are recorded here for audit trail; the decisions above already reflect the final shape.

| # | Source | Amendment (one-line) | Where it landed in the ADR |
|---|---|---|---|
| 1 | GPT | Add a fourth CLI command: `mc model test <path>` for parse + validate + compile + golden execution. Don't overload `mc demo --model` with golden-test responsibility. | Decision 3 (new fourth row in CLI table) + Decision 1 (golden-failure layer) |
| 2 | GPT | Confirm the three pre-existing CLI commands (`validate` / `inspect` / `lint`) plus `--format text\|json`. | Decision 3 (unchanged from proposal; explicit confirmation) |
| 3 | GPT | Confirm lint warnings are advisory by default. `mc_model::load()` MUST ignore lint output. `--deny-warnings` only affects `mc model lint` CLI exit code. | Decision 2 (strengthened to a "binding contract" with explicit "no `lint_on_load` flag" rule) |
| 4 | GPT | Move weighted-average-missing-weight out of lint into validation. Treat as MC2xxx with blocking semantics. | Decision 5 (MC3008 retired, new MC2011 row in the validators table; Decision 1's validation row updated) |
| 5 | GPT | Remove the reserved naming-convention lint from Phase 3B unless a concrete naming convention is defined now. Defer to a later style-guide phase. | Decision 5 (reserved row dropped; explicit deferral note) |
| 6 | GPT | Use a concrete starting lint set of 10 rules (no naming-convention placeholder). | Decision 5 (table updated to the concrete 10 rules) |
| 7 | GPT | Prefer hand-rolled snapshot fixture comparisons over `insta`. Allow `insta` only if Phase 3B handoff requires Claude Code to prove it works on Rust 1.78 without toolchain bump or dep churn. | Decision 8 (success gate rule 6) + Decision 9 (toolchain rationale) |
| 8 | GPT | Soften MC3006's performance citation. Frame as model-complexity / explainability primary, performance secondary, with honest PERF.md §6 citation (cold derived reads scale with chain depth at ~600 ns/level). | Decision 5 (MC3006 row + dedicated paragraph) |
| 9 | GPT | JSON diagnostics in scope for Phase 3B. Diagnostic shape: code, severity, path, message, suggestion. Include enough structure for future LLM feedback. | Decision 7 (struct definitions + Phase 4/6 downstream rationale) |
| 10 | GPT | After edits, flip ADR-0005 to Accepted and draft `docs/handoffs/phase-3b-handoff.md`. Do not start implementation. | Status flip (this section) + Phase 3B handoff drafted at acceptance |
| 11 | Desktop | Do NOT renumber MC3009/MC3010/MC3011 down after MC3008 promotion. Leave MC3008 slot permanently vacant; document as "retired — promoted to MC2011 in Phase 3B." | Decision 5 (MC3008 row marked retired; dedicated paragraph + implementation requirement for assertion) + Decision 7 (code-namespace table notes MC3008 retired) + Decision 8 (gate rule 4 — MC3008-retired assertion) |
| 12 | Desktop | `mc demo --model <path>` must NOT run goldens. Demo loads, validates, runs the cube, prints brief §4.6 output, exits. Goldens are exclusively `mc model test`'s responsibility. | Decision 1 (golden-failure row updated; new "separation of concerns" paragraph) + Decision 3 (separation-of-concerns paragraph) + Decision 8 (gate rule 9 — demo-without-goldens test) |
| 13 | Desktop | JSON output envelope must include a `schema_version` field. Format: `{ "schema_version": "1.0", "diagnostics": [...] }`. | Decision 7 (envelope shape with worked example) + Decision 8 (gate rule 7 — envelope assertion) |
| 14 | Desktop | Diagnostic emission order must be deterministic and specified. Sort by `(severity desc, code asc, yaml_pointer asc, message asc)` before formatting. | Decision 7 (deterministic ordering paragraph) + Decision 8 (gate rule 8 — deterministic-emission assertion) |
| 15 | Desktop | Acme must lint clean with zero documented warnings. Close the success-gate escape hatch. Lint-trap demonstrations belong in `tests/lint_fixtures/`, not `examples/acme.yaml`. | Decision 8 (gate rule 2 rewritten; escape hatch closed) |

No remaining open questions. Phase 3B handoff at [`../handoffs/phase-3b-handoff.md`](../handoffs/phase-3b-handoff.md) is the implementation contract; this ADR is the strategic context behind it.

---

## Alternatives considered (whole-ADR scope)

1. **Skip Phase 3B; jump to Phase 3C friendly-formula syntax.** Rejected — Phase 4 (LLM authoring) needs the diagnostic surface more than Phase 3C does. 3C's value is "humans write nicer rules"; 3B's value is "every later phase can consume structured feedback."
2. **Skip Phase 3B; jump to Phase 4 LLM authoring.** Rejected — Phase 4 without 3B's stable codes + JSON output produces an LLM iteration loop that consumes free-form error strings. Worse outcomes per LLM call; costlier; harder to debug.
3. **Skip Phase 3B; jump to Phase 5 actuals import.** Rejected — actuals into a badly-shaped model (e.g., MC3007) silently produce wrong totals. Phase 3B's lint catches that shape problem before any real data lands.
4. **Bundle Phase 3B and Phase 3C together.** Rejected — Phase 3C (friendly formula syntax) requires a formula parser, which is its own substantial chunk of work. Bundling expands Phase 3B from "1–2 days" to "a week+", making it harder to ship cleanly.
5. **Defer Phase 3B until Phase 4/5/6 surfaces concrete needs.** Rejected — every concrete need from those phases (LLM consuming codes, UI rendering diagnostics, actuals validation) traces back to Phase 3B. Building 3B on demand means each later phase blocks until 3B's relevant slice ships, which serializes the phase plan.
6. **Make Phase 3B a feature flag inside `mc-model` rather than a CLI surface.** Rejected — the CLI is what humans use day-to-day. A library-only Phase 3B would be invisible to authors; that's the wrong shape for a quality / UX phase.
7. **Make every lint a hard error.** Rejected — see Decision 2. Hard-error lints kill iteration; the project's pace depends on incremental authoring.

---

## Cross-links

- [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) — Phase 3B is currently absent from the plan; this ADR proposes adding it as the next sub-phase after Phase 3A (which shipped at `603c537`).
- [`../CURRENT_STATE.md`](../CURRENT_STATE.md) — Phase status; will be updated to add Phase 3B once this ADR is Accepted.
- [`0004-phase-3a-model-definition-format.md`](0004-phase-3a-model-definition-format.md) — Phase 3A ADR; this ADR builds directly on the `ParsedModel` / `ValidatedModel` / `ValidationError` types it shipped.
- [`0003-workload-sketch.md`](0003-workload-sketch.md) — workload sketch; MC3006 (long rule chain depth) cites §6.10 (per-mark cost analysis).
- [`0002-perf-assertions-in-benchmarks-not-tests.md`](0002-perf-assertions-in-benchmarks-not-tests.md) — Phase 3B has no perf claims, so this ADR doesn't apply directly, but the Decision-8 snapshot-test discipline is in the same spirit (assert behavior contracts at the appropriate test layer).
- [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) §1 — out-of-scope list; Phase 3B touches none of the items there.
- [`../../CLAUDE.md`](../../CLAUDE.md) §1 — allowed runtime deps; Phase 3B inherits the ban (no new `mc-core` deps).

---

## Notes

This ADR is the strategic gate for Phase 3B the way ADR-0003 was for Phase 2 and ADR-0004 was for Phase 3A. It scopes the next sub-phase once so the Phase 3B handoff can be a build contract rather than a debate.

If this ADR is amended after Acceptance, the amendment lands as `0005-amendment-N.md` (append-only, mirroring the ADR-0003 + ADR-0004 pattern).

**Phase 3B is *next-after-3A* in the proposed order**, but the master plan currently does not have a `proposed` row queued — Phase 3A's completion left the queue empty. Accepting this ADR would queue Phase 3B as `proposed`. If the project owner instead wants Phase 3C, Phase 4, Phase 5, or Phase 6 to go next, this ADR is rejected and a different ADR proposes the alternative sequence.
