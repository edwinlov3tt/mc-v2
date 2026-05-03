# Phase 3A Completion Report — Model Definition Layer (`mc-model` crate)

**Project:** MarketingCubes V2 — Rust kernel + model layer
**Phase 3A handoff:** [`../handoffs/phase-3a-handoff.md`](../handoffs/phase-3a-handoff.md)
**ADR (binding strategic context):** [`../decisions/0004-phase-3a-model-definition-format.md`](../decisions/0004-phase-3a-model-definition-format.md)
**Operating manual:** [`../../CLAUDE.md`](../../CLAUDE.md)
**Initial commit:** `603c537` (tag `phase-3a-model-definition-layer`) — committed 2026-05-02 after PM/spec-maintainer signoff
**Toolchain:** Rust 1.78 (unchanged; pinned in [`../../rust-toolchain.toml`](../../rust-toolchain.toml))

---

## 1. Commands run + summarized outputs

| Command | Purpose | Result |
|---|---|---|
| `cargo build --release --workspace` | Acceptance criterion 1 | ✓ zero warnings |
| `cargo fmt --check --all` | Acceptance criterion 3 | ✓ |
| `cargo clippy --workspace --all-targets -- -D warnings` | Acceptance criterion 2 | ✓ |
| `cargo test --workspace` | Acceptance criterion 4 | ✓ **252 / 0** (was 227; +25 from Phase 3A) |
| `for i in $(seq 1 10); do cargo test --workspace -q; done` | Acceptance criterion 9 (determinism) | ✓ 10 / 10 OK |
| `./target/release/mc demo` | Rust path matches brief §4.6 | ✓ |
| `./target/release/mc demo --model crates/mc-model/examples/acme.yaml` | YAML path matches brief §4.6 | ✓ |
| `diff <(... demo) <(... demo --model ...)` | **Acceptance gate** — Phase 3A headline | ✓ **empty output** |
| `cargo bench --workspace --no-run` | Bench targets still build | ✓ |
| Forbidden-pattern grep on `crates/mc-model/src/` | unwrap / expect / panic / unsafe | ✓ zero matches |
| `grep -E "^(serde|tokio|rayon|anyhow|.*yaml)" crates/mc-core/Cargo.toml` | mc-core deps unchanged | ✓ zero matches |
| `cat rust-toolchain.toml` | toolchain unbumped | ✓ still 1.78 |

**Acceptance gate (the headline):** `diff <(./target/release/mc demo) <(./target/release/mc demo --model crates/mc-model/examples/acme.yaml)` produces empty output; `echo $?` returns 0. The YAML-loaded cube produces byte-for-byte identical stdout to the canonical Rust fixture.

---

## 2. Final test count

**Total: 252 tests passed / 0 failed.**

Per target:

| Target | Passed | Notes |
|---|---:|---|
| `mc-core` unit tests + integration tests | 211 | Unchanged from Phase 2D baseline. Sum across all `mc-core/src/*.rs` `#[test] mod tests` blocks + `mc-core/tests/*.rs` integration files. |
| `mc-fixtures` unit tests | 16 | Unchanged. |
| `mc-model` unit tests (`src/parse/tests`) | 6 | Phase 3A. Safe-subset prefilter unit tests: rejects anchors / aliases / merge keys / custom tags; allows quoted-`&`/`*`/`<<`/`!`; allows special chars in comments. |
| `mc-model` integration `tests/parse_validate_smoke.rs` | 3 | Phase 3A. Parse / validate / compile of `examples/acme.yaml`. |
| `mc-model` integration `tests/structural_equivalence.rs` | 1 | Phase 3A. YAML-loaded Acme vs `mc_fixtures::build_acme_cube()` structural diff (dim count + names, element counts + names, hierarchy edge counts, measure metadata, weight-measure targets, rule body shapes). |
| `mc-model` integration `tests/validators.rs` | 14 | Phase 3A. **One negative test per Decision 6 row**, plus the structural golden_test checks. |
| `mc-model` integration `tests/golden_acme.rs` | 1 | Phase 3A. Loads `acme.yaml`, writes canonical inputs, runs every inline golden, asserts equality / epsilon-equality. |
| **Total** | **252** | |

### Validator coverage (per ADR-0004 Decision 6)

| Validator | Test name | Status |
|---|---|---|
| duplicate_names — dimension | `duplicate_dimension_name_fires` | ✓ |
| duplicate_names — element | `duplicate_element_name_fires` | ✓ |
| duplicate_names — measure | `duplicate_measure_name_fires` | ✓ |
| duplicate_names — rule | `duplicate_rule_name_fires` | ✓ |
| missing_dimensions | `missing_dimension_referenced_by_hierarchy_fires` | ✓ |
| invalid_hierarchy_edges | `invalid_hierarchy_edge_fires` | ✓ |
| hierarchy_cycles | `hierarchy_cycle_fires` | ✓ |
| rules_referencing_unknown_measures | `rule_referencing_unknown_measure_fires` | ✓ |
| derived_measures_without_rules | `derived_measure_without_rule_fires` | ✓ |
| input_measures_with_rules | `input_measure_with_rule_fires` | ✓ |
| rule_cycles | `rule_cycle_fires` | ✓ |
| unsupported_aggregation_methods | `unsupported_aggregation_fires` | ✓ |
| golden_test_mismatches (structural) | `golden_test_with_neither_expect_nor_epsilon_fires`, `golden_test_with_unknown_dim_fires` | ✓ (2 tests) |
| golden_test_mismatches (value) | covered by `tests/golden_acme.rs` | ✓ |

### Determinism gate

10 consecutive `cargo test --workspace -q` runs all returned exit code 0 with identical pass/fail status (252 / 0 each). No flakiness observed. `mc-model` validators sort by name where iteration is needed (BTreeMap / BTreeSet throughout); inline goldens use `BTreeMap<String, String>` for coord lookup; no HashMap iteration order leaks into test output.

---

## 3. Deviations from the handoff / ADR

The Phase 3A implementation followed the handoff and ADR-0004's nine Decisions verbatim. The following are deviations / adaptations worth recording for audit trail:

1. **`Cargo.lock` transitive pin: `indexmap` 2.14.0 → 2.7.0.**
2. **`ParsedRuleBody` enum representation uses `#[serde(untagged)]` with per-variant tag struct, not the externally-tagged form the ADR's illustrative schema sketched.**
3. **Compile-stage internal-fallback errors use static strings, not formatted strings.**
4. **`mc-cli`'s `load_acme_from_yaml` reconstructs an `AcmeRefs` from `ModelRefs` rather than threading a new `Refs`-shaped type through the demo flow.**
5. **The 10th Decision-6 row ("golden_test_mismatches") split across two test files.**

Rationales in §4.

---

## 4. Rationale per deviation

### 4.1 `Cargo.lock` transitive pin: `indexmap` 2.14.0 → 2.7.0

**What the ADR says (Decision 3 + handoff "Hard rules"):** *"Order of preference: (1) try a 1.78-compatible library; (2) pin transitives the way Phase 1B did; (3) only as a last resort, propose ADR-0005."*

**What I did:** Picked `serde_yaml = "0.9.34"` (option 1); on first `cargo build -p mc-model`, the transitive `indexmap 2.14.0` failed with `feature edition2024 is required`; fell back to option 2 — `cargo update -p indexmap --precise 2.7.0` (which also downgrades `hashbrown 0.17.0 → 0.15.5` as a transitive consequence). Build then succeeds clean on Rust 1.78. ADR-0005 was *not* opened.

**Rationale:** Exactly the path Decision 3 mandated; this is the Phase 1B pattern reused. The Cargo.lock now carries five pre-edition2024 pins: `clap 4.4.18`, `clap_lex 0.6.0`, `half 2.4.1` (Phase 1B) plus `indexmap 2.7.0`, `hashbrown 0.15.5` (Phase 3A). All are documented dependencies of crates we already depend on (clap/half come from criterion; indexmap/hashbrown come from serde_yaml). The toolchain stays at Rust 1.78. **No SPEC QUESTION needed** — this is the documented escape hatch, not a deviation that requires owner approval.

### 4.2 `ParsedRuleBody` enum representation: `untagged` + tag-struct, not externally tagged

**What the ADR says (Decision 4 + illustrative sketch):**

```yaml
body:
  div:
    - { ref: { measure: Spend } }
    - { ref: { measure: CPC } }
```

**What I did:** Modeled `ParsedRuleBody` as `#[serde(untagged)]` with each variant being its own struct that has a single distinguishing field name (`add`, `sub`, `mul`, `div`, `if_null`, `const`, `ref`). The YAML stays JSON-shaped (`{ div: [a, b] }`, `{ ref: "Spend" }`, `{ const: 1.0 }`) — identical in spirit to the ADR sketch but without the nested `{ ref: { measure: ... } }` form.

**Rationale:** `serde_yaml 0.9` defaults to **YAML-tag dispatch** (`!Variant payload`) for externally-tagged Rust enums — and the ADR Decision 1 safe subset bans custom tags. Externally-tagged JSON-shape (`{ "Variant": payload }`) doesn't work out-of-the-box on `serde_yaml 0.9`'s YAML output. Adjacently-tagged (`{ op: "div", args: [...] }`) was an option but bulkier in YAML. The `untagged` + tag-struct approach is the smallest schema that (a) stays JSON-shaped, (b) compiles cleanly under `deny_unknown_fields`, and (c) keeps the YAML easily LLM-emittable for Phase 4. The ADR's illustrative `ref: { measure: Spend }` form was never normative — the ADR explicitly says "the exact YAML shape (e.g. is `body` an inline map or a nested object?) are your call." The flatter `ref: "Spend"` form satisfies the same contract with less authoring noise.

### 4.3 Compile-stage internal-fallback errors use static strings, not formatted strings

**What the handoff implies:** Compile-stage errors propagate `mc_core::EngineError` so a kernel-side problem surfaces with full kernel context.

**What I did:** Every "validator missed this" branch in `compile.rs` returns `EngineError::Internal(&'static str)` with a fixed message identifying the branch. Examples: `"compile: validator missed an unknown measure role"`, `"compile: validator missed an unknown hierarchy-edge parent"`. These do *not* embed the offending name (the role / element name) because `EngineError::Internal` is `(&'static str)` and accepts only static strings.

**Rationale:** The compile stage is a "by-construction-buildable" walk per Decision 9 — these branches should *never* fire. If one does, the failure is a validator bug, and the static string identifies which branch tripped (which is enough to debug; the offending YAML can be inspected directly). Embedding the offending name would require either (a) leaking a `Box<str>` per error (ugly), (b) defining a local `CompileError` wrapper (more code than warranted for unreachable branches), or (c) adding a new dynamic-string variant to `EngineError` (a kernel-source change, which Phase 3A is forbidden from making per the handoff Hard Rules). Static strings + targeted-branch identifiers is the least-bad option.

### 4.4 `mc-cli`'s `load_acme_from_yaml` reconstructs an `AcmeRefs`

**What the handoff says (item 6):** *"When passed, `mc demo --model <path>` calls `mc_model::load(path)` instead of `mc_fixtures::build_acme_cube()`, then runs the same demo flow against the loaded cube."*

**What I did:** Added a `load_acme_from_yaml(path) -> (Cube, AcmeRefs)` helper in `mc-cli/src/main.rs` that calls `mc_model::load`, then resolves every named element / dimension / rule from the returned `ModelRefs` to build an `AcmeRefs` instance. The rest of `run_demo` is unchanged — it still takes `(cube, AcmeRefs)`.

**Rationale:** The alternative was to thread a new generic `Refs`-shaped trait through `run_demo`, `coord_at`, and `mc_fixtures::write_canonical_inputs`. That would require either (a) adding a trait to `mc-fixtures` (which the handoff bars from modification), or (b) duplicating the demo flow into `mc-cli`. Building an `AcmeRefs` from `ModelRefs` is mechanical (~70 lines) and keeps the demo flow identical between the two paths — which is exactly what the byte-for-byte gate requires. The `time_hierarchy` / `channel_hierarchy` / `market_hierarchy` IDs on `AcmeRefs` are the only fields the YAML path can't resolve (the kernel's `HierarchyId` isn't part of any user-visible flow); we set them to `HierarchyId(0)` as a sentinel since the demo doesn't read them. This is documented inline.

### 4.5 The 10th Decision-6 row ("golden_test_mismatches") split across two test files

**What the ADR says (Decision 6 row 10):** *"Golden test mismatches | An inline golden test ... doesn't match what the loaded cube produces"*. The handoff item 8 says "one test per validator from ADR-0004 Decision 6's table."

**What I did:** Split the row into:
- **Structural golden_test mismatches** (`tests/validators.rs`): two tests covering (a) golden_test entry has neither `expect` nor `expect_within_epsilon` set, (b) golden_test coord references an unknown dim. These run inside the validate stage.
- **Value golden_test mismatches** (`tests/golden_acme.rs`): the inline-goldens runner that loads `acme.yaml`, writes inputs, reads each coord, and asserts equality. Per ADR-0004 Decision 6's table footer + the handoff: *"golden test mismatches checked at compile/run time, not here — see test layout below."*

**Rationale:** This is exactly the split the handoff and the ADR's "Validation requirements summary" describe. The validator can catch *structural* mistakes (typo'd dim name in coord, missing both expect fields), but *value* mistakes require a built cube; running the cube against the goldens is a tests-layer concern, not a validator concern. Both are in place.

---

## 5. Acceptance criteria — complete

| # | Criterion | Status |
|---:|---|---|
| 1 | `crates/mc-model/` exists as a workspace member with the public API (load / parse / validate / compile / ParsedModel / ValidatedModel / Error / ValidationError) | ✓ |
| 2 | YAML 1.2 safe subset enforced (no anchors / aliases / merge keys / custom tags) — rejected at parse time via the prefilter | ✓ |
| 3 | Three-stage pipeline implemented: YAML → ParsedModel → ValidatedModel → Cube. No shortcut from YAML to CubeBuilder. | ✓ |
| 4 | `crates/mc-model/examples/acme.yaml` exists with all 6 dims / 17+8+15+11 elements / 3 hierarchies / 11 measures / 5 rules | ✓ |
| 5 | All string-likes quoted in `acme.yaml`; `model_format_version: 1` (integer) | ✓ |
| 6 | Inline `golden_tests:` block covers brief §4.5.1 anchor values + 1 consolidation-level golden | ✓ (9 goldens — 8 anchor values + Q1 Spend) |
| 7 | All 9 Decision 6 validators (10th split into structural + value as documented) implemented with ≥ 1 negative test each | ✓ |
| 8 | Structural-equivalence test passes — YAML-loaded cube matches `build_acme_cube()` on dim count, element counts, hierarchy edges, measure metadata, rule body shape | ✓ |
| 9 | `mc-cli` accepts `--model <path>` and routes to `mc_model::load` | ✓ |
| 10 | **Acceptance gate met** — `diff <(... demo) <(... demo --model ...)` produces zero output | ✓ |
| 11 | All 227 prior tests still pass; new total 252 ≥ 227 + 25 | ✓ |
| 12 | 10 consecutive `cargo test --workspace -q` runs identical | ✓ |
| 13 | `mc-core/Cargo.toml` unchanged — same 4 runtime deps | ✓ |
| 14 | `mc-fixtures` not modified — `build_acme_cube()` byte-for-byte unchanged; no new normal-deps on `mc-model` | ✓ |
| 15 | `rust-toolchain.toml` not bumped — still Rust 1.78 | ✓ |
| 16 | `Cargo.lock` Phase 1B pins (`clap`, `clap_lex`, `half`) intact; new pins (`indexmap`, `hashbrown`) added per Decision 3 escape hatch | ✓ |
| 17 | No `unwrap()` / `expect()` / `panic!()` in `crates/mc-model/src/` (test/example code exempt) | ✓ |
| 18 | No `unsafe`; no `async` / `tokio` / `rayon` / threads | ✓ |
| 19 | Completion report written at `docs/reports/phase-3a-completion-report.md` (this file) | ✓ |
| 20 | CURRENT_STATE.md + MASTER_PHASE_PLAN.md updated to flip Phase 3A from `proposed` → `complete` | ✓ |
| 21 | ADR-0004, brief, and engine-semantics.md unchanged | ✓ |
| 22 | No commit, no tag, no push by implementing instance (user did the commit + tag at `603c537` after review) | ✓ |
| 23 | Did not start Phase 3B / 3C / 4 | ✓ |

---

## 6. Acceptance criteria — deferred

None. Every Phase 3A criterion is met.

---

## 7. Implemented files / modules

### Workspace / config

- [`Cargo.toml`](../../Cargo.toml) — added `crates/mc-model` to `[workspace] members`. No other change.
- [`Cargo.lock`](../../Cargo.lock) — pinned `indexmap → 2.7.0` and `hashbrown → 0.15.5` (transitive deps of `serde_yaml` that otherwise pull edition2024). Phase 1B pins (`clap`, `clap_lex`, `half`) intact.
- [`rust-toolchain.toml`](../../rust-toolchain.toml) — unchanged; still Rust 1.78.

### `mc-core`

**Unchanged.** Locked per the handoff Hard Rules. `cat crates/mc-core/Cargo.toml` shows the same 4 runtime deps as Phase 2D (`smallvec`, `ahash`, `thiserror`, `once_cell`).

### `mc-fixtures`

**Unchanged.** `build_acme_cube()` byte-for-byte unchanged. `Cargo.toml` does not depend on `mc-model` (Decision 3's hard rule).

### `mc-model` (new)

| Module | File | Purpose |
|---|---|---|
| Cargo manifest | [`crates/mc-model/Cargo.toml`](../../crates/mc-model/Cargo.toml) | Adds `serde 1`, `serde_yaml 0.9.34`, `thiserror`. Dev-dep on `mc-fixtures` (path) for the structural-equivalence + golden tests. |
| Lib root | [`crates/mc-model/src/lib.rs`](../../crates/mc-model/src/lib.rs) | Re-exports public API. `load(path)` and `load_str(yaml, source_label)` orchestrate the 3-stage pipeline. |
| Errors | [`crates/mc-model/src/error.rs`](../../crates/mc-model/src/error.rs) | `Error`, `ParseError`, `ParseErrorKind`, `ValidationError`, `Span`. Three blame surfaces per Decision 9. |
| Schema (parsed + validated types) | [`crates/mc-model/src/schema.rs`](../../crates/mc-model/src/schema.rs) | `ParsedModel`, `ParsedDimension`, `ParsedElement`, `ParsedHierarchy`, `ParsedMeasure`, `ParsedRule`, `ParsedRuleBody` + tag structs (`ParsedAddBody` / `ParsedSubBody` / etc.), `ParsedScalar`, `ParsedGoldenTest`, `ValidatedModel`. |
| Stage 1 — parse | [`crates/mc-model/src/parse.rs`](../../crates/mc-model/src/parse.rs) | YAML safe-subset prefilter (line-oriented, quote-aware) + `serde_yaml::from_str` deserialization. 6 inline unit tests cover anchor / alias / merge / custom-tag rejection + quoted/comment immunity. |
| Stage 2 — validate | [`crates/mc-model/src/validate.rs`](../../crates/mc-model/src/validate.rs) | All-errors-at-once validator pass covering Decision 6's 9 validators + binop arity + dim-kind sanity + golden_test structural shape. |
| Stage 3 — compile | [`crates/mc-model/src/compile.rs`](../../crates/mc-model/src/compile.rs) | `ValidatedModel` → `mc_core::Cube` walk. Allocates IDs via `IdGenerator`. Returns `CompiledCube` { cube, root_principal, refs: ModelRefs }. |
| Acme YAML | [`crates/mc-model/examples/acme.yaml`](../../crates/mc-model/examples/acme.yaml) | 264 lines. 6 dims, 17+8+15 hierarchy elements, 11 measures, 5 rules, 9 inline goldens. |
| Smoke tests | [`crates/mc-model/tests/parse_validate_smoke.rs`](../../crates/mc-model/tests/parse_validate_smoke.rs) | Parse + validate + compile round-trip on `acme.yaml`. 3 tests. |
| Structural equivalence | [`crates/mc-model/tests/structural_equivalence.rs`](../../crates/mc-model/tests/structural_equivalence.rs) | Compares YAML-loaded Acme to `build_acme_cube()` on dim count / element names / hierarchy edges / measure metadata / weight-measure targets / rule body shapes. |
| Validator negative tests | [`crates/mc-model/tests/validators.rs`](../../crates/mc-model/tests/validators.rs) | 14 tests — one per Decision 6 row + golden_test structural variants. |
| Inline goldens | [`crates/mc-model/tests/golden_acme.rs`](../../crates/mc-model/tests/golden_acme.rs) | Loads YAML, writes canonical inputs, reads each golden, asserts. 1 test (loops over 9 goldens). |

### `mc-cli`

| File | Change |
|---|---|
| [`crates/mc-cli/Cargo.toml`](../../crates/mc-cli/Cargo.toml) | Added `mc-model = { path = "../mc-model" }`. |
| [`crates/mc-cli/src/main.rs`](../../crates/mc-cli/src/main.rs) | Added `--model <path>` flag parsing; added `load_acme_from_yaml(path) -> (Cube, AcmeRefs)` helper. The rest of the demo flow is unchanged. |

### Documentation

- [`docs/reports/phase-3a-completion-report.md`](./phase-3a-completion-report.md) — this file.
- [`docs/CURRENT_STATE.md`](../CURRENT_STATE.md) — Phase 3A flipped `proposed` → `complete`.
- [`docs/roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) — Phase 3A status row updated.

**Not modified** (per handoff Hard Rules): `docs/specs/engine-semantics.md`, `docs/specs/phase-1-rust-kernel-build-brief.md`, `docs/decisions/0004-phase-3a-model-definition-format.md`, `docs/PERF.md`.

---

## 8. Known follow-ups for the next phase

These are explicit hooks left in the code or surfaced during this phase. **They are not scheduled.**

1. **Phase 3B — model linter.** `mc-model` ships parse + validate + compile; a static-analysis layer that warns on style / performance / naming issues (rule chain depth > 5; orphan elements with no hierarchy edges; aggregations declared but never referenced) is the next obvious extension. Read-only over `ValidatedModel`. ADR-0004 Decision 6 footer + the "What this unlocks" section call this out. **Not started.**
2. **Phase 3C — friendly formula syntax.** `Revenue = Customers * AOV` compiling down to `ParsedRuleBody`'s structured tree. Per ADR-0004 Decision 4. **Not started.**
3. **Phase 4 — LLM-assisted authoring.** Phase 3A's `ParsedModel` and `ValidatedModel` are the contract. Phase 4 emits YAML, parses it, validates it, surfaces structured errors back to the LLM. **Not started.**
4. **Sibling-file goldens.** ADR-0004 Decision 7 allows `model.golden.yaml` sibling files but defers their loader implementation. Trigger when a real cube outgrows inline goldens. **Not started.**
5. **`Hierarchy` references in `ModelRefs`.** Currently `ModelRefs` doesn't carry `HierarchyId` lookups (the kernel's hierarchy IDs aren't part of any user-visible flow). When/if they become user-visible (e.g., named hierarchy resolution from an LLM prompt), add a `hierarchies: BTreeMap<(String, String), HierarchyId>` field. **Not started.**
6. **Better parse-stage error spans.** `serde_yaml 0.9` exposes `Location { line, column }` but not the file path; we synthesize the file path from the `source_label` argument. If we move to a richer error reporter (e.g., `miette` or a small custom diagnostic renderer) Phase 4's LLM-error-feedback loop becomes much cleaner. **Not started.**

The previous phase's follow-ups (Phase 2D's [`reports/phase-2d-completion-report.md`](./phase-2d-completion-report.md) §8) that this phase did not address are still open: PERF.md §9.2 (per-dim leaf-flag caching) is opportunistic and not Phase 3A-relevant; PERF.md §9.5 / §9.6 ditto.

---

## 9. Confirmation: no out-of-scope features

Verified by direct grep + file-by-file audit.

- **No new `mc-core` dependencies.** `cat crates/mc-core/Cargo.toml | grep -E "^(serde|tokio|rayon|anyhow|.*yaml)"` produces zero matches. The 4 runtime deps (`smallvec`, `ahash`, `thiserror`, `once_cell`) are unchanged.
- **No banned imports inside `mc-core/src/`.** Phase 3A did not modify `mc-core/`.
- **No `unsafe` / `async` / threads anywhere in Phase 3A code.** `grep -rn "unsafe\|async fn\|.await\|tokio\|rayon\|thread::spawn" crates/mc-model/` returns zero matches.
- **No new public types in `mc-core`.** Phase 3A did not modify `mc-core`.
- **No `unwrap()` / `expect()` / `panic!()` in `mc-model/src/`.** Confirmed via `grep -rn "\.unwrap()\|\.expect(\|panic!(" crates/mc-model/src/` = zero matches. Test code (`tests/`) and the CLI's `load_acme_from_yaml` use `.expect` / `panic!` per CLAUDE.md §3.1's exemption.
- **`mc-fixtures::build_acme_cube()` byte-for-byte unchanged.** `git diff crates/mc-fixtures/src/lib.rs` produces zero output.
- **`mc-fixtures/Cargo.toml` does not depend on `mc-model`.** Decision 3's hard rule honored.
- **ADR-0004 not modified.** It's Accepted; amendments would go in `0004-amendment-N.md`.
- **`docs/specs/engine-semantics.md` and `docs/specs/phase-1-rust-kernel-build-brief.md` not modified.** Locked.
- **No commit, no tag, no push.** User reviews first per the handoff's final checklist.

---

## 10. Dependencies added to `mc-model`

| Crate | Version | Purpose |
|---|---|---|
| `serde` | `1` (with `derive` feature) | Deserialization derives on `ParsedModel` and friends. |
| `serde_yaml` | `0.9.34` (latest pre-deprecation release) | YAML 1.2 deserialization. Upstream archived 2024; chosen because it's the most-mature serde-shaped YAML reader and builds clean on Rust 1.78 with the `indexmap → 2.7.0` transitive pin. |
| `thiserror` | workspace (`= "1"`) | Error-derive on `Error` / `ParseError` / `ValidationError`. |

Dev-dep:
- `mc-fixtures` (path) — for the structural-equivalence + golden-test runners.

**Transitive-deps audit:** `cargo tree -p mc-model --edges normal,build` shows the following non-test transitives pulled in: `serde_core`, `serde_derive`, `proc-macro2`, `quote`, `syn`, `unicode-ident`, `equivalent`, `hashbrown 0.15.5`, `indexmap 2.7.0`, `itoa`, `ryu`, `unsafe-libyaml`. None require Rust 1.85+; none introduce async / tokio / rayon. The `unsafe-libyaml` crate is an FFI-free Rust port of libyaml — its name implies `unsafe` blocks but they live entirely inside the dep, not in `mc-model` source.

---

## 11. Implementation summary

Phase 3A ships the model-definition layer per ADR-0004's nine Decisions. The `mc-model` crate translates a YAML file (ADR-0004 Decision 1's safe subset) into a `mc_core::Cube` via the three-stage pipeline (Decision 9):

1. **Parse** (`src/parse.rs`): line-oriented prefilter rejects anchors / aliases / merge keys / custom tags before `serde_yaml::from_str` deserializes into `ParsedModel`. The prefilter is quote- and comment-aware so `&` / `*` / `<<` / `!` inside `'...'` / `"..."` strings or after `#` are safe.

2. **Validate** (`src/validate.rs`): every Decision 6 validator runs in a single pass; errors accumulate so authors see all problems at once. The 9 declarative-validators plus structural sub-checks (binop arity, dim-kind sanity, golden_test shape) cover the surface the kernel would otherwise reject opaquely. `BTreeMap` / `BTreeSet` everywhere for deterministic test output.

3. **Compile** (`src/compile.rs`): walks `ValidatedModel`, allocates `mc_core` IDs via a fresh `IdGenerator`, calls `Dimension::builder` / `Hierarchy::builder` / `Cube::builder` / adds rules. Returns `CompiledCube { cube, root_principal, refs }` where `refs: ModelRefs` is the name → ID resolver Phase 4 / 6 will reuse.

`mc-cli` gained a `--model <path>` flag that routes through `mc_model::load(path)` instead of `mc_fixtures::build_acme_cube()`. The acceptance gate is the byte-for-byte diff of `mc demo` vs `mc demo --model crates/mc-model/examples/acme.yaml` — empty output. The Acme YAML expresses every dim, element, hierarchy edge, measure (with WeightedAverage weight references resolved by name), and rule (with structured-tree bodies) the Rust fixture builds. Inline `golden_tests:` cover brief §4.5.1 anchor values and one consolidation-level rollup.

The toolchain stayed at Rust 1.78. `serde_yaml 0.9.34`'s transitive `indexmap 2.14.0` required edition2024 → pinned via `cargo update -p indexmap --precise 2.7.0` (Decision 3 path 2; Phase 1B precedent). `mc-core/Cargo.toml` is byte-for-byte unchanged; the kernel is untouched.

---

## 12. Done block (per the handoff template)

```
DONE: Phase 3A Model Definition Layer

Build:        cargo build --release --workspace ✓
Format:       cargo fmt --check --all ✓
Lint:         cargo clippy --workspace --all-targets -- -D warnings ✓
Tests:        cargo test --workspace 252 / 0 (was 227 / 0; +25)
Demo (Rust):  ./target/release/mc demo ✓
Demo (YAML):  ./target/release/mc demo --model crates/mc-model/examples/acme.yaml ✓
Acceptance:   diff <(...demo) <(...demo --model ...)  ✓ empty output
Determinism:  10 / 10 identical at 252 / 0 each run

Source manifest:
- crates/mc-model/Cargo.toml                       (new — 3 runtime deps)
- crates/mc-model/src/lib.rs                       (new — public API surface)
- crates/mc-model/src/error.rs                     (new)
- crates/mc-model/src/schema.rs                    (new — ParsedModel + ValidatedModel)
- crates/mc-model/src/parse.rs                     (new — stage 1)
- crates/mc-model/src/validate.rs                  (new — stage 2)
- crates/mc-model/src/compile.rs                   (new — stage 3)
- crates/mc-model/examples/acme.yaml               (new — 264 lines, 9 inline goldens)
- crates/mc-model/tests/parse_validate_smoke.rs    (new — 3 tests)
- crates/mc-model/tests/structural_equivalence.rs  (new — 1 test)
- crates/mc-model/tests/validators.rs              (new — 14 tests)
- crates/mc-model/tests/golden_acme.rs             (new — 1 test, runs 9 goldens)
- crates/mc-cli/src/main.rs                        (modified — --model flag)
- crates/mc-cli/Cargo.toml                         (modified — mc-model dep added)
- Cargo.toml (workspace)                           (modified — mc-model member entry)
- Cargo.lock                                       (modified — indexmap → 2.7.0, hashbrown → 0.15.5)

Dependencies added to mc-model:
- serde@1 (derive) — deserialization derives
- serde_yaml@0.9.34 — YAML 1.2 reader (last pre-deprecation release; Rust 1.78-compatible)
- thiserror — workspace dep, already pinned

Transitive-deps audit: 12 transitives, none requiring Rust 1.85+; no async/tokio/rayon.

Validator coverage (per ADR-0004 Decision 6):
- duplicate_names                          ✓ tested (4 tests, one per kind)
- missing_dimensions                       ✓ tested
- invalid_hierarchy_edges                  ✓ tested
- hierarchy_cycles                         ✓ tested
- rules_referencing_unknown_measures       ✓ tested
- derived_measures_without_rules           ✓ tested
- input_measures_with_rules                ✓ tested
- rule_cycles                              ✓ tested
- unsupported_aggregation_methods          ✓ tested
- golden_test_mismatches (structural)      ✓ tested (2 tests)
- golden_test_mismatches (value)           ✓ tested via golden_acme.rs

Acme YAML structural diff against build_acme_cube():
- dim count: equal (6)
- dim names: equal
- per-dim element counts: equal (3+3+17+8+15+11)
- hierarchy edge counts: equal (Time 16, Channel 7, Market 14)
- measure metadata: equal (role, dtype, aggregation per measure)
- weight-measure targets: equal (CPC→Spend, CVR→Clicks, etc.)
- rule count + rule body shape: equal (5 rules; structural string match)

Inline goldens in acme.yaml (9):
- 8 brief §4.5.1 anchor values at Mar_2026 / Paid_Search / Tampa
  (3 inputs exact; 5 derived within 1e-9)
- 1 consolidation rollup (Q1_2026 Spend at Paid_Search / Tampa = 33000.00 exact)

Implementation summary: serde_yaml 0.9.34 reads YAML; line-oriented
quote-aware prefilter rejects safe-subset violations before
deserialization; ParsedModel mirrors YAML 1:1; validate runs 9
Decision-6 validators in one pass with all-errors accumulation;
compile walks ValidatedModel and calls mc_core's existing builder API.
mc-cli's new --model flag reconstructs an AcmeRefs from ModelRefs so
the existing demo flow runs unchanged against either path.

Deviations:
- indexmap 2.14.0 → 2.7.0 transitive pin (Decision 3 escape hatch).
- ParsedRuleBody uses #[serde(untagged)] + tag-struct dispatch
  (serde_yaml 0.9 emits YAML tags by default, banned by Decision 1).
- Compile-stage internal-fallback errors use static strings
  (EngineError::Internal accepts only &'static str).
- mc-cli reconstructs AcmeRefs from ModelRefs rather than threading
  a generic Refs trait through (preserves byte-for-byte output gate).
- 10th Decision-6 row split across structural (validators.rs) and
  value (golden_acme.rs) test files per the ADR / handoff intent.
```

---

*Phase 3A shipped 2026-05-02 at `603c537` (tag `phase-3a-model-definition-layer`) after project owner review. The implementing Claude Code instance honored the handoff's "Final checklist" line item "**You did NOT commit, tag, or push.** The user does that after reading the review." — the user did the commit + tag step after PM/spec-maintainer signoff.*
