# ADR-0004: Phase 3A model-definition format & parser scope

**Status:** Accepted (with project-owner amendments — see "Acceptance amendments" section below)
**Date:** 2026-05-02 (Proposed); 2026-05-02 (Accepted, same day)
**Deciders:** project owner
**Phase:** 3A precondition (per [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md))

> Phase 2D shipped at `0678a98` and closed PERF.md §9.3. This ADR is the gating Phase 3A artifact and is now **Accepted**, unblocking Phase 3A. The Phase 3A handoff at [`../handoffs/phase-3a-handoff.md`](../handoffs/phase-3a-handoff.md) is the implementation contract; this ADR is the strategic context behind it.

---

## Context

Today, MarketingCubes cubes are authored by writing Rust against `mc-core`'s builder API (see [`crates/mc-fixtures/src/lib.rs`](../../crates/mc-fixtures/src/lib.rs) — `build_acme_cube` is ~700 lines of `Dimension::builder().with_element(...).build()` chains). That works for the single in-tree fixture and is exactly the right shape for Phase 1, but it does not scale to:

- An internal user authoring a planning model without engineering help (Phase 6's "first usable product" requirement #1).
- An LLM authoring or editing models from a natural-language prompt (Phase 4).
- A web UI rendering a model schema for editing (Phase 6).
- A round-trippable `model.<ext>` file an analyst can edit, version-control, diff, review.

Phase 3A introduces the **model-definition layer** — a declarative format that compiles down to the same `mc-core` builder calls `mc-fixtures` makes today, with no kernel semantics change.

The decision being made now is *not* "should we build a model layer" — that's already on the master phase plan. The decision is **what shape the layer takes**, and which scope cliffs Phase 3A draws so the work is one shippable phase rather than a four-quarter epic.

This ADR is a strategic gate, not an implementation spec. The implementation spec lands in `phase-3a-handoff.md` after this is Accepted.

---

## Decisions needed

The eight decisions below are listed in dependency order — answering #1 informs #2, etc. Each has a question, my recommendation as a starting default, the alternatives I considered, and the downstream effect of the answer.

### Decision 1: file format

**Question:** What syntax should the Phase 3A model file use?

**Decision (Accepted):** **YAML, restricted to a documented safe subset.**

| Format | Pros | Cons | Verdict |
|---|---|---|---|
| **YAML** | Human-readable; multi-line strings without escaping; comments; widely understood; LLMs emit clean YAML; Rust support via `serde_yaml`/`serde_yml` is mature | Indentation is significant (whitespace bugs); type coercion surprises (`yes`/`no`/`on`/`off` → bool; unquoted versions parse as floats); many "valid" YAMLs aren't what the author meant | **Accepted** with the safe-subset rules below |
| **TOML** | No whitespace ambiguity; type-strict; clean for flat config | Awkward for nested/hierarchical structures (dimensions → elements → hierarchies → edges); array-of-tables syntax becomes unreadable past two levels; LLMs emit it less reliably | Rejected — model definitions are inherently nested; TOML's strength is flat config |
| **JSON** | Universally supported; no whitespace bugs; strict types | No comments; awkward for human authoring; quote-noise everywhere; LLMs sometimes emit JSON with trailing commas / single quotes / comments that aren't valid JSON | Rejected — UX cost of no comments + quote-noise outweighs the strictness benefit |
| **Custom DSL** | Could express domain semantics directly (e.g. `Revenue = Customers * AOV` as a first-class line) | Requires a parser, a grammar, error-message infrastructure, an editor mode, and migration when the grammar evolves; doesn't compose with existing tooling (LLMs, YAML viewers, GitHub renderers) | Rejected for Phase 3A — see Decision 4. Phase 3C may reopen this for friendly formula syntax only, not for the whole model |

**Phase 3A YAML safe subset (binding):**

- **YAML 1.2** parser. Phase 3A handoff picks the library (likely `serde_yaml` 0.9.x or `serde_yml`); whichever is chosen must operate against the YAML 1.2 spec, not the older 1.1 spec where `yes`/`no`/`on`/`off` parse as booleans.
- **No anchors** (`&foo`) and **no aliases** (`*foo`). Anchors enable structural sharing that complicates round-trip and confuses LLMs emitting against the schema.
- **No merge keys** (`<<:`). Same reasoning — the implicit deep-merge semantics aren't part of YAML 1.2 core and create hidden coupling between sections.
- **No custom tags** (`!!Foo`, `!Bar`). The model schema is closed; tag-driven extension points are unnecessary and hide validation surface.
- **All string-like values quoted in examples + golden specimens.** This includes IDs (`"Tampa"`), dates (`"2026-03-01"`), version strings (`"1.78"`), enum-like values (`"Sum"`, `"Input"`, `"F64"`). The validator (Decision 6) rejects unquoted enum values; the shipped Acme YAML and any in-tree example sets the convention.

The validator (Decision 6) enforces this subset — anchors / merge keys / unknown tags are blocking errors with file-line context.

**Downstream:** all subsequent decisions assume the YAML safe subset. The `mc-model` parser configures the YAML library to reject anchors/aliases/merge keys at parse time where the library supports it; the validator catches the rest.

### Decision 2: parser crate boundary

**Question:** Which crate owns parsing, validation, and the YAML → `mc-core` builder translation?

**Decision (Accepted):** A new crate `mc-model`. Layered alongside `mc-fixtures` and consumed by `mc-cli` (which gains a `--model <path>` flag). Per Decision 3 below + the owner amendment to Decision 7's Acme placement, **`mc-fixtures` does NOT depend on `mc-model` in normal Phase 3A code** — the canonical Rust path stays, and `mc-model::tests` may pull `mc-fixtures` as a dev-dependency to compare YAML-loaded Acme against the Rust canonical fixture.

```
                ┌─────────┐
                │ mc-cli  │  uses mc-model when --model is passed
                └────┬────┘
                     │
              ┌──────▼──────┐
              │  mc-model   │  parses YAML → ParsedModel → ValidatedModel → Cube
              └──────┬──────┘                  (see Decision 9 for the staged pipeline)
                     │
              ┌──────▼──────┐
              │  mc-core    │  unchanged — no parser, no serde, no YAML
              └─────────────┘
                     ▲
                     │ direct builder API (existing — Phase 3A leaves it as-is)
              ┌──────┴──────┐
              │ mc-fixtures │  build_acme_cube() Rust path stays as the canonical
              │             │  reference fixture (see Decision 3). NORMAL CODE
              │             │  does NOT depend on mc-model. mc-model's TEST CODE
              │             │  may pull mc-fixtures as a dev-dependency to diff
              │             │  YAML-loaded Acme against the Rust canonical fixture.
              └─────────────┘
```

**Mandatory invariant:** `mc-core` does not gain `serde`, `serde_yaml`, or any parser dependency. This is a strict reading of CLAUDE.md §1 ("Allowed runtime deps in `mc-core`: `smallvec`, `ahash`, `thiserror`, `once_cell`. Nothing else."). The brief §1 out-of-scope list also explicitly bans `serde` from the kernel.

**Downstream:** Phase 3A's dep budget lives entirely in `mc-model`. Likely additions: `serde`, `serde_yaml` (or `serde_yml`), maybe a small validation helper. Each one needs an explicit line item in the Phase 3A handoff; this ADR pre-approves the *shape* (a separate crate where parser deps land) but not the *exact list* (that's the handoff's job).

### Decision 3: dependency rules

**Question:** Where can parser-related dependencies live?

**Decision (Accepted):**

- **`mc-core`:** unchanged. No `serde`, no YAML parser, no anything outside the four allowed runtime deps (`smallvec`, `ahash`, `thiserror`, `once_cell`). **Hard rule.** Violating this is a Phase 3A scope failure, not a deviation.
- **`mc-model`:** may take `serde`, a YAML parser, and small validation helpers. Each addition documented in the Phase 3A handoff with rationale + a transitive-deps audit.
- **`mc-fixtures`:** the Rust `build_acme_cube` path stays as the canonical fixture reference. **`mc-fixtures` does NOT depend on `mc-model` in Phase 3A** unless a specific reason surfaces (in which case: SPEC QUESTION before adding the dep). `mc-model::tests` may pull `mc-fixtures` as a dev-dependency to diff YAML-loaded Acme against the Rust canonical fixture.
- **`mc-cli`:** may depend on `mc-model` to add the `--model <path>` flag.

**Toolchain-bump trigger.** Any parser dep (or its transitives) that requires a Rust toolchain bump past 1.78 **triggers ADR-0005**. Phase 3A does NOT bump `rust-toolchain.toml` unilaterally; the bump is its own decision with its own audit trail. If the Phase 3A handoff's chosen YAML library (or its transitives) need `edition2024` / Rust 1.85+, the handoff blocks on ADR-0005 landing first. Order of preference: (1) pick a 1.78-compatible library; (2) pin transitive crates the way Phase 1B did with `clap`/`clap_lex`/`half`; (3) only as a last resort, open ADR-0005 for the toolchain bump.

**Downstream:** the Phase 3A handoff opens with a "deps to add" section. If any of them need Rust 1.85+ AND no transitive-pin escape works, Phase 3A blocks on ADR-0005 before any source change.

### Decision 4: rule representation in YAML

**Question:** How are rules expressed in the model file?

**Decision (Accepted):** **Structured expression tree** (s-expression-shaped YAML). Friendly formula strings (`Revenue = Customers * AOV`) are deferred to **Phase 3C**.

| Approach | YAML shape | Pros | Cons | Verdict |
|---|---|---|---|---|
| **Structured expression tree** | `body: { mul: [{ ref: Customers }, { ref: AOV }] }` | No parser needed; maps 1:1 onto `mc-core::Expr` enum; round-trip from/to Rust is trivial; LLMs can emit it reliably given the schema | Verbose; not what an analyst wants to type by hand | **Recommended for Phase 3A** |
| **Formula strings** | `formula: "Customers * AOV"` | Natural for human authors; matches what Excel users expect | Requires a formula parser + grammar + error messages (column-precise) + reserved-word management; non-trivial scope on its own | **Deferred to Phase 3C.** Reopens once Phase 3A's structured-tree foundation is shipped |
| **Custom DSL** | (whole new language) | Could express domain-specific concepts | Massive scope; not composable with LLM authoring; same arguments as Decision 1's DSL row | Rejected |

**Why defer formula strings:** writing a formula parser well is its own Phase. The current 5-rule Acme cube is small; structured-tree YAML for those 5 rules is ~30 lines total. The friction is real but bounded for Phase 3A; Phase 3C can convert the structured-tree YAML to a formula-string surface once the foundation is proven.

**Schema sketch (illustrative, not normative):**

```yaml
rules:
  - id: clicks
    target: { measure: Clicks, scope: leaf }
    body:
      div:
        - ref: { measure: Spend }
        - ref: { measure: CPC }
    declared_dependencies:
      - { measure: Spend }
      - { measure: CPC }
```

The exact schema (field names, nesting shape, how `declared_dependencies` overlap with `body.refs`) is the Phase 3A handoff's job. This ADR commits to the *category* (structured tree), not the *exact YAML shape*.

**Downstream:** Phase 3C's friendly-formula work compiles formula strings into the same structured tree this ADR commits to. No throw-away code in 3A.

### Decision 5: cube count

**Question:** Does Phase 3A support multi-cube models or cross-cube references?

**Decision (Accepted): one cube per file. No cross-cube references.** Multi-cube and cross-cube are deferred to a future Phase 3 sub-phase (Phase 3D or later — not pre-named).

**Why:** the kernel today is single-cube. `mc-core` has no concept of a "workspace" or "cube reference." Adding multi-cube semantics is a kernel change, not a model-format change — and it's specifically called out as Phase 3+ in CLAUDE.md §1 ("Workspace is Phase 3+"). Phase 3A's job is the single-cube authoring path; the multi-cube layer can sit on top of that later without changing the single-cube format.

**Downstream:** the YAML file's top level is one model. The `metadata` block names the cube. There is no `imports:`, no `refs:`, no `extends:`. A future multi-cube layer adds those; this ADR commits to not pre-positioning for them.

### Decision 6: validation surface

**Question:** What validation does `mc-model` perform before handing the model to `mc-core::CubeBuilder`?

**Decision (Accepted):** all of the following, every one of them blocking. Validation runs in `mc-model` between the `ParsedModel` and `ValidatedModel` stages of Decision 9's pipeline, so errors are surfaced with file / line / column context, not as `EngineError::Internal` after the fact.

| Validator | Catches | Why blocking |
|---|---|---|
| **Duplicate names** | Two dimensions, two elements within a dim, two measures, two rules with the same name | Without this, the builder picks one and silently shadows the other |
| **Missing dimensions** | A measure / coordinate / hierarchy references a dim name that isn't declared | Builder would fail with a generic ID error; user wants "line 42: dim `Channel` not declared" |
| **Invalid hierarchy edges** | Hierarchy edge child / parent references an element not in the dim | Same — builder error is generic, model error should be precise |
| **Hierarchy cycles** | Element A → B → A in any default hierarchy | `Hierarchy::builder` already detects this but Phase 3A surfaces it pre-build with model context |
| **Rules referencing unknown measures** | `body.refs` mentions a measure not declared in the measures section | Builder fails opaquely; model error names the rule |
| **Derived measures without rules** | A measure is declared `role: derived` but no rule has `target: that measure` | Silent failure mode in the kernel — no rule means the cell is permanently `Null`. Catch at parse time |
| **Input measures with rules** | A measure is declared `role: input` but a rule targets it | Silent failure mode (rule registration may or may not error depending on impl); catch at parse time |
| **Rule cycles** | Rule R1 reads measure M targeted by R2 which reads measure N targeted by R1 | The kernel detects this at registration (`EngineError::CycleDetected`); model layer surfaces it pre-registration with the cycle path |
| **Unsupported aggregation methods** | Measure says `agg: median` and the kernel doesn't implement `Median` | Kernel surface (`AggregationRule` enum) is finite — model validator enforces that finiteness |
| **Golden test mismatches** | An inline golden test (Decision 7) doesn't match what the loaded cube produces | Catches both kernel regressions and model authoring errors |

Each validator returns `mc_model::Error` with a structured variant (e.g. `DuplicateName { kind: "dimension", name: "Time", first_at: Span, second_at: Span }`). Error messages include file:line:column where the source supports it (YAML libraries vary on span fidelity; the handoff specifies which library is chosen).

**Downstream:** `mc-model` carries a substantial validation layer (probably the largest single part of the crate). This is intentional — the brief's CLAUDE.md §2.6 discipline ("never just loosen the bound to match what you measured") translates here as "never silently accept a malformed model just because the kernel happens not to crash on it."

### Decision 7: golden tests

**Question:** How are golden tests for the model expressed and run?

**Decision (Accepted):** **Inline golden tests** under a top-level `golden_tests:` block in the model YAML are the **default** for Phase 3A. The shipped Acme YAML MUST include inline goldens covering the brief §4.5.1 anchor values byte-for-byte. **Sibling-file golden support (`model.golden.yaml`) is allowed but deferred to a later phase** for larger models — Phase 3A's parser does not need to implement sibling-file loading; that's a forward extension when a real cube outgrows inline goldens. The Acme cube fits inline cleanly.

**Inline shape (illustrative):**

```yaml
golden_tests:
  - name: spend_at_tampa_paid_search_march
    coord:
      Scenario: Baseline
      Version: Working
      Time: Mar_2026
      Channel: Paid_Search
      Market: Tampa
      Measure: Spend
    expect: 11500.0

  - name: revenue_consolidates_to_florida_q1
    coord: { Scenario: Baseline, Version: Working, Time: Q1_2026, Channel: All_Channels, Market: Florida, Measure: Revenue }
    expect_within_epsilon: { value: 1234567.89, epsilon: 1.0e-9 }
```

**Why inline (Phase 3A default):** the model and its expected outputs travel together. Editing the model and forgetting to update the goldens is impossible if they're in the same file. For LLM-authored models (Phase 4), the LLM emits both the model and the goldens in one response — no chance of drift.

**Why sibling files are deferred** (not banned, just not Phase 3A scope): for the Acme cube specifically, the §4.5.1 anchor goldens (~8 of them) inline cleanly. A future scaled cube with hundreds of sample-grid goldens would dominate the YAML, but Phase 3A doesn't ship that cube. When the need arises, sibling-file loading is a small additive parser feature (no schema change, no validator change) — the right time to build it is when the first real cube needs it.

**Acceptance gate:** Phase 3A's `cargo test -p mc-model` includes a test that loads the Acme YAML, runs every inline golden, and diffs against the value the loaded cube produces. The Acme YAML's goldens MUST include the brief §4.5.1 anchor values byte-for-byte.

**Downstream:** the model file is self-validating. A user can hand someone a YAML and say "here's the model; here's what it should produce; the validator + golden runner proves it works."

### Decision 8: LLM authoring scope boundary

**Question:** Does Phase 3A include LLM-assisted model authoring?

**Decision (Accepted): No. LLM authoring is Phase 4, not Phase 3A.** Phase 3A ships the deterministic, hand-authored YAML format + parser + validator. Phase 4 consumes the schema after Phase 3A proves the deterministic path.

**Why split:** the LLM-authoring layer needs (a) a stable schema to emit against, (b) a validator that turns malformed LLM output into actionable error messages, (c) a round-trip canonicalization step (LLM-emitted YAML → parsed → re-serialized → still parses), (d) prompt scaffolding. **(a) and (b) are Phase 3A's deliverable.** Phase 4 then has a foundation to build on.

Mixing the two in one phase has the failure mode of every previous "we'll do it all at once" project: the deterministic path doesn't get exercised before the LLM path is layered on, and validation gaps are masked by LLM "good enough" behavior.

**Downstream:** Phase 4's handoff opens with "Phase 3A's `mc-model::Schema` is the contract you emit against; Phase 4 does not modify it." Same shape as Phase 2D's handoff opening with "Phase 2C's bench baseline is the diff target."

### Decision 9: intermediate representation pipeline

**Question:** How does YAML get to a `mc_core::Cube`? One step (parse-and-build) or staged?

**Decision (Accepted):** Staged, with **two intermediate types** between YAML and `Cube`. The pipeline is:

```
YAML bytes
    │ (yaml library: serde_yaml or equivalent)
    ▼
ParsedModel        ←─ raw deserialization; mirrors YAML structure 1:1; field types
    │                 are owned strings + numbers + Vecs; no IDs allocated yet;
    │                 no semantic checking; only YAML-syntax errors surface here
    │
    │ (mc-model::validate)
    ▼
ValidatedModel     ←─ Decision 6's full validator pass has run; every check passed;
    │                 names resolved to internal references; element ordering canonical;
    │                 hierarchy edges checked; rule deps checked; this type is a
    │                 "guaranteed-buildable" model
    │
    │ (mc-model::compile / build)
    ▼
mc_core::Cube      ←─ ValidatedModel walked to call CubeBuilder / Dimension::builder /
                      Hierarchy::builder / Rule { ... } in the right order. This stage
                      cannot fail except for IdGenerator exhaustion (which is an
                      EngineError::Internal-class problem, not a model error).
```

**Why three stages, not one:**

1. **`ParsedModel` separates YAML errors from semantic errors.** A typo in YAML syntax (missing colon, bad indentation) surfaces as a `ParseError` with line:column from the YAML library. A typo in a measure name (typo'd "Spnd" instead of "Spend") surfaces as a `ValidationError` with model-level context ("rule R1 references measure 'Spnd' which is not declared, did you mean 'Spend'?"). Mixing them produces unactionable errors.
2. **`ValidatedModel` is the LLM contract.** Phase 4 consumes `ValidatedModel`-shaped data — emitted by an LLM, parsed once, validated once, then compiled to a `Cube`. If Phase 4 emits a malformed model, validation rejects it before any builder call; Phase 4 can re-prompt the LLM with the structured `ValidationError`. This is impossible if the parser and builder are coupled.
3. **`compile` is a function from `ValidatedModel` to `Cube`, not from YAML to `Cube`.** That makes the compilation step independently testable, and it makes a future "in-memory model construction" path (e.g. a UI editing surface in Phase 6) trivial — the UI builds a `ValidatedModel` directly, calls `compile`, gets a `Cube`. No YAML round-trip needed.

**Hard rule:** **Do not parse YAML directly into `mc_core::CubeBuilder` calls.** Even if it would be smaller code in Phase 3A, it conflates parse errors with semantic errors and forecloses the Phase 4 / Phase 6 use cases. The intermediate types are mandatory.

The exact field shape of `ParsedModel` and `ValidatedModel` (what's an `Option`, what's owned vs borrowed, where IDs are allocated) is the Phase 3A handoff's job. This ADR commits to the *three-stage shape*, not the byte-for-byte type definitions.

**Downstream:** Phase 4 (LLM authoring) emits against `ParsedModel`'s shape (since it's the YAML mirror), validates with `mc-model::validate`, compiles with `mc-model::compile`. Phase 6 (UI editor) builds `ValidatedModel` directly from edits, calls `compile`. Phase 3B (linter) reads `ValidatedModel` and runs static analysis on it. All three downstream phases anchor on this pipeline; collapsing it into one step in Phase 3A makes them all harder.

---

## Out of scope (explicit)

The following are NOT Phase 3A. Each is named here so a future implementer cannot rationalize "well, while we're at it…" into the scope.

| Out of scope | Phase | Notes |
|---|---|---|
| **UI / web grid editor** | Phase 6 | Phase 3A produces a YAML; Phase 6 produces a UI that edits YAMLs |
| **LLM model authoring** | Phase 4 | See Decision 8 |
| **DuckDB / external storage backend** | Phase 5+ | `HashMapStore` is still the only store |
| **Actuals import (CSV / API)** | Phase 5 | Actuals load values into a model that's already authored; Phase 3A authors the model itself |
| **Auth / authentication** | Phase 6 | The model file has no notion of users, principals, login |
| **Permissions in the model file** | Phase 6 | The kernel's `PrincipalId` / `Permission` types stay as they are; the YAML doesn't author them |
| **Multi-cube models** | Future Phase 3 sub-phase | See Decision 5 |
| **Cross-cube rules** | Future Phase 3 sub-phase | See Decision 5 |
| **Custom formula parser** | Phase 3C | See Decision 4 |
| **Model migration / versioning** | Future Phase 3 sub-phase | First version of the format ships with `model_format_version: 1`; migration semantics defined when v2 is needed |
| **Bidirectional round-trip (Cube → YAML)** | Future Phase 3 sub-phase | Phase 3A is one-way: YAML → Cube. Phase 6 (UI editor) needs the reverse direction |

A row appearing in this list is not a promise it will ship later — it's a promise it is **not Phase 3A** and any work on it requires its own ADR + handoff.

---

## What the model file defines in Phase 3A (in scope)

The Phase 3A model file expresses a single cube via these top-level sections:

| Section | Purpose | Maps to mc-core |
|---|---|---|
| `metadata` | Model name, format version, description, author, created date | `Cube` name + free-text |
| `dimensions` | Dim names, kinds (Standard / Time / Measure / Scenario / Version) | `Dimension::builder` calls |
| `elements` | Per-dim element list (id, name, optional metadata) | `Dimension::add_element` calls |
| `hierarchies` | Per-dim default hierarchy (parent-child edges, weights) | `Hierarchy::builder` calls |
| `measures` | Measure declarations (data type, role, aggregation) | `MeasureMeta` on Measure-dim elements |
| `scenarios` | Scenario element metadata (e.g. `is_active`, free-text) | `ScenarioMeta` |
| `versions` | Version element states (`Working` / `Submitted` / `Approved` / `Archived`) | `VersionState` |
| `rules` | Rule declarations per Decision 4 (structured expression trees) | `Rule { target, body, declared_dependencies }` |
| `golden_tests` | Per Decision 7 — inline expected values for round-trip + regression check | `mc-model` validator output |

The exact YAML schema (field names, nesting, optional vs required) is the Phase 3A handoff's deliverable. This ADR commits to the *list of sections*, not the *byte-for-byte schema*.

---

## Validation requirements summary

The validators from Decision 6 form `mc-model::validate(&Model) -> Result<ValidatedModel, Vec<ValidationError>>`. The function returns *all* errors at once (not first-error-then-stop) so a user editing a 500-line YAML sees every problem in one pass.

The error type is structured (per Decision 6) so the Phase 6 UI can render a marker per error in the editor gutter. Phase 3A ships the structured errors; Phase 6 consumes them. Phase 3A's CLI prints them as `file:line:column: <kind>: <message>` for terminal users.

---

## Success criteria

Phase 3A is complete when **all** of the following hold:

1. **Acme is represented as `acme.yaml`** in the repo (probably under `mc-fixtures/models/` or `mc-model/examples/`).
2. **`mc-model::load(&path) -> Result<Cube, _>`** loads the Acme YAML into an `mc_core::Cube` that is structurally equivalent to `build_acme_cube()` (same dimensions, same elements, same hierarchies, same measures, same rules, same metadata).
3. **`cargo run --release --bin mc -- demo --model crates/mc-fixtures/models/acme.yaml`** produces brief §4.6 output **byte-for-byte identical** to `cargo run --release --bin mc -- demo` (the existing Rust-fixture demo).
4. **Inline golden tests in the Acme YAML pass** — every brief §4.5.1 anchor value is present in `golden_tests:` and verified by `cargo test -p mc-model`.
5. **All existing kernel tests pass** — `cargo test --workspace` ≥ 227 / 0 (Phase 2D's count); any new tests added by Phase 3A are additive.
6. **`mc-core` has zero new dependencies.** `cat crates/mc-core/Cargo.toml` shows the same four allowed runtime deps as today (`smallvec`, `ahash`, `thiserror`, `once_cell`). The forbidden-pattern grep from CLAUDE.md §6.2 stays clean.
7. **Determinism gate holds.** 10 consecutive `cargo test --workspace -q` runs identical, including the new `mc-model` tests.
8. **Validation catches every error in Decision 6's table** — `mc-model` ships unit tests proving each validator triggers on a malformed-input fixture.

Phase 3A does NOT need to flip Phase 2D's tag, change PERF.md, or modify any spec doc. The kernel is locked.

---

## Risks

| Risk | Mitigation |
|---|---|
| **YAML ambiguity** (`yes`/`no`/`on`/`off` parse as bool; unquoted versions parse as floats; `1.78` parses as float not string) | Documented "safe subset" — quote all strings; schema validator rejects ambiguous values (e.g. measure type must be a string from a known enum, not a parsed-as-bool); Decision 6's "unsupported aggregation" validator catches the float-vs-string trap |
| **Overbuilding a DSL too early** (Decision 4's friendly formulas, Decision 1's custom DSL) | Both deferred. Phase 3A ships structured trees only. Phase 3C reopens formula strings *after* the foundation is proven |
| **Weak validation lets bad LLM output through** (Phase 4 emits malformed YAML; the kernel happens to accept it; bug surfaces in production) | Decision 6's validators are exhaustive and blocking. Phase 4's handoff will reference this ADR's validator list as the contract LLM output is checked against |
| **Dependency creep into mc-core** (the easy mistake — "let me just add `serde` to make this nicer") | CLAUDE.md §1 hard rule. Decision 3's invariant. Phase 3A handoff opens with this as a forbidden action |
| **Format becoming hard to migrate** (v1 ships, real users adopt, v2 needs a breaking change) | `metadata.model_format_version: 1` from day one. Migration semantics defined when v2 is needed (separate ADR), not pre-built. The format's first version is small enough that a v2 transformer is tractable |
| **YAML library choice** (multiple Rust YAML crates exist with different span fidelity, different YAML 1.1 vs 1.2 quirks, different maintenance status) | Phase 3A handoff picks one with rationale; this ADR doesn't pick. The choice is reversible (format spec is library-agnostic) |
| **Schema evolution** (a Phase 3B linter wants stricter schema; existing models break) | Linter is opt-in (Phase 3B); strictness lives in the linter, not in the parser. Parser stays backwards-compatible within a major version |
| **Inline goldens balloon the model file** | Decision 7 allows sibling `model.golden.*.yaml` files. Phase 3A handoff defines the loader behavior |

---

## What this unlocks

Phase 3A's deliverable is the foundation for:

- **Phase 3B — Model linter.** Static analysis of the YAML beyond Phase 3A's parse-time validation. Style rules, performance hints ("this rule chain is 8 deep; consolidation reads will be slow at scale"), naming-convention checks. Read-only over a parsed `mc_model::Model`.
- **Phase 3C — Friendly formula syntax.** Formula strings (`Revenue = Customers * AOV`) compile down to Phase 3A's structured expression trees. No kernel change; the parser sits in `mc-model` alongside the structured-tree path.
- **Phase 4 — LLM-assisted model authoring.** LLM emits YAML against Phase 3A's schema; validator catches malformed output; Phase 4 layer adds prompt scaffolding, conversational refinement, "edit this rule" semantics. Without Phase 3A's deterministic foundation, Phase 4 has nothing to emit *against*.
- **Phase 5 — Actuals / data integration.** A "model the cube once, then load actuals into it" workflow needs a stable model artifact. Phase 3A's YAML is that artifact.
- **Phase 6 — UI model editor.** Web UI parses YAML into the editing surface; on save, serializes the editing surface back to YAML (round-trip — Phase 6's deliverable, not Phase 3A's). Phase 3A's `mc-model::Schema` becomes the JSON-schema-equivalent the editor renders forms against.

Each of those phases blocks on Phase 3A. Phase 3A blocks on this ADR.

---

## Accepted decisions — TL;DR

Phase 3A ships against:

1. **YAML** as the file format, **safe-subset only** — YAML 1.2, no anchors, no merge keys, no custom tags, all string-like values quoted in examples + goldens (Decision 1).
2. **`mc-model`** as a new crate for parsing / validation / compilation (Decision 2).
3. **No parser deps in `mc-core`** — `serde` and YAML libraries live in `mc-model` only; toolchain bumps trigger ADR-0005 (Decision 3).
4. **Structured expression trees** for rules in Phase 3A; friendly formula strings deferred to Phase 3C (Decision 4).
5. **One cube per file**, no cross-cube refs, no multi-cube models (Decision 5).
6. **Exhaustive blocking validation** per Decision 6's table (Decision 6).
7. **Inline golden tests** are the Phase 3A default; sibling-file goldens deferred (Decision 7).
8. **LLM authoring is Phase 4, not Phase 3A** (Decision 8).
9. **Three-stage pipeline:** YAML → `ParsedModel` → `ValidatedModel` → `mc_core::Cube`; no direct YAML-to-CubeBuilder shortcut (Decision 9).

The Acme cube becomes the first round-tripped model: `crates/mc-model/examples/acme.yaml` + inline golden tests, demonstrably equivalent to `build_acme_cube()`, runnable via `mc demo --model crates/mc-model/examples/acme.yaml`.

---

## Acceptance amendments

This ADR was Proposed and Accepted on 2026-05-02 with the following project-owner amendments to the Proposed defaults. Each is recorded here for audit trail; the decisions above already reflect the final shape.

| # | Open question (Proposed) | Owner decision | Where it landed |
|---|---|---|---|
| 1 | Is YAML the right pick? | **Yes — accepted** with a binding "safe subset" (YAML 1.2; no anchors; no merge keys; no custom tags; quote all string-like values, IDs, dates, versions, enum-like values in examples + goldens) | Decision 1 expanded with the safe-subset rules |
| 2 | `mc-model` as a separate crate vs a feature flag? | **Separate crate, accepted.** Parser deps allowed only in `mc-model`. Any parser dep needing a Rust toolchain bump triggers ADR-0005 | Decision 2 + Decision 3 |
| 3 | Keep `build_acme_cube()` or replace with YAML load? | **Keep — `mc-fixtures::build_acme_cube()` stays as the canonical Rust reference.** Do not replace it in Phase 3A. Acme YAML is the *new* path; Rust fixture is the regression-test floor it's diffed against | Decision 3 + Success criterion #2; new locked-language in the Phase 3A handoff |
| 4 | Inline goldens vs sibling-file goldens? | **Inline default for Phase 3A. Acme MUST include inline goldens.** Sibling-file support is allowed but deferred — Phase 3A's parser does not implement sibling loading | Decision 7 |
| 5 | Toolchain bump policy? | **(b)-then-ADR-0005.** Try a 1.78-compatible YAML library first; pin transitives if needed (Phase 1B precedent); only as a last resort, open ADR-0005 for the toolchain bump | Decision 3 expanded with the trigger language |
| 6 | Format version: integer or semver? | **Integer.** `model_format_version: 1`. Semver deferred until v2 is being designed | Decision 6 (validator); explicit success criterion |
| 7 | Where does `acme.yaml` live? | **`crates/mc-model/examples/acme.yaml`.** `mc-model::tests` may pull `mc-fixtures` as a dev-dependency to compare YAML-loaded Acme against the Rust canonical fixture; `mc-fixtures` does NOT depend on `mc-model` in normal Phase 3A code | Decision 2 architecture diagram + Decision 3 dep rules; Phase 3A handoff specifies the exact path |
| 8 | Draft Phase 3A handoff now or wait? | **Wait until Accepted, then draft.** Handoff drafted at acceptance, lives at [`../handoffs/phase-3a-handoff.md`](../handoffs/phase-3a-handoff.md) | Phase 3A handoff exists post-acceptance |

Additionally, the project owner introduced a **new Decision 9** at acceptance: the three-stage pipeline `YAML → ParsedModel → ValidatedModel → Cube` is mandatory; no direct YAML-to-CubeBuilder shortcut. Decision 9 above contains the full rationale.

No remaining open questions. Phase 3A handoff is the implementation contract.

---

## Alternatives considered (whole-ADR scope)

1. **Skip Phase 3A; jump to a UI-driven authoring layer in Phase 6.** Rejected — Phase 4 (LLM authoring) needs a deterministic schema to emit against, and Phase 5 (data integration) needs a stable model artifact. The UI is downstream of the schema, not a substitute.
2. **Build a custom DSL instead of YAML.** Rejected — see Decision 1. The format-cost is unjustified for Phase 3A scope; the DSL idea reopens narrowly in Phase 3C for formula strings only.
3. **Make `serde` a `mc-core` dep "since we'll need it eventually anyway."** Rejected, hard. CLAUDE.md §1 bans it; the brief §1 out-of-scope lists `serde` explicitly. The whole point of the `mc-model` crate is to keep the parser concern out of the kernel.
4. **Ship Phase 3A and Phase 4 (LLM authoring) together.** Rejected — see Decision 8. The deterministic foundation needs to be exercised standalone before the LLM layer is bolted on.
5. **Defer the format ADR entirely; let the Phase 3A implementer pick the format during the handoff.** Rejected — the format choice has consequences for Phases 3B/3C/4/5/6 that need project-owner sign-off, not implementer-default. ADR-shaped decisions are the right shape for this.

---

## Cross-links

- [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) — Phase 3A entry, currently `planned, blocked` on this ADR.
- [`../CURRENT_STATE.md`](../CURRENT_STATE.md) — Phase 3A queued line.
- [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) §1 — out-of-scope list including `serde`.
- [`../../CLAUDE.md`](../../CLAUDE.md) §1 — allowed runtime deps; §1.1 — the `proptest` / `insta` deferral, similar shape to "parser deps live outside `mc-core`."
- [`../product/MC-PRD.md`](../product/MC-PRD.md) — original product framing this ADR narrows for Phase 3A.
- [`../external-conversations/`](../external-conversations/) — historical record of the model-layer framing.
- [`0001-phase-1-scope.md`](0001-phase-1-scope.md) — Phase 1 scope ADR; this ADR is the analogous strategic document for Phase 3.
- [`0002-perf-assertions-in-benchmarks-not-tests.md`](0002-perf-assertions-in-benchmarks-not-tests.md), [`0003-workload-sketch.md`](0003-workload-sketch.md) — prior ADRs.

## Notes

This ADR is the strategic gate for Phase 3 the way ADR-0003 was the strategic gate for Phase 2. It makes the format/parser/scope decisions once so the Phase 3A handoff can be a build contract rather than a debate.

If this ADR is amended after Acceptance, the amendment lands as `0004-amendment-N.md` rather than rewriting the original. The append-only discipline matches ADR-0003's pattern.

The **decision to even have a Phase 3A** is upstream of this ADR — it's recorded in MASTER_PHASE_PLAN.md as part of the path to "first usable product" (Phase 6). This ADR scopes *what* Phase 3A is, not *whether* it should exist.
