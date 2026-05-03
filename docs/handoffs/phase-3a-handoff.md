# Phase 3A Handoff — Model Definition Layer (`mc-model` crate)

> **Audience:** the Claude Code instance running in
> `/Users/edwinlovettiii/Projects/mc-v2/` that picks up Phase 3A.
> **You inherit a green Phase 2D** (commit `0678a98`, tag
> `phase-2d-bitset-and-invalidated-fix`).
>
> **This is the first Phase that ships a NEW CRATE.** Phase 3A creates
> `crates/mc-model/` to translate human-authored YAML cube definitions
> into `mc_core::Cube` instances. The kernel is **NOT modified.**
>
> **Hard rule:** Phase 3A touches `crates/mc-model/` (new), `crates/mc-cli/`
> (one new flag), and `Cargo.toml` (workspace member entry). It does **NOT**
> touch `crates/mc-core/src/`, `crates/mc-core/tests/`, `crates/mc-core/benches/`,
> `crates/mc-fixtures/src/`, or `docs/specs/`. The previous-phase template
> (Phase 2D's surgical kernel change) is the operating model: small, source-
> bounded, accompanied by tests, justified by a contract — but this time the
> contract is [`ADR-0004`](../decisions/0004-phase-3a-model-definition-format.md)
> and the source-bound is "everything in `crates/mc-model/`, plus a CLI flag."

---

## Where Phase 2D ended

- **Phase 2D commit / tag:** `0678a98` — *phase-2d: bitset DirtyTracker + WritebackResult.invalidated semantic fix* — tag `phase-2d-bitset-and-invalidated-fix`. Backfill commit at `e06d4ee`.
- **Test status:** 227 / 0 passing across all targets. 10/10 deterministic.
- **Demo:** `cargo run --release --bin mc -- demo` matches brief §4.6.
- **Gates green:** build / fmt / clippy / test / demo / bench.
- **Toolchain:** Rust 1.78 pinned in [`../../rust-toolchain.toml`](../../rust-toolchain.toml). **Do not bump without explicit approval — see ADR-0005 trigger below.**
- **Cargo.lock pins (still load-bearing):** `clap → 4.4.18`, `clap_lex → 0.6.0`, `half → 2.4.1`. Do not run `cargo update`.
- **PERF.md §9 status:** §9.3 closed in Phase 2D; §9.2/§9.5/§9.6 stay opportunistic. **Phase 3A is not a performance phase** — it adds a translation layer with no kernel change.

For the full Phase 2D audit see [`../reports/phase-2d-completion-report.md`](../reports/phase-2d-completion-report.md). For the binding strategic context read [`../decisions/0004-phase-3a-model-definition-format.md`](../decisions/0004-phase-3a-model-definition-format.md) **before this handoff** — the ADR is the contract; this handoff is the build instructions.

---

## Phase 3A prompt (verbatim — this is your contract)

> We are starting MarketingCubes Phase 3A: Model Definition Layer.
>
> **Context.** Today, cubes are authored by writing Rust against `mc-core`'s builder API (see [`crates/mc-fixtures/src/lib.rs`](../../crates/mc-fixtures/src/lib.rs) — `build_acme_cube` is ~700 lines of `Dimension::builder()`/`Hierarchy::builder()`/`Rule { ... }` calls). That works for the in-tree fixture but doesn't scale to LLM-assisted authoring (Phase 4), data integration (Phase 5), or a UI editor (Phase 6). Phase 3A introduces a YAML-based model file + a parser/validator/compiler crate (`mc-model`) that translates the YAML into the same builder calls `mc-fixtures` makes today.
>
> **Goal.** Ship `mc-model` such that:
> 1. A YAML file at `crates/mc-model/examples/acme.yaml` describes the Acme cube using the schema this Phase defines.
> 2. `mc_model::load(&path) -> Result<Cube, Vec<mc_model::Error>>` loads the YAML into a `mc_core::Cube` that is structurally equivalent to `mc_fixtures::build_acme_cube()`.
> 3. `cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml` produces brief §4.6 output **byte-for-byte identical** to `cargo run --release --bin mc -- demo`.
> 4. Inline golden tests in `acme.yaml` covering brief §4.5.1 anchor values pass.
>
> **Phase 3A scope** (read [`../decisions/0004-phase-3a-model-definition-format.md`](../decisions/0004-phase-3a-model-definition-format.md) for the strategic rationale; this scope IS what the ADR's Decisions 1–9 commit to):
>
> 1. **Create the `mc-model` crate** at `crates/mc-model/`. Workspace member. Library crate. Public API surface: `mc_model::load(&path) -> Result<Cube, Vec<Error>>`, `mc_model::parse(&str) -> Result<ParsedModel, Error>`, `mc_model::validate(ParsedModel) -> Result<ValidatedModel, Vec<ValidationError>>`, `mc_model::compile(ValidatedModel) -> Result<Cube, EngineError>`. Plus the public types `ParsedModel`, `ValidatedModel`, `Error`, `ValidationError`. Internal modules: `parse`, `validate`, `compile`, `schema`, `error`. The exact public function names + types may evolve — what matters is the three-stage shape.
>
> 2. **Implement the YAML safe subset parser** per ADR-0004 Decision 1. YAML 1.2; reject anchors / aliases / merge keys / custom tags at parse time where the library supports it, in the validator otherwise. Quote-all-string-like-values is an authoring convention enforced by the validator (Decision 6 includes "unsupported aggregation" / "unknown role" checks that catch the float-vs-string trap).
>
> 3. **Implement the three-stage pipeline** per ADR-0004 Decision 9: `YAML bytes → ParsedModel → ValidatedModel → mc_core::Cube`. **Do NOT take a YAML-to-CubeBuilder shortcut** even if it would be smaller code. The intermediate types are the contract Phase 4 (LLM authoring) and Phase 6 (UI editor) consume.
>
> 4. **Implement the validator** covering ADR-0004 Decision 6's full table (10 validators). All errors return at once (not first-error-then-stop) with `file:line:column` context where the YAML library supports spans. Errors are structured (per `ValidationError` enum) so a future UI can render them.
>
> 5. **Re-express the Acme cube as `crates/mc-model/examples/acme.yaml`.** Same dimensions / elements / hierarchies / measures / rules as `mc_fixtures::build_acme_cube()`. Includes a `golden_tests:` block with at minimum the brief §4.5.1 anchor values byte-for-byte. The Acme YAML is the canonical Phase 3A example AND the regression-test fixture.
>
> 6. **Add `--model <path>` flag to `mc-cli`.** When passed, `mc demo --model <path>` calls `mc_model::load(path)` instead of `mc_fixtures::build_acme_cube()`, then runs the same demo flow against the loaded cube. Output must match brief §4.6.
>
> 7. **Add the structural-equivalence test** in `crates/mc-model/tests/`. Loads `acme.yaml` via `mc_model::load`, builds the canonical cube via `mc_fixtures::build_acme_cube()` (dev-dep), asserts dim count / dim names / dim element counts / hierarchy edge counts / measure metadata / rule count / rule body shape are equal. The test does NOT compare every coordinate — that's the demo equivalence's job — but it asserts *structural* equality so a YAML-vs-Rust drift surfaces as a clear test failure.
>
> 8. **Add per-validator unit tests** in `crates/mc-model/src/validate/tests.rs` (or `crates/mc-model/tests/validators.rs`). One test per validator from ADR-0004 Decision 6's table — each builds a malformed `ParsedModel`, runs `validate`, asserts the expected `ValidationError` variant fires. **Negative tests are the validator's contract.**
>
> 9. **Run the full validation gate.** All 227 existing tests still pass. New `mc-model` tests pass. Demo equivalence holds byte-for-byte. Determinism gate (10×) holds.
>
> **Hard rules:**
>
> - **`crates/mc-core/src/`, `crates/mc-core/tests/`, `crates/mc-core/benches/`, and `crates/mc-fixtures/src/` are LOCKED.** Phase 3A does not modify them. The kernel is finished for this phase.
> - **`mc-core` gets ZERO new dependencies.** `cat crates/mc-core/Cargo.toml` after Phase 3A shows the same four runtime deps as today (`smallvec`, `ahash`, `thiserror`, `once_cell`). The forbidden-pattern grep stays clean.
> - **Parser deps live in `mc-model` only.** `serde`, `serde_yaml` (or `serde_yml`), and any small validation helpers are added to `crates/mc-model/Cargo.toml`. Each addition is documented in the completion report with rationale + a transitive-deps audit.
> - **Toolchain stays at Rust 1.78.** If your chosen YAML library or its transitives need `edition2024` / Rust 1.85+, **stop and check ADR-0004 Decision 3's order of preference**: (1) try a 1.78-compatible library; (2) pin transitives the way Phase 1B did; (3) only as a last resort, **stop and write a SPEC QUESTION** that proposes opening ADR-0005 for the toolchain bump. Do not bump unilaterally.
> - **`mc-fixtures` does NOT depend on `mc-model`** in normal Phase 3A code. `mc-model::tests` may pull `mc-fixtures` as a `[dev-dependencies]` entry to compare YAML-loaded Acme against the Rust canonical fixture (Decision 3, Decision 7). If a non-test code path needs `mc-fixtures → mc-model`, **stop and write a SPEC QUESTION** before adding it.
> - **`mc-fixtures::build_acme_cube()` stays as the canonical Rust reference.** Do not replace it with a YAML-load. Do not delete it. Do not modify it. The YAML is the *new* path; the Rust fixture is the regression-test floor (Decision 3).
> - **Three-stage pipeline is mandatory.** No direct YAML-to-CubeBuilder shortcut, even for "just the Acme cube to bootstrap" (Decision 9).
> - **YAML safe subset is binding.** No anchors, no merge keys, no custom tags. Quote all string-like values in the Acme YAML and any other example you ship (Decision 1).
> - **Inline goldens are the Phase 3A default.** Sibling-file goldens (`model.golden.yaml`) are deferred — do not implement sibling-loading in Phase 3A (Decision 7).
> - **Structured expression trees only for rules.** Friendly formula strings (`Revenue = Customers * AOV`) are Phase 3C — do not implement a formula parser in Phase 3A (Decision 4).
> - **One cube per file.** No multi-cube YAML, no `imports:`, no `extends:`, no cross-cube references (Decision 5).
> - **`docs/specs/` is locked.** The brief and engine-semantics doc are unchanged.
> - **No async / threads / rayon / tokio anywhere** (still). The validator and compiler are sync. Loading a YAML file is sync.
> - **All 227 existing tests must still pass.** New total ≥ 227 + the new `mc-model` test count.
> - **No `cargo update`.** Cargo.lock pins from Phase 1B (`clap → 4.4.18`, `clap_lex → 0.6.0`, `half → 2.4.1`) stay.
>
> **Acceptance gate (the one thing that determines done):**
>
> `diff <(cargo run --release --bin mc -- demo) <(cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml)` produces **zero output** (byte-for-byte identical).
>
> Secondary expectations (all required, but the diff above is the headline gate):
> - `cargo test --workspace` ≥ 227 + the new `mc-model` test count, all passing.
> - All inline goldens in `acme.yaml` pass (`cargo test -p mc-model -- goldens`).
> - All per-validator negative tests pass (one per Decision 6 row).
> - Structural-equivalence test passes (compares `mc_model::load(acme.yaml)` against `mc_fixtures::build_acme_cube()`).
> - 10/10 deterministic.
>
> **Validation gate before reporting done:**
>
> Run, in order:
> - `cargo fmt --check --all` (exit 0)
> - `cargo clippy --workspace --all-targets -- -D warnings` (exit 0)
> - `cargo build --release --workspace` (zero warnings)
> - `cargo test --workspace` (≥ 227 + new tests)
> - `cargo run --release --bin mc -- demo` (matches brief §4.6 — Rust path)
> - `cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml` (matches brief §4.6 — YAML path)
> - `diff <(cargo run --release --bin mc -- demo) <(cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml)` (empty output — the acceptance gate)
> - 10 consecutive `cargo test --workspace -q` (still deterministic)
> - `cat crates/mc-core/Cargo.toml | grep -E "^(serde|tokio|rayon|anyhow|.*yaml)"` (zero matches — `mc-core` deps unchanged)
>
> **Documentation requirements:**
> - Append `docs/reports/phase-3a-completion-report.md` per the [`../templates/phase-completion-report.md`](../templates/phase-completion-report.md) template.
> - Update [`../CURRENT_STATE.md`](../CURRENT_STATE.md) to flip Phase 3A from `proposed` → `complete`.
> - Update [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) Phase 3A status row.
> - Update [`../decisions/README.md`](../decisions/README.md) only if you authored a new ADR (e.g., ADR-0005 for toolchain bump or ADR-0006 for a YAML library choice that needs ADR-shape rationale).
> - **Do NOT modify [`ADR-0004`](../decisions/0004-phase-3a-model-definition-format.md).** It's Accepted; amendments go in `0004-amendment-N.md`.
> - **Do NOT modify the brief or the semantics doc.** They're contracts.
>
> **SPEC QUESTION triggers:**
>
> Open a SPEC QUESTION (per CLAUDE.md §11) before continuing if any of these surface:
> 1. The chosen YAML library or its transitives need a Rust toolchain bump past 1.78. Per Decision 3: try a 1.78-compatible alternative first; pin transitives if possible; only as a last resort propose ADR-0005.
> 2. A `mc-fixtures → mc-model` non-test dependency feels necessary. Per Decision 3: not allowed without owner approval.
> 3. A validator from Decision 6's table doesn't fit the structured-error model cleanly (e.g., the YAML library doesn't expose spans for a particular error class).
> 4. The structural-equivalence test surfaces a difference between YAML-loaded Acme and `build_acme_cube()` that you can't reconcile by tweaking the YAML — i.e., a kernel feature is reachable from the Rust builder but not from any YAML field shape this Phase commits to.
> 5. A `model_format_version` migration concern surfaces (e.g., you find yourself wanting v2 mid-Phase). Per Decision 6 + the ADR's risks table: ship v1 only; v2 is its own ADR + Phase.
> 6. The Acme YAML grows past ~1000 lines (sign that inline goldens or rule bodies are bloated; sibling files are deferred but if Acme genuinely doesn't fit inline that's a SPEC QUESTION).
>
> **Rollback plan (in case complexity explodes):**
>
> If the validator surface or the YAML schema balloons beyond ~1500 lines of source in `crates/mc-model/src/`, **stop and write a SPEC QUESTION**. Two recovery paths:
> 1. **Narrow the validator surface for Phase 3A.1**: ship a minimum-viable subset (parse + structural-equivalence + the 3 most load-bearing validators), defer the other 7 to a follow-up phase. Requires ADR-amendment.
> 2. **Reconsider library choice**: a different YAML / serde backend may dramatically reduce the schema scaffolding. Requires SPEC QUESTION + the deps audit.
>
> Either fallback is a Phase 3A.1 amendment, not a Phase 3A scope rewrite.
>
> **Completion report format:**
> ```
> DONE: Phase 3A Model Definition Layer
>
> Build:    cargo build --release --workspace ✓
> Format:   cargo fmt --check --all ✓
> Lint:     cargo clippy --workspace --all-targets -- -D warnings ✓
> Tests:    cargo test --workspace [N] / 0
> Demo:     cargo run --release --bin mc -- demo ✓
> Demo (YAML): cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml ✓
> Acceptance gate (diff): empty output ✓
> Determinism: 10 / 10 identical
>
> Source manifest:
> - crates/mc-model/Cargo.toml                       (new — N deps added)
> - crates/mc-model/src/lib.rs                       (new — public API + module decls)
> - crates/mc-model/src/parse/...                    (new — N files)
> - crates/mc-model/src/validate/...                 (new — N files)
> - crates/mc-model/src/compile/...                  (new — N files)
> - crates/mc-model/src/schema/...                   (new — ParsedModel + ValidatedModel types)
> - crates/mc-model/src/error.rs                     (new)
> - crates/mc-model/examples/acme.yaml               (new — N lines including N inline goldens)
> - crates/mc-model/tests/structural_equivalence.rs  (new)
> - crates/mc-model/tests/validators.rs              (new — one test per Decision 6 row)
> - crates/mc-model/tests/golden_acme.rs             (new — runs inline goldens from acme.yaml)
> - crates/mc-cli/src/main.rs                        (modified — --model flag)
> - crates/mc-cli/Cargo.toml                         (modified — mc-model dep added)
> - Cargo.toml (workspace)                           (modified — mc-model member entry)
>
> Dependencies added to mc-model:
> - <library>@<version> — <purpose>
> - <library>@<version> — <purpose>
> - <library>@<version> — <purpose>
> Transitive-deps audit: [no toolchain-bump triggers; no async; no serde feature flags pulling in unwanted code]
>
> Validator coverage (per ADR-0004 Decision 6):
> - duplicate_names                          ✓ tested
> - missing_dimensions                       ✓ tested
> - invalid_hierarchy_edges                  ✓ tested
> - hierarchy_cycles                         ✓ tested
> - rules_referencing_unknown_measures       ✓ tested
> - derived_measures_without_rules           ✓ tested
> - input_measures_with_rules                ✓ tested
> - rule_cycles                              ✓ tested
> - unsupported_aggregation_methods          ✓ tested
> - golden_test_mismatches                   ✓ tested
>
> Acme YAML structural diff against build_acme_cube():
> - dim count: equal
> - dim names: equal
> - per-dim element counts: equal
> - hierarchy edge counts: equal
> - measure metadata: equal (data type, role, aggregation per measure)
> - rule count + rule body shape: equal
>
> Inline goldens in acme.yaml:
> - <N> values from brief §4.5.1, all matching byte-for-byte
> - epsilon-tolerant where the brief specifies tolerance, exact otherwise
>
> Implementation summary:
> - <one paragraph: YAML library chosen + why; pipeline flow; validator pattern; CLI flag wiring>
>
> Deviations:
> - <list any; ideally empty>
> ```
>
> Do NOT commit or tag. The user reviews first.

---

## Context the prompt above does NOT spell out

These are landmarks the receiving instance will need.

### A. The shape of the Acme cube you're translating

[`crates/mc-fixtures/src/lib.rs`](../../crates/mc-fixtures/src/lib.rs) is the canonical Rust source for the Acme cube. Read its `build_acme_cube()` start-to-finish before you write any YAML. The cube is:

- 6 dimensions in this exact order: `Scenario`, `Version`, `Time`, `Channel`, `Market`, `Measure`.
- 3 default hierarchies: Time (Month → Quarter → Year), Channel (Channel → Channel_Group → All_Channels), Market (City → State → Region → USA).
- 11 measures: 6 inputs (`Spend`, `CPC`, `CVR`, `Close_Rate`, `AOV`, `COGS_Rate`) + 5 derived (`Clicks`, `Leads`, `Customers`, `Revenue`, `Gross_Profit`).
- 5 deterministic rules (per brief §4.4):
  - `Clicks = Spend / CPC`
  - `Leads = Clicks * CVR`
  - `Customers = Leads * Close_Rate`
  - `Revenue = Customers * AOV`
  - `Gross_Profit = Revenue * (1 - COGS_Rate)`
- 2,520 input cells loaded by `write_canonical_inputs` per brief §4.5 closed-form formulas.

The Acme YAML must produce a cube that walks identically through all of the above. The structural-equivalence test (item 7 in the prompt) is what catches a divergence; the demo-equivalence diff is what catches a behavioral one.

### B. The three-stage pipeline shape

ADR-0004 Decision 9 mandates:

```
YAML bytes
   │ serde_yaml (or chosen alternative) — YAML 1.2 safe-subset deserialization
   ▼
ParsedModel
   ├── metadata: ParsedMetadata
   ├── dimensions: Vec<ParsedDimension>
   ├── hierarchies: Vec<ParsedHierarchy>
   ├── measures: Vec<ParsedMeasure>
   ├── rules: Vec<ParsedRule>
   ├── golden_tests: Vec<ParsedGoldenTest>
   └── ... (mirrors YAML structure 1:1; field types are owned strings + numbers + Vecs;
            no IDs allocated; no semantic checking)
   │
   │ mc_model::validate(ParsedModel) -> Result<ValidatedModel, Vec<ValidationError>>
   ▼
ValidatedModel
   ├── (same shape as ParsedModel BUT every check from Decision 6 passed)
   ├── (names resolved to internal references)
   ├── (element ordering canonical)
   ├── (hierarchy edges checked; rule deps checked; cycle-free)
   └── (this type is "guaranteed-buildable")
   │
   │ mc_model::compile(ValidatedModel) -> Result<Cube, EngineError>
   ▼
mc_core::Cube
   (ValidatedModel walked to call CubeBuilder / Dimension::builder /
    Hierarchy::builder / Rule { ... } in the right order. This stage cannot
    fail except for IdGenerator exhaustion / EngineError::Internal-class problems.)
```

The exact field shapes of `ParsedModel` and `ValidatedModel` are **your call**, with these constraints:

- `ParsedModel` mirrors the YAML 1:1. Field names match the YAML keys. Optional fields are `Option<T>`. Owned strings for everything name-shaped.
- `ValidatedModel` may rearrange (e.g., resolve string measure names to indices into a measures table) but does not allocate `mc_core::ElementId` / `MeasureId` etc. — those are still strings/indices internal to `mc-model`. ID allocation happens in `compile`.
- Both types are `pub` so Phase 4 (LLM authoring) can construct `ParsedModel` directly without YAML.

### C. The validator surface

Decision 6's 10 validators run inside `mc_model::validate` and return ALL errors at once via `Result<ValidatedModel, Vec<ValidationError>>`. Implementation pattern:

```rust
let mut errors = Vec::new();
check_duplicate_names(&parsed, &mut errors);
check_missing_dimensions(&parsed, &mut errors);
check_invalid_hierarchy_edges(&parsed, &mut errors);
check_hierarchy_cycles(&parsed, &mut errors);
check_rules_reference_known_measures(&parsed, &mut errors);
check_derived_measures_have_rules(&parsed, &mut errors);
check_input_measures_have_no_rules(&parsed, &mut errors);
check_rule_cycles(&parsed, &mut errors);
check_aggregation_methods_supported(&parsed, &mut errors);
// (golden test mismatches checked at compile/run time, not here — see test layout below)
if errors.is_empty() {
    Ok(into_validated(parsed))
} else {
    Err(errors)
}
```

Each validator pushes `ValidationError` variants with:
- A discriminant (`DuplicateName`, `MissingDimension`, ...)
- The offending name(s) / coords
- A `Span { line: usize, column: usize, file: PathBuf }` if your YAML library exposes it; `Option<Span>` if it doesn't always

The Phase 3A handoff doesn't pick the YAML library — that's your call. `serde_yaml` 0.9.x and `serde_yml` are the obvious candidates; whichever builds cleanly on Rust 1.78 with the smallest transitive footprint wins. Document your choice in the completion report.

### D. The golden test loop

Per ADR-0004 Decision 7, inline goldens live in the model YAML:

```yaml
golden_tests:
  - name: spend_at_tampa_paid_search_march
    coord:
      Scenario: "Baseline"
      Version: "Working"
      Time: "Mar_2026"
      Channel: "Paid_Search"
      Market: "Tampa"
      Measure: "Spend"
    expect: 11500.0

  - name: revenue_consolidates_to_florida_q1
    coord: { Scenario: "Baseline", Version: "Working", Time: "Q1_2026", Channel: "All_Channels", Market: "Florida", Measure: "Revenue" }
    expect_within_epsilon: { value: 1234567.89, epsilon: 1.0e-9 }
```

`crates/mc-model/tests/golden_acme.rs` loads `examples/acme.yaml`, compiles it to a `Cube`, then for each golden:
1. Resolves the named coord against the cube's dimensions/elements (string → ElementId via the `ValidatedModel`'s name → ID map).
2. Calls `cube.read(coord, principal)` to get the value.
3. Asserts equality with `expect` (or epsilon-equality with `expect_within_epsilon`).

The minimum required goldens are the brief §4.5.1 anchor values — get those byte-for-byte. Add a few consolidation-level goldens (e.g., Q1 Florida Revenue rolled up) to cover the rule-evaluation path through the YAML-loaded cube.

### E. Schema cheat-sheet (illustrative — not normative)

This is what the Acme YAML *might* look like. The Phase 3A implementation defines the actual schema; this is just to orient the receiver.

```yaml
model_format_version: 1

metadata:
  name: "Acme_MarketingFinance"
  description: "Brief §4 reference cube"
  author: "MarketingCubes V2"
  created: "2026-05-02"

dimensions:
  - name: "Scenario"
    kind: "Scenario"
    elements:
      - { id: "scen_baseline", name: "Baseline" }
      - { id: "scen_aggressive", name: "Aggressive" }
      - { id: "scen_conservative", name: "Conservative" }

  - name: "Version"
    kind: "Version"
    elements:
      - { id: "ver_working", name: "Working", state: "Working" }
      - { id: "ver_submitted", name: "Submitted", state: "Submitted" }
      - { id: "ver_approved", name: "Approved", state: "Approved" }

  # ... Time, Channel, Market, Measure ...

hierarchies:
  - dimension: "Time"
    name: "default"
    edges:
      - { child: "Jan_2026", parent: "Q1_2026", weight: 1.0 }
      - { child: "Feb_2026", parent: "Q1_2026", weight: 1.0 }
      # ...

measures:
  - { name: "Spend", role: "Input", data_type: "F64", aggregation: "Sum" }
  - { name: "Clicks", role: "Derived", data_type: "F64", aggregation: "Sum" }
  # ...

rules:
  - id: "rule_clicks"
    target: { measure: "Clicks", scope: "leaf" }
    body:
      div:
        - { ref: { measure: "Spend" } }
        - { ref: { measure: "CPC" } }
    declared_dependencies:
      - { measure: "Spend" }
      - { measure: "CPC" }
  # ... 4 more rules ...

golden_tests:
  - name: "spend_at_tampa_paid_search_march"
    coord:
      Scenario: "Baseline"
      Version: "Working"
      Time: "Mar_2026"
      Channel: "Paid_Search"
      Market: "Tampa"
      Measure: "Spend"
    expect: 11500.0
  # ... ~8 brief §4.5.1 anchor goldens ...
```

Notice every name-like value is **quoted** per the YAML safe subset. Notice `model_format_version: 1` is an integer, not `"1.0.0"` (Decision 6).

The exact field shapes (e.g., is `body` an inline map or a nested object?) are your call. **Pick a shape that's straightforward to deserialize via `serde_yaml`** without custom `Deserialize` impls if possible — the validator does the semantic work, not the deserializer.

### F. The CLI flag

`crates/mc-cli/src/main.rs` currently does (roughly):

```rust
let (mut cube, refs) = mc_fixtures::build_acme_cube().expect("build_acme_cube ok");
// ... runs the demo flow against `cube` ...
```

After Phase 3A, the demo subcommand accepts `--model <path>`:

```
cargo run --release --bin mc -- demo                                        # uses build_acme_cube() (existing path)
cargo run --release --bin mc -- demo --model <path>                          # uses mc_model::load(<path>)
```

Implementation: parse the flag (use existing CLI parsing if any, or `std::env::args` for Phase 3A simplicity — `mc-cli` is allowed `clap` if it's already there; check first). If `--model` is present, call `mc_model::load(path)`. If not, call `mc_fixtures::build_acme_cube()`. The rest of the demo flow is unchanged.

The two paths must produce **byte-for-byte identical** stdout for the Acme cube. That's the acceptance gate. If you find yourself adding a "// TODO: this differs slightly between paths" — stop. Either the YAML doesn't capture something the Rust fixture does (fix the YAML / schema) or the CLI is doing something path-dependent (fix the CLI). The diff must be empty.

### G. What Phase 4 (LLM authoring) will consume

Phase 4 doesn't exist yet, but Phase 3A is its foundation. Concretely, Phase 4 will:

1. Take a natural-language prompt.
2. Have an LLM emit YAML against the schema Phase 3A defines.
3. Call `mc_model::parse(yaml_str)` → `ParsedModel`. If the YAML is structurally malformed, surface the parse error to the LLM for re-prompting.
4. Call `mc_model::validate(parsed)` → `ValidatedModel`. If validators fail, surface the structured `ValidationError`s to the LLM for re-prompting.
5. Call `mc_model::compile(validated)` → `Cube`. Display to the user.

Phase 4 is impossible without the three-stage pipeline being well-bounded. Each stage has a different error type with a different "blame" semantics — parse errors blame the YAML syntax; validation errors blame the model semantics; compile errors blame the kernel state (and shouldn't normally happen). LLMs benefit hugely from being told *which kind* of mistake they made.

This is the strongest argument for Decision 9 in the ADR. Don't shortcut the pipeline.

---

## Pointers to existing files you will most likely touch

| Why | File | Action |
|---|---|---|
| Workspace member entry for the new crate | [`../../Cargo.toml`](../../Cargo.toml) | add `crates/mc-model` to `[workspace] members` |
| The new crate root | `crates/mc-model/Cargo.toml` | new — runtime deps: serde + chosen YAML library + small validators; dev-deps: `mc-fixtures` (path) for the structural-equivalence test |
| The crate's library root | `crates/mc-model/src/lib.rs` | new — public API exports |
| Parser, validator, compiler modules | `crates/mc-model/src/{parse,validate,compile,schema,error}/...` | new |
| The Acme YAML | `crates/mc-model/examples/acme.yaml` | new — every dim, element, hierarchy edge, measure, rule, plus `golden_tests:` section |
| Structural-equivalence test | `crates/mc-model/tests/structural_equivalence.rs` | new — uses `mc-fixtures` dev-dep |
| Per-validator unit tests | `crates/mc-model/tests/validators.rs` | new — one test per Decision 6 row |
| Inline-goldens runner | `crates/mc-model/tests/golden_acme.rs` | new |
| CLI flag | [`../../crates/mc-cli/src/main.rs`](../../crates/mc-cli/src/main.rs) | modify — add `--model <path>` arg; route through `mc_model::load` when present |
| CLI dep on mc-model | [`../../crates/mc-cli/Cargo.toml`](../../crates/mc-cli/Cargo.toml) | modify — add `mc-model` path dep |
| Phase 3A completion report | `docs/reports/phase-3a-completion-report.md` | new file (use [`../templates/phase-completion-report.md`](../templates/phase-completion-report.md)) |
| Status flips | [`../CURRENT_STATE.md`](../CURRENT_STATE.md), [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) | flip Phase 3A from `proposed` → `complete` |

**Do not touch:**

- **`crates/mc-core/`** — entire crate is locked. Source, tests, benches, Cargo.toml, all of it. The kernel is finished for Phase 3A.
- **`crates/mc-fixtures/src/`** — locked. `build_acme_cube()` stays as the canonical Rust reference. Adding code to fixtures or changing existing fixture code is out of scope. (Adding `mc-fixtures` as a dev-dependency in `mc-model/Cargo.toml` is fine; modifying `mc-fixtures` itself is not.)
- **`crates/mc-fixtures/Cargo.toml`** — do not add `mc-model` as a normal dep. Per Decision 3, `mc-fixtures → mc-model` is forbidden in non-test code without owner approval.
- **`docs/specs/`** — locked. Brief and engine-semantics doc are contracts.
- **`rust-toolchain.toml`** — pinned at 1.78. If your YAML library forces a bump, **stop and propose ADR-0005**. Do not bump unilaterally.
- **`Cargo.lock` (the existing pins)** — `clap`, `clap_lex`, `half` pins from Phase 1B stay. Do not run `cargo update` even if `cargo` suggests it.
- **ADR-0004** — Accepted. Amendments go in `0004-amendment-N.md`, not in the original.
- **PERF.md** — Phase 3A doesn't touch performance documentation. The kernel didn't change; benches don't need to be re-run.

---

## Reproducible commands you can rely on

```bash
cd /Users/edwinlovettiii/Projects/mc-v2

# (only if your shell didn't initialize rustup)
source $HOME/.cargo/env

# Pre-3A gate — must remain green throughout
cargo build --release --workspace
cargo fmt --check --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                  # 227 / 0 (Phase 2D's count)
cargo run --release --bin mc -- demo    # matches brief §4.6

# Initial dep audit (after writing crates/mc-model/Cargo.toml):
cargo tree -p mc-model --edges normal,build | head -50      # what does mc-model pull in?
cargo tree -p mc-core --edges normal | head -20             # confirm mc-core deps are unchanged

# Iteration loop during Phase 3A development:
cargo build -p mc-model
cargo test -p mc-model
cargo test -p mc-model -- validators       # negative tests for Decision 6's 10 validators
cargo test -p mc-model -- structural       # YAML vs build_acme_cube() structural diff
cargo test -p mc-model -- golden           # inline goldens runner

# Acceptance gate — the empty-diff check:
diff <(cargo run --release --bin mc -- demo) \
     <(cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml)
# expected: zero output (exit 0)

# Determinism gate (10 runs, identical pass/fail):
for i in $(seq 1 10); do cargo test --workspace -q || echo "FAIL run $i"; done

# Verify mc-core got no new deps:
cat crates/mc-core/Cargo.toml
# expected: smallvec, ahash, thiserror, once_cell — and that's it
grep -E "^(serde|tokio|rayon|anyhow|.*yaml)" crates/mc-core/Cargo.toml
# expected: zero matches
```

---

## Final checklist before you call Phase 3A done

- [ ] `crates/mc-model/` exists as a workspace member with the public API surface above.
- [ ] YAML 1.2 safe subset enforced — no anchors, no merge keys, no custom tags (rejected at parse OR validation time).
- [ ] Three-stage pipeline implemented: `YAML → ParsedModel → ValidatedModel → Cube`. No shortcut from YAML directly into `CubeBuilder`.
- [ ] `crates/mc-model/examples/acme.yaml` exists with every dim / element / hierarchy edge / measure / rule from `build_acme_cube()`.
- [ ] All string-like values in `acme.yaml` are quoted (IDs, dates, version strings, enum-like values per ADR-0004 Decision 1).
- [ ] `model_format_version: 1` (integer) at the top of `acme.yaml`.
- [ ] Inline `golden_tests:` block in `acme.yaml` covers the brief §4.5.1 anchor values byte-for-byte (epsilon-tolerant where the brief specifies).
- [ ] All 10 Decision 6 validators implemented, each with a negative unit test.
- [ ] Structural-equivalence test passes — YAML-loaded cube matches `build_acme_cube()` on dim count, element counts, hierarchy edges, measure metadata, rule shape.
- [ ] `mc-cli` accepts `--model <path>` and routes to `mc_model::load`.
- [ ] **Acceptance gate met:** `diff <(cargo run --release --bin mc -- demo) <(cargo run --release --bin mc -- demo --model crates/mc-model/examples/acme.yaml)` produces zero output.
- [ ] All 227 existing tests still pass; new total ≥ 227 + (`mc-model` test count).
- [ ] 10 consecutive `cargo test --workspace -q` runs identical.
- [ ] **`mc-core` Cargo.toml unchanged** — same 4 runtime deps (`smallvec`, `ahash`, `thiserror`, `once_cell`).
- [ ] **`mc-fixtures` not modified** — `build_acme_cube()` byte-for-byte unchanged, no new normal-deps on `mc-model`.
- [ ] **`rust-toolchain.toml` not bumped** — still Rust 1.78. If the YAML library required a bump, ADR-0005 was opened and Accepted *before* this checkbox was checked.
- [ ] **`Cargo.lock`** — Phase 1B pins (`clap → 4.4.18`, `clap_lex → 0.6.0`, `half → 2.4.1`) intact. New `mc-model` deps may add new lock entries; existing entries unchanged.
- [ ] No `unwrap()` / `expect()` / `panic!()` in `crates/mc-model/src/` (test/example code is exempt; production paths return `Result`).
- [ ] No `unsafe` anywhere.
- [ ] No `async` / `tokio` / `rayon` / threads anywhere.
- [ ] Completion report at `docs/reports/phase-3a-completion-report.md` written from template.
- [ ] CURRENT_STATE.md and MASTER_PHASE_PLAN.md updated to flip Phase 3A from `proposed` → `complete`.
- [ ] **You did NOT commit, tag, or push.** The user does that after reading the review.
- [ ] **You did NOT start Phase 3B (linter), Phase 3C (formula strings), or Phase 4 (LLM).**

If you are uncertain at any point, the resolution order is:

1. The Phase 3A prompt above.
2. **[ADR-0004](../decisions/0004-phase-3a-model-definition-format.md) — the binding strategic contract.**
3. The brief and `engine-semantics.md` for kernel-side semantics that the schema must respect.
4. Phase 2D completion report (recent context).
5. Earlier completion reports (1A / 1B / 2A / 2B / 2C).
6. `CLAUDE.md`.
7. `docs/roadmap/MASTER_PHASE_PLAN.md`.
8. Anything else.

If those don't resolve it: stop, write a SPEC QUESTION per CLAUDE.md §11, and wait. Don't guess.

---

## Operating principles (carry-forward from Phase 2D)

**Read the ADR before you write the code.** ADR-0004's nine Decisions are the contract. Anything in the prompt above is a derivation; if a derivation seems to contradict the ADR, the ADR wins and the prompt is buggy — surface it.

**Source-bounded, but the bound is "the new crate."** This is the first phase whose source bound is "everything in `crates/mc-model/`" rather than "this one file in `mc-core/`." That's a wider surface than 2D had, but the boundary is just as hard: kernel and fixtures are off-limits.

**The acceptance gate is the diff.** Two demo invocations producing byte-for-byte identical stdout is the headline. Everything else (validators, structural diffs, inline goldens) is supporting evidence. If the diff isn't empty, you don't ship.

**Validators are the contract LLMs will be checked against.** Phase 4 emits YAML and learns from validator errors. Skimping on validator coverage in Phase 3A directly hurts Phase 4. Decision 6's table is the floor, not a ceiling.

**Three stages, three error types, three blame surfaces.** `ParseError` blames YAML syntax. `ValidationError` blames model semantics. `EngineError` blames the kernel. Don't merge them. Phase 4 (LLM authoring) and Phase 6 (UI editor) both depend on the blame surfaces being distinct.

**A bench is a contract, not a draft — but Phase 3A has no benches.** `mc-model` is not on a hot path; loading a cube happens once at startup. The kernel's PERF.md / bench-data baselines are unchanged. If you find yourself wanting to add a `mc-model` benchmark, **stop and check** — that's Phase 3B (linter) territory or pure optimization scope-creep.

**Do not pick the next phase.** Phase 3A's deliverable is the YAML-loadable Acme cube + the parser + validator. If the work surfaces opportunities for Phase 3B (linter), Phase 3C (formula strings), or other follow-ons, note them in the completion report's "follow-up candidates" section — do not start them.

---

*Phase 3A handoff drafted 2026-05-02 immediately after [ADR-0004](../decisions/0004-phase-3a-model-definition-format.md) was Accepted with project-owner amendments. The handoff is the build contract; the ADR is the strategic context behind it.*
