# ADR-0006: Phase 3C — Model Test Fixtures and Input Sets

**Status:** Accepted (with project-owner amendments — see "Acceptance amendments" section below)
**Date:** 2026-05-03 (Proposed); 2026-05-03 (Accepted, same day)
**Deciders:** project owner
**Phase:** 3C precondition (per [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md))

> Phase 3B shipped at `f4f7fa8` (tag `phase-3b-lint-and-diagnostics`), giving the project four CLI verbs (`mc model {validate, inspect, lint, test}`) and a stable diagnostic surface. This ADR is the gating Phase 3C artifact and is now **Accepted**, unblocking Phase 3C. The Phase 3C handoff at [`../handoffs/phase-3c-handoff.md`](../handoffs/phase-3c-handoff.md) is the implementation contract; this ADR is the strategic context behind it.
>
> **Roadmap impact (per acceptance amendment #16; see "Roadmap impact" section below).** This ADR redefines Phase 3C from "friendly formula syntax" (originally named in ADR-0004 Decision 4) to "model test fixtures and input sets." Friendly formula syntax becomes Phase 3D. The swap is named explicitly here so future readers don't get confused; `MASTER_PHASE_PLAN.md` is updated as part of acceptance.

---

## Context

Phase 3B closed the model-quality story for cubes that the kernel already knows how to populate. But it left one visible scaffolding hack: `mc model test` works on Acme because `mc-cli/src/main.rs:253` has an `if model.parsed.metadata.name == "Acme_MarketingFinance"` branch that calls `mc_fixtures::write_canonical_inputs`. **Any other YAML model passes through `mc model test` with empty input cells, which means goldens that depend on input values will spuriously fail.**

The Phase 3B completion report flagged this as deviation 4.3, explicitly labeling it "no `inline_inputs` schema in Phase 3A — flagged as Phase 3C candidate." Phase 3C is the candidate.

Three things make this the right next phase:

- **`mc model test` is a visible CLI command.** Every YAML author who writes a model with goldens will hit the limitation. Today the workaround is "rename your model `Acme_MarketingFinance`" or "have your goldens not depend on inputs," neither of which is sustainable.
- **The fix unblocks Phase 4 (LLM authoring) and Phase 5 (data integration).** Phase 4 will emit YAML models from natural language; those models will need declared input data to be testable. Phase 5 will bulk-load real actuals; the schema-side mechanism for declaring inputs is the same shape (just a different data source).
- **The fix is small.** Phase 3C is a schema addition + a loader path + validators + Acme migration. No new parser, no new dep (CSV is the obvious sibling format and may be hand-rolled).

The four key questions this ADR answers — informed by GPT's draft + Claude Desktop's 6 supplemental refinements — are:

1. **What's the data shape?** (inline YAML vs sibling CSV vs both)
2. **What's the relationship between always-load data and per-test data?** (canonical_inputs vs test_fixtures distinction)
3. **What validators ship?** (the 14-validator surface)
4. **What stays out of scope?** (no actuals import, no DuckDB, no formula strings)

The 9 decisions below are listed in dependency order.

---

## Decisions needed

### Decision 1: inline YAML vs sibling files vs both

**Question:** Should test inputs live inline in the model YAML, in sibling files, or both?

**Decision (Accepted):** **Two forms — tabular inline YAML for small fixtures (≤ ~50 rows), sibling CSV files for everything else.** Per acceptance amendments (a) + (b) + #18 + #19, the verbose per-row inline YAML form is dropped (defer to a future phase only if a real implementation reason surfaces); the CSV grammar is pinned to a strict fixture-only subset; CSV path resolution is pinned to the YAML's directory tree; the columns contract is fully specified.

| Format | Best for | Pros | Cons |
|---|---|---|---|
| **Tabular inline YAML** | Small fixtures (≤ ~50 rows) | Compact; ~1/6th the tokens of per-row form; easy for LLMs to emit | Requires column declaration; column order matters |
| **Sibling CSV** | Large fixtures (Acme's 2,520; future production scale) | Compact; standard format; easy to author in spreadsheets; easy for data pipelines to emit | Separate file to track |

**Tabular inline form** (the daily driver for small inline fixtures):

```yaml
canonical_inputs:
  inline:
    columns: [Scenario, Version, Time, Channel, Market, Measure, value]
    rows:
      - [Forecast, Base, Mar_2026, Paid_Search, Tampa, Spend, 11500.0]
      - [Forecast, Base, Mar_2026, Paid_Search, Tampa, CPC,   1.50]
      # ... up to ~50 rows ...
```

**Sibling CSV form** (Acme + future production scale):

```yaml
canonical_inputs:
  source: "acme.inputs.csv"
  columns: [Scenario, Version, Time, Channel, Market, Measure, value]
```

A model file uses one form (or neither, if it has no canonical inputs). Both forms resolve to the same internal `Vec<(CellCoordinate, ScalarValue)>` after loading.

**CSV grammar — strict fixture-only subset (binding contract per acceptance amendment (b)):**

The Phase 3C CSV parser supports **only** the following subset. Any deviation is a parse error:

- **UTF-8 encoding** (no BOM tolerated; reject and report).
- **Required header row** as the first line; column names byte-exact match the YAML's declared `columns:` (same names, same order, no trailing whitespace). Header mismatch → MC2024.
- **Comma-separated** field delimiter. No tab, no semicolon, no other delimiter.
- **No quoted fields.** A quote character (`"`) anywhere in a field is a parse error. Field values must not contain commas, newlines, or quotes.
- **No embedded commas** in field values.
- **No embedded newlines** in field values; one logical row per physical line.
- **No comments** (no `#`-prefixed lines, no `--` lines).
- **Numeric value column** — the last column is the cell value, parsed as F64 (or matched against the row's measure-declared type per the columns contract below). NaN values → MC2021.
- **Trailing newline** on the last line is tolerated (`\n` after the last data row); not required.
- **No empty rows** — every non-header line must have a non-empty value in every column.

**This subset is hand-rollable in ~50 lines of Rust** (split by `\n`, split by `,`, parse). Phase 3C does NOT add the `csv` crate as a dependency; the implementer writes the splitter directly. Real CSV actuals import (with quoted fields, escaped commas, multi-line cells, encodings, etc.) is **Phase 5**, not Phase 3C.

**CSV columns contract (binding per acceptance amendment #19):**

The `columns:` declaration is implicit about three things; pin each explicitly:

(a) **The last column name in `columns:` is reserved as the cell value.** Phase 3C uses the literal string `value` as the convention, but the implementer may pick a different reserved name (`__value`, `_value`, etc.) if `value` conflicts with anything. All other columns are dimensions and must match dimension names declared in the model.

(b) **The CSV file's header row must match `columns:` byte-exact** — same names, same order, no trailing whitespace. Header mismatch → MC2024.

(c) **The value column's parse type is determined by the row's Measure column** resolving to a measure declaration (which carries the `data_type`). Type mismatch → MC2018. NaN value → MC2021.

**CSV path resolution (binding per acceptance amendment #18):**

`canonical_inputs: { source: "<path>" }` and `test_fixtures.<name>: { source: "<path>" }` paths are resolved **relative to the YAML model file's directory**. Paths that resolve outside the YAML file's directory tree (`../../escape.csv`, absolute paths, paths through symlinks pointing outside the directory) are rejected with a typed diagnostic — either MC2022 (source missing/unreadable) with a "path-escape" message variant, OR a dedicated MC2026 (Phase 3C handoff makes the call).

Rationale: principle of least surprise + prevents a class of accidental data-leak bugs when models are shared via copy-paste or version-control snapshot. If a real two-model project surfaces a need for a shared `fixtures/` directory at the workspace root, expand the allowed root to "the nearest ancestor directory containing a Cargo.toml or a project-marker file" — but the Phase 3C default is the strict per-model-directory rule until concrete need argues otherwise.

**Downstream:** Phase 3C ships parser support for tabular inline + sibling CSV. Acme uses the CSV form (its canonical_inputs are 2,520 rows). New model authors writing small fixtures use the tabular inline form. No third form ships in Phase 3C.

### Decision 2: test fixtures vs real actuals import

**Question:** What's the difference between Phase 3C test fixtures and Phase 5's actuals-import workflow?

**Decision (Accepted):** They are **deliberately separate concerns** — same data shape, different intent.

- **Phase 3C test fixtures** are *deterministic model QA data*. They live in version control alongside the model file. They exist to make `mc model test` reproducible. Their values are hand-authored or derived from the closed-form formulas the brief specifies (e.g., Acme's `write_canonical_inputs` produces values from a formula, not from a real-world feed).
- **Phase 5 actuals import** loads *real-world data* from external sources (CSVs from media platforms, API responses from ad networks). Actuals are large, mutable, and have provenance metadata. They live in databases or object stores, not version control.

Both paths produce `(CellCoordinate, ScalarValue)` tuples that get written to the cube via the same kernel API. The schema mechanisms differ:

- Phase 3C: `canonical_inputs:` and `test_fixtures:` blocks in the model YAML, pointing at inline data or sibling CSVs.
- Phase 5 (future): `actuals_sources:` block (or similar) pointing at external feeds with periodic-refresh semantics.

**Why keep them separate:** mixing test data with production actuals is the classic finance-team failure mode. If `model.yaml` could declare both *"these test values run on every CI build"* and *"these production actuals refresh nightly from BigQuery,"* one of them eventually leaks into the wrong context. Hard separation prevents the leak: test fixtures are version-controlled and immutable; actuals are external and mutable.

**Downstream:** Phase 5's handoff (whenever it happens) inherits this separation. The `actuals_sources:` schema is independent of `canonical_inputs:` / `test_fixtures:`.

### Decision 3: golden test → fixture references

**Question:** How do golden tests reference the input data they need?

**Decision (Accepted):** Each golden test optionally references a fixture by name. If omitted, only `canonical_inputs` load (per amendment #11's distinction below). Golden tests that don't depend on inputs (e.g., a static metadata check) can omit the reference entirely.

```yaml
golden_tests:
  # Uses canonical_inputs only (no fixture reference)
  - name: spend_at_tampa_paid_search_march
    coord: { Scenario: Baseline, Version: Working, Time: Mar_2026, ... }
    expect: 11500.0

  # Uses canonical_inputs + a named fixture that overrides specific cells
  - name: revenue_under_aggressive_lift
    fixture: aggressive_lift_2026q1
    coord: { Scenario: Aggressive, Version: Working, Time: Q1_2026, ... }
    expect_within_epsilon: { value: 1500000.0, epsilon: 1.0 }
```

**Resolution order at `mc model test` time (binding contract):**

1. Load `canonical_inputs` (always, if declared).
2. **Snapshot the cube** via `mc_core::Cube::snapshot()` immediately after canonical_inputs load. Save the snapshot handle for between-goldens reset.
3. For each golden test:
   a. If the golden test names a `fixture:`, load that fixture's data on top — fixture data overrides any cell that canonical_inputs also wrote (per amendment #11).
   b. Run the golden's `expect` / `expect_within_epsilon` check.
   c. **Rollback to the snapshot** via `mc_core::Cube::rollback_to(snapshot)` to reset cube state between goldens. Each golden test is independent.

**Performance rationale (per acceptance amendment #17):** the naive "clear store + reload canonical_inputs between every golden" pattern would cost ~2,520 writes × ~165 µs/write × 9 goldens ≈ 3.7 seconds per `mc model test acme.yaml` run. Using snapshot/rollback (PERF.md §6.9: snapshot ≈ 30 µs at Acme scale, rollback ≈ 75 µs each) brings the between-goldens cost to ~50 ms total. **This matters because Phase 4 (LLM authoring) uses `mc model test` as a tight feedback loop** — each LLM iteration runs goldens; a 3.7-second baseline becomes a 30-second iteration cycle in Phase 4, which kills the LLM-iteration story.

**Snapshot/rollback semantic check:** the implementer verifies that `Cube::snapshot()` + `Cube::rollback_to()` correctly restore dirty-set state, revision, and consolidation cache for the fixture-overlay case before locking in the snapshot/rollback approach. If a behavior gotcha surfaces (e.g., dirty-set restoration interacts oddly with fixture writes), fall back to "only reset between goldens that reference fixtures" — goldens using only canonical_inputs are read-only and need no reset (the cube state is unchanged across them). The Phase 3C handoff confirms which semantic ships.

**Validators that fire on missing references:** see Decision 6 — golden referencing an unknown fixture is its own error code.

**Downstream:** the resolution-order semantic is the contract Phase 4 (LLM authoring) and Phase 6 (UI editor) consume. An LLM authoring a "what-if" scenario emits a fixture; a UI's "compare versions" view loads two fixtures side-by-side.

### Decision 4: canonical_inputs vs test_fixtures (per amendment #11)

**Question:** Are "always-load this" data and "per-test setup" data the same concept or two concepts?

**Decision (Accepted): two concepts, separately named.**

- **`canonical_inputs:`** (top-level, optional, **at most one per model**). The standard input set always loaded before any golden test runs. Replaces the Acme `metadata.name == "Acme_MarketingFinance"` special case in `mc-cli/src/main.rs:253`. For models that don't need any baseline inputs, this is omitted entirely.
- **`test_fixtures:`** (top-level, optional, **named, multiple per model**). Per-test-named fixtures that augment or override `canonical_inputs` for specific golden tests. A golden test references a fixture by name via `fixture: <fixture_name>`. If the golden omits `fixture:`, only `canonical_inputs` apply.

```yaml
canonical_inputs:
  source: "acme.inputs.csv"

test_fixtures:
  - name: aggressive_lift_2026q1
    inline:
      columns: [Scenario, Version, Time, Channel, Market, Measure, value]
      rows:
        - [Aggressive, Working, Jan_2026, Paid_Search, Tampa, Spend, 15000.0]
        - [Aggressive, Working, Feb_2026, Paid_Search, Tampa, Spend, 16500.0]
        # ... overrides for Q1 only ...

  - name: conservative_drawdown_2026q4
    source: "fixtures/conservative_q4.csv"
    # ... another scenario ...
```

**Why separate:** GPT's original draft conflated "always-load this" with "per-test setup." Separating them keeps the simple case simple — Acme uses `canonical_inputs:` only, no test_fixtures — while leaving room for multi-scenario models that legitimately need both.

**Test contract (per amendment #11):** an integration test loads a model with no `test_fixtures` and confirms `canonical_inputs` apply; a second model with both confirms the override semantic works correctly.

**Alternate route flagged in amendment #11:** if separating turns out to add ceremony for no practical benefit (i.e., every real model uses one or the other, never both), collapse to a single `test_fixtures:` concept with a `default: true` flag on one fixture. The Phase 3C handoff makes the call after Acme + at least one other example model surface real usage patterns.

**Downstream:** the schema's two-concept shape is the contract Phase 4 / 5 / 6 inherit. Collapsing later is cheaper than splitting later, but evidence has to support the collapse.

### Decision 5: Acme migration

**Question:** Does Acme move from the CLI hard-coded canonical-inputs branch to a model-owned fixture?

**Decision (Accepted): Yes, definitively.** The `metadata.name == "Acme_MarketingFinance"` branch in `mc-cli/src/main.rs:253` is removed; Acme's canonical inputs move to `crates/mc-model/examples/acme.inputs.csv` (sibling file form per Decision 1). The Phase 3C completion report's diff against `phase-3b-lint-and-diagnostics` should show:

- **Removed:** the `if model.parsed.metadata.name == "Acme_MarketingFinance"` branch in mc-cli.
- **Added:** `acme.yaml`'s top-level `canonical_inputs: { source: "acme.inputs.csv", columns: [...] }` block.
- **Added:** `crates/mc-model/examples/acme.inputs.csv` with 2,520 rows (1 header + 2,520 data rows).

**The headline acceptance gate (per acceptance amendments #12 + (c)):** an equivalence test asserts that the Rust canonical-input path and the YAML+CSV path produce byte-identical store state across all 2,520 canonical input coordinates AND all 9 inline goldens. **Implementation MUST use only existing public APIs from `mc-core` + `mc-fixtures` — no new APIs added to either crate** (per acceptance amendment #15: `mc-fixtures` stays untouched; per the Phase 3A precedent: `mc-core` is locked).

Concretely, the test pattern is:

```rust
// Path A — existing Rust fixture path (unchanged from Phase 1A onward)
let (mut cube_a, refs_a) = mc_fixtures::build_acme_cube().expect("build acme");
mc_fixtures::write_canonical_inputs(&mut cube_a, &refs_a).expect("write inputs");

// Path B — new YAML + CSV path (Phase 3C deliverable)
let cube_b = mc_model::load("crates/mc-model/examples/acme.yaml")
    .expect("load yaml");
//          ^ load() resolves canonical_inputs:{source: "acme.inputs.csv", ...}
//            internally; cube_b returns already-populated.

// Equivalence check — compare via 2,520 per-coordinate public reads
for coord in mc_fixtures::canonical_input_coords(&refs_a) {
    let val_a = cube_a.read(&coord, refs_a.root_principal).expect("read a");
    let val_b = cube_b.read(&coord, /* root principal from cube_b */).expect("read b");
    assert_eq!(val_a.value.as_f64(), val_b.value.as_f64(),
        "coord mismatch at {coord:?}");
}

// Plus: every inline golden in acme.yaml passes byte-identical against the
// Rust path (existing Phase 3A goldens cover this; Phase 3C's gate just
// re-runs them through the new code path)
```

**The exact API names above (`build_acme_cube`, `write_canonical_inputs`, `Cube::read`, etc.) all already exist as of `phase-3a-model-definition-layer`.** The Phase 3C handoff confirms each public symbol is in place at handoff time; if any API the test pattern above relies on is missing or has a different signature, **stop and write a SPEC QUESTION** before adding anything to `mc-core` or `mc-fixtures`. Almost certainly the answer is "use a different existing API," not "add a new one."

**`canonical_input_coords` vs hand-rolled coord enumeration:** if `mc_fixtures` doesn't export a `canonical_input_coords()` helper, the implementer enumerates the 2,520 coords inline in the test (1 scenario × 1 version × 12 months × 5 channels × 7 markets × 6 input measures) — same shape as the existing `write_canonical_inputs` body, just iterating instead of writing. Adding a public helper to `mc-fixtures` is allowed for this case **only** because it's a pure read-side iterator over already-exported IDs, but the default is "enumerate inline" to keep the lock guarantee.

This is Phase 3C's analogue to Phase 3A's demo-equivalence diff.

**Why definitive:** the Acme-name special case was always a known-temporary scaffold (Phase 3B deviation 4.3). Leaving it in place after Phase 3C ships would mean two independent paths for canonical inputs (the Acme-only Rust path AND the generic YAML/CSV path), which is exactly the kind of duplication that produces silent divergence.

**`mc-fixtures` stays untouched (per amendment #15).** `build_acme_cube` and `write_canonical_inputs` are still the canonical Rust-side reference; Phase 1/2 benches and tests depend on them. Phase 3C adds the YAML-side equivalent in `mc-model`; the two paths produce identical state on Acme but live in different layers. The locked-surfaces guarantee (mc-fixtures untouched since Phase 2D) is preserved.

**Downstream:** Phase 3C's CLI flow becomes:

```
mc model test <path>:
  load(<path>) → ValidatedModel + Cube
  if model.canonical_inputs.is_some():
    apply canonical_inputs to Cube
  for each golden_test:
    if golden.fixture.is_some():
      apply test_fixtures[golden.fixture] to Cube (override semantic)
    run golden's expect check
    reset Cube (clear store, re-apply canonical_inputs for next iter)
```

Generic, no special cases.

### Decision 6: validators

**Question:** What validators ship for the new fixture/input schema?

**Decision (Accepted):** **14 validators (MC2012–MC2025)** with stable codes from MC2012 onward (continuing the validation namespace from Phase 3A's MC2001–MC2010 + Phase 3B's MC2011). Per acceptance amendments (d) + (e) + GPT #8 + #9: MC2012 narrowed to "unknown dimension KEY" (typo'd column name); MC2013 stays "unknown element VALUE" (separately diagnosed); MC2025 reassigned pre-acceptance to "duplicate input coordinate within the same input set" (NOT a duplicate of MC2019).

| Code | Severity | Validator |
|---|---|---|
| **MC2012** | error | **Unknown dimension KEY** in fixture columns or coord — a column header like `Scenrio` (typo) that doesn't match any dimension name declared in the model. (Amendment (d): tightened from "unknown dimension VALUE" — that's MC2013.) |
| **MC2013** | error | **Unknown element VALUE** — a cell value like `Mar_2026` in the Time column that isn't in the Time dimension's element list. Distinct from MC2012 because the *column* is correctly named but the *row's value* is wrong. |
| **MC2014** | error | Fixture references unknown measure — a row's Measure column has a value (e.g., `Spnd`) not in the measures list |
| **MC2015** | error | Fixture writes to derived measure — only inputs are writable; derived cells are computed by rules |
| **MC2016** | error | Duplicate fixture names within a model — two `test_fixtures` entries with the same `name:` |
| **MC2017** | error | Golden test references unknown fixture name — `golden_tests[i].fixture` doesn't match any declared `test_fixtures.name` |
| **MC2018** | error | Fixture value type mismatch — writing a string where the measure declares F64; or any other measure-declared-type vs row-value-type mismatch |
| **MC2019** | error | Fixture coordinate missing required dimension — a leaf write must specify all 6 dims; missing any column means the write is ambiguous. **(Amendment (e): MC2025 is NOT a duplicate of this; see below.)** |
| **MC2020** | error | Coordinate points to a consolidated cell — only leaves are writable; same kernel rule the Phase 1A `write` path enforces |
| **MC2021** | error | Fixture value is NaN — kernel rejects NaN writes anyway, but catching at load time gives a better error message with file:line:column |
| **MC2022** | error | Source CSV file not found or unreadable. Includes "path-escape" rejection per acceptance amendment #18 (paths resolving outside the YAML's directory tree); the message variant disambiguates the two cases (file-not-found vs path-escape). The Phase 3C handoff may split path-escape into a dedicated MC2026 if cleaner |
| **MC2023** | error | CSV row column count does not match declared `columns:` length — too few or too many fields on a row |
| **MC2024** | error | CSV header row mismatch with declared `columns:` — header line doesn't byte-exact match the YAML's `columns:` declaration |
| **MC2025** | error | **Duplicate input coordinate within the same input set** — two rows in `canonical_inputs` (or within a single `test_fixtures` entry) writing to the exact same coordinate. **(Amendment (e): repurposed pre-acceptance from "missing required dimension" — that's MC2019. Catches silent last-write-wins on duplicate CSV rows, which is genuinely valuable.)** |

**On amendment (e)'s code-cleanup (per acceptance amendment #9):** the original ADR draft had MC2019 and MC2025 as conceptually-overlapping codes (both about "you didn't specify all dims"). Per amendment (e), MC2019 is the canonical "missing required dimension" check, and MC2025 is **repurposed pre-acceptance** to "duplicate coordinate within input set" — a distinct, valuable check. Per amendment #9, **proposed unshipped codes can be revised before acceptance**; this is the only such revision in Phase 3C's code-namespace. **After Phase 3C ships, MC2025's meaning ("duplicate input coordinate") is locked forever.**

**No code retirement in Phase 3C.** The Phase 3B MC3008-retired pattern doesn't repeat here — every code MC2012–MC2025 ships with its declared meaning. Future phases follow the same CVE-style rule: codes can be retired but not repurposed once shipped.

**Alternate route still relevant:** if any of MC2020 / MC2021 / MC2022 turn out to be already covered by existing Phase 3A validators on the cube side (e.g., NaN rejection might already fire at write-time, making the load-time check redundant), drop the redundant ones. Don't double-validate. The Phase 3C handoff confirms which are net-new vs duplicates of existing kernel checks; if any code drops, it stays vacant in the registry (CVE-style).

**Each validator gets a negative-test fixture** under `crates/mc-model/tests/fixture_validation_fixtures/` — one minimal model per validator that triggers exactly that code. Same pattern as Phase 3B's `lint_fixtures/`.

**Downstream:** the validator surface grows from Phase 3B's 11 (MC2001–MC2011) to Phase 3C's 25 (MC2001–MC2025, with MC3008 still permanently retired). The diagnostic envelope shape from ADR-0005 Decision 7 is unchanged; only the code count grows. Per acceptance amendment #20, **adding new codes is backwards-compatible** — `schema_version` stays at `"1.0"` (see Decision 9 gate #12).

### Decision 7: CLI behavior

**Question:** What changes in `mc model test`'s CLI surface?

**Decision (Accepted):** Two CLI changes — one mandatory, one new flag. Per acceptance amendments (f) + GPT #5: ship `--fixture <name>` with the **filter-only semantic** (run only goldens that explicitly reference the named fixture). Defer `--inputs <fixture_path>` to Phase 5 (its real use case is loading actuals).

| Change | Purpose | Required? |
|---|---|---|
| **`mc model test <path>` resolves fixture references from the model itself** (no Acme special case) | Closes Phase 3B deviation 4.3 | **Yes** — this is the headline |
| **`mc model test <path> --fixture <name>`** | **Run only goldens that explicitly reference the named fixture.** All other goldens (those without `fixture:` or with a different fixture name) are skipped. Reports the count of skipped goldens in the output | **Yes** — ships in Phase 3C |
| ~~`mc model test <path> --inputs <fixture_path>`~~ | ~~Override the model's `canonical_inputs:` with a sibling file specified at the CLI~~ | **Deferred to Phase 5** (real use case is loading actuals) |

**`--fixture` semantic — pinned to one reading (per Desktop wording note):** the `--fixture <name>` flag is a **filter** — it selects which goldens to run, not how to run them. Concretely: `mc model test acme.yaml --fixture aggressive_q1` runs only the goldens whose `fixture:` field equals `"aggressive_q1"`; all other goldens are reported as "skipped (filtered)". Each selected golden then loads its declared fixture (via Decision 3's resolution order) and runs.

**The "overlay all goldens with this fixture regardless of their declared fixture" behavior is a different feature** that Phase 5 (or a later phase) can name explicitly if needed — likely as a separate flag (`--overlay <name>` or similar). Bundling the two semantics into one `--fixture` flag would conflate filtering with overlay, which produces ambiguous CLI behavior.

**Other CLI surfaces unchanged:** `mc model validate` / `inspect` / `lint` / `demo` / `demo --model` — all behave identically to Phase 3B. Only `mc model test`'s internal logic changes (resolution order from Decision 3 + the new `--fixture` filter).

**Downstream:** the CLI signature stability from Phase 3B is preserved; users learning four verbs in Phase 3B don't need to relearn them. Phase 3C just adds one optional flag to one command.

### Decision 8: out of scope

**Question:** What is *not* Phase 3C?

**Decision (Accepted):** the following are out of scope. Each is named here so the implementer can't rationalize "while we're at it":

| Out of scope | Phase | Notes |
|---|---|---|
| **CSV actuals import (real-world data feeds)** | Phase 5 | Per Decision 2 — separate concern |
| **DuckDB / external storage** | Phase 5+ | `HashMapStore` remains the only store |
| **API / network data loading** | Phase 5+ | Same |
| **Formula strings** (`Revenue = Customers * AOV`) | Phase 3D *(per Roadmap impact section below)* | Per ADR-0004 Decision 4, now sequenced after Phase 3C per amendment #16 |
| **LLM authoring** | Phase 4 | Phase 3C's fixtures are designed *for* Phase 4 to consume, but Phase 3C doesn't ship LLM scaffolding |
| **UI editor** | Phase 6 | Same — fixtures are designed for Phase 6's editor to render, but Phase 3C is CLI-only |
| **`mc-core` changes** | Future, if ever needed | Phase 3C is read-only over `mc-model` + `mc-cli`; the kernel is locked |
| **Multi-cube models / cross-cube references** | Future Phase 3 sub-phase | Per ADR-0004 Decision 5 |
| **Bidirectional round-trip** (Cube → YAML, store → CSV) | Future | Phase 3A is one-way; Phase 3C is also one-way (CSV → store); reverse direction is its own scope |
| **Auto-fix for fixture validators** (`mc model fix-fixtures`) | Future | Same pattern as Phase 3B deferred `mc model fix` |

**Hard rule (per amendment #15):** no source change in `crates/mc-core/`. No source change in `crates/mc-fixtures/src/`. No new dep in either. The Phase 2D / Phase 3A / Phase 3B locks stay locked.

**Downstream:** the Phase 3C handoff opens with this list as a visible "do not touch."

### Decision 9: success gate

**Question:** What does Phase 3C "complete" mean?

**Decision (Accepted):** Phase 3C is complete when **all** of the following hold:

1. **Acme canonical inputs move to model-owned data.** `crates/mc-model/examples/acme.inputs.csv` exists with 2,520 rows (1 header + 2,520 data); `crates/mc-model/examples/acme.yaml` declares `canonical_inputs: { source: "acme.inputs.csv", columns: [Scenario, Version, Time, Channel, Market, Measure, value] }`.
2. **The Acme-name special case is REMOVED from `mc-cli/src/main.rs`.** `git diff phase-3b-lint-and-diagnostics -- crates/mc-cli/src/main.rs` shows the `if model.parsed.metadata.name == "Acme_MarketingFinance"` branch deleted. **Acceptance gate: `grep -c "Acme_MarketingFinance" crates/mc-cli/src/main.rs` returns 0.**
3. **Headline equivalence test passes (per acceptance amendments #12 + (c)):** the test pattern from Decision 5 — uses **only existing public APIs** from `mc-core` + `mc-fixtures` (no new APIs added to either crate); compares Rust path (`build_acme_cube` + `write_canonical_inputs`) against YAML+CSV path (`mc_model::load`) across all 2,520 canonical input coordinates plus all 9 inline goldens. Byte-identical results.
4. **`mc model test crates/mc-model/examples/acme.yaml` exits 0** with all 9 inline goldens passing — without any Acme-specific CLI logic.
5. **`mc model test crates/mc-model/examples/acme.yaml --fixture <name>`** runs only the goldens that explicitly reference that fixture; reports skipped goldens.
6. **`mc demo --model crates/mc-model/examples/acme.yaml`** still produces brief §4.6 output byte-for-byte identical to `mc demo` (the Phase 3A demo-equivalence diff stays empty).
7. **Phase 3B CLI behavior preserved.** `mc model {validate, inspect, lint}` work identically to phase-3b-lint-and-diagnostics on the updated Acme YAML. The added `canonical_inputs:` block doesn't introduce any new lints on Acme; if it does, those lints either fire correctly (and are documented) or get suppressed (with rationale in the completion report).
8. **`mc model lint crates/mc-model/examples/acme.yaml` still exits 0 with zero warnings** (Phase 3B headline gate carry-forward — the new `canonical_inputs:` block must not trigger any lints on Acme).
9. **All 14 fixture-validation rules implemented (MC2012–MC2025)**, each with a negative-test fixture under `crates/mc-model/tests/fixture_validation_fixtures/` triggering exactly that rule. Per acceptance amendment #19's CSV columns contract, three of those fixtures specifically exercise: header byte-mismatch (MC2024); type mismatch (MC2018); NaN value (MC2021).
10. **CSV path-escape rejection (per acceptance amendment #18):** a fixture with `source: "../escape.csv"` produces a typed diagnostic (MC2022 with path-escape variant, OR a dedicated MC2026 — implementer's call).
11. **Snapshot/rollback used for between-goldens reset (per acceptance amendment #17):** the implementer verifies `Cube::snapshot()` + `Cube::rollback_to()` correctly restore state for the fixture-overlay case. **Performance gate:** `mc model test crates/mc-model/examples/acme.yaml` completes in **< 500 ms wall-clock** on the Phase 1B reference machine (this is the conservative gate; the < 200 ms target from amendment #17 is the stretch goal — the implementer measures and reports actual; if the gate is comfortably hit, tighten to < 200 ms in the completion report). If snapshot/rollback has a behavior gotcha for fixture-overlay, fall back to "only reset between goldens that reference fixtures" per amendment #17's alternate route.
12. **JSON envelope schema_version unchanged at `"1.0"` (per acceptance amendment #20):** Phase 3C adds new MC2xxx codes but doesn't change the `Diagnostic` struct shape. **Stability rule (binding):**
    - Adding diagnostic codes is backwards-compatible. **No `schema_version` bump.**
    - Removing or repurposing a code requires a `schema_version` bump (CVE-style retirement discipline from Phase 3B amendment #11).
    - Adding a NEW FIELD to the `Diagnostic` struct (e.g., a `fixture_name: Option<String>`) requires a `schema_version` bump to `"1.1"`.
    Phase 3C ships with `schema_version: "1.0"` unchanged. **Acceptance assertion:** Phase 3B's snapshot fixtures (`crates/mc-model/tests/expected/lint_*.json`) re-run at Phase 3C completion produce zero diffs.
13. **All 293 existing tests still pass.** New total ≥ 293 + (Phase 3C test count). 10/10 deterministic.
14. **`mc-core` untouched.** `git diff phase-3b-lint-and-diagnostics -- crates/mc-core/` returns zero lines.
15. **`mc-fixtures` untouched** (per acceptance amendment #15). `git diff phase-3b-lint-and-diagnostics -- crates/mc-fixtures/src/ crates/mc-fixtures/Cargo.toml` returns zero lines.
16. **Toolchain stays at Rust 1.78.** No `cargo update`. No new dep that requires `edition2024`. CSV parsing is **hand-rolled** (per acceptance amendment (b)'s strict subset — Acme's CSV is pure ASCII / numeric, doesn't need quoted fields or escaped commas, doesn't justify pulling in the `csv` crate).
17. **`MASTER_PHASE_PLAN.md` updated** to reflect the 3C/3D swap (per acceptance amendment #16 + GPT #1; see "Roadmap impact" section). MASTER_PHASE_PLAN.md only — no ADR-0004 amendment unless a direct contradiction surfaces.

Phase 3C does NOT need to flip Phase 3B's tag, change PERF.md, or modify any spec doc. The kernel is locked; the model schema gains additive `canonical_inputs:` and `test_fixtures:` blocks (backwards-compatible — models without them load identically).

---

## Roadmap impact (per acceptance amendment #16)

ADR-0004 Decision 4 originally named **Phase 3C** as the friendly-formula-syntax phase: *"Friendly formula strings (`Revenue = Customers * AOV`) are deferred to **Phase 3C**."* This ADR redefines Phase 3C to mean "Model Test Fixtures and Input Sets" instead.

**The swap (named explicitly here so future readers don't get confused):**

- **New Phase 3C:** Model Test Fixtures and Input Sets (this ADR).
- **New Phase 3D:** Friendly Formula Syntax (originally ADR-0004's Phase 3C).

**Why the swap:** Phase 3C as defined here closes a visible scaffolding hack (`mc-cli/main.rs:253`'s Acme-name special case) that affects every YAML author writing a model. Friendly formula syntax is a quality-of-life add on top of the structured-tree representation that already works. Closing the visible hack is higher leverage than ergonomic improvements to a working surface.

**`MASTER_PHASE_PLAN.md` updated as part of acceptance** of this ADR. The Phase 3C row in the status overview points at this ADR (ADR-0006); the Phase 3D row is added with status `not started` and a brief description ("Friendly formula syntax — `Revenue = Customers * AOV` strings compile down to ParsedRuleBody's structured tree per ADR-0004 Decision 4").

**ADR-0004 amendment status:** ADR-0004 Decision 4 is technically still accurate ("formula strings deferred to a later sub-phase") — it doesn't pin the *number* of that sub-phase. So no ADR-0004 amendment is needed *unless* the project owner prefers to formalize the renumbering with a `0004-amendment-1.md`. The Phase 3C handoff makes this call; the default is "no amendment, since ADR-0004's wording survives."

**Alternate route flagged in amendment #16:** if the project owner prefers to preserve ADR-0004's original Phase 3C label, the substantive content of this ADR ships under a different number ("Phase 3C-prime" or "Phase 3B.5"). The decisions don't change; only the heading does. Default is the swap (3C → fixtures, 3D → formulas) per Claude Desktop's recommendation.

---

## Out of scope (explicit recap)

The Decision 8 table above is the binding list. Highlights:

- **No CSV actuals import.** Phase 5.
- **No DuckDB / external storage.** Phase 5+.
- **No API / network data loading.** Phase 5+.
- **No formula strings.** Phase 3D (renamed from Phase 3C per Roadmap impact).
- **No LLM authoring.** Phase 4.
- **No UI.** Phase 6.
- **No `mc-core` changes.** Locked.
- **No `mc-fixtures` changes.** Locked (per amendment #15).
- **No multi-cube / cross-cube references.** Per ADR-0004 Decision 5.

---

## Accepted decisions — TL;DR

Phase 3C ships against:

1. **Two forms — tabular inline YAML + sibling CSV** (Decision 1; per-row inline dropped per acceptance amendment (a)). CSV pinned to a strict fixture-only subset (UTF-8, required header, comma-separated, no quoted fields / embedded commas / embedded newlines / comments) per amendment (b). CSV path resolution relative to YAML directory, `../` escapes rejected per amendment #18. Columns contract pinned per amendment #19.
2. **Test fixtures (Phase 3C) ≠ actuals import (Phase 5)** — separate concerns, separate schema mechanisms (Decision 2).
3. **Golden tests reference fixtures by name; omitted reference means `canonical_inputs` only.** Snapshot/rollback used for between-goldens reset (perf gate `mc model test acme.yaml < 500 ms`) per amendment #17 (Decision 3).
4. **Two distinct top-level concepts: `canonical_inputs` (always-load, at-most-one) and `test_fixtures` (named, multiple)** per amendment #11 (Decision 4).
5. **Acme migrates to `acme.inputs.csv`; CLI Acme-name special case REMOVED.** Equivalence test uses ONLY existing public APIs from mc-core + mc-fixtures (no new APIs added) per amendments #12 + (c) (Decision 5).
6. **14 fixture validators** (MC2012–MC2025) with stable codes + per-rule negative fixtures. MC2012 narrowed to "unknown dimension KEY"; MC2013 stays "unknown element VALUE"; MC2025 reassigned pre-acceptance to "duplicate input coordinate within the same input set" per amendments (d) + (e) + #9 (Decision 6).
7. **`mc model test` resolves fixture references generically + optional `--fixture <name>` filter (filter-only semantic per Desktop wording note).** `--inputs` deferred to Phase 5 per amendments (f) + GPT #5 (Decision 7).
8. **No actuals, no DuckDB, no formulas, no LLM, no UI, no `mc-core`/`mc-fixtures` changes** (Decision 8).
9. **17-item success gate** including byte-identical equivalence between Rust and YAML+CSV canonical-input paths; `mc model lint acme.yaml` still exits 0 (Phase 3B carry-forward); CSV path-escape rejection; perf < 500 ms; schema_version stability rule pinned per amendment #20 (Decision 9).

**Roadmap impact:** Phase 3C → Test Fixtures (this ADR); Phase 3D → Friendly Formulas (originally ADR-0004's Phase 3C). `MASTER_PHASE_PLAN.md` updated at acceptance per amendment #16 + GPT #1. **No ADR-0004 amendment needed** unless a direct contradiction surfaces.

---

## Acceptance amendments

This ADR was Proposed and Accepted on 2026-05-03 with project-owner amendments on top of the proposed defaults. Two reviews contributed: GPT (9 owner decisions, with #10 being the meta-instruction to flip + draft handoff) and Claude Desktop (1 wording-tightening note + 4 supplemental amendments numbered #17–20). All 13 substantive amendments are recorded here for audit trail; the decisions above already reflect the final shape.

| # | Source | Amendment (one-line) | Where it landed in the ADR |
|---|---|---|---|
| 1 / GPT | GPT | Accept the 3C/3D roadmap swap. MASTER_PHASE_PLAN.md only — no ADR-0004 amendment unless a direct contradiction surfaces. | Roadmap impact section + Decision 9 gate #17 |
| 2 / GPT | GPT | Support sibling CSV + inline tabular YAML; defer verbose per-row inline YAML unless implementation reason argues otherwise. | Decision 1 (per-row inline form dropped) |
| 3 / GPT | GPT | Keep `canonical_inputs` and `test_fixtures` as separate concepts; reset cube state between golden tests; fixture data overrides canonical_inputs for matching coordinates. | Decision 3 (resolution order) + Decision 4 (separate concepts) |
| 4 / GPT | GPT | Define Phase 3C CSV as a strict fixture-only subset: UTF-8, required header row, comma-separated, no quoted fields, no embedded commas, no embedded newlines, no comments. Hand-rolled, no `csv` crate. Real CSV actuals import is Phase 5. | Decision 1 (CSV grammar — strict fixture-only subset) |
| 5 / GPT | GPT | Ship `mc model test --fixture <name>` (filter semantic — run only goldens that explicitly reference that fixture). Defer `--inputs <fixture_path>` to Phase 5. | Decision 7 (CLI table updated; `--fixture` semantic pinned) |
| 6 / GPT | GPT | Acme migration is mandatory. Remove the `metadata.name == "Acme_MarketingFinance"` branch from mc-cli; add `acme.inputs.csv`; add `canonical_inputs` reference in `acme.yaml`; `mc model test acme.yaml` must pass without Acme-specific CLI logic. | Decision 5 (definitive) + Decision 9 gate #2 (grep assertion) |
| 7 / GPT | GPT | Revise the equivalence gate so it does NOT require new mc-core or mc-fixtures APIs. Use existing public builders/loaders/reads. Compare across all 2,520 canonical input coordinates and existing goldens. mc-core and mc-fixtures remain locked. | Decision 5 (rewritten test pattern using existing public APIs only) + Decision 9 gate #3 |
| 8 / GPT | GPT | Clean up validator codes: MC2019 = missing required dimensions; MC2025 NOT a duplicate of MC2019 — repurpose for "duplicate input coordinate within same input set"; unknown dimension KEY (MC2012) and unknown element VALUE (MC2013) are separate diagnostics. | Decision 6 (table rewritten — MC2012 narrowed to KEY; MC2025 repurposed pre-acceptance) |
| 9 / GPT | GPT | Retired diagnostic codes stay retired forever, but proposed unshipped codes can still be revised before acceptance. | Decision 6 (MC2025 reassignment is pre-acceptance per this rule; lock-after-ship discipline noted) |
| (g) Wording / Desktop | Desktop | Tighten `--fixture` semantic to "filter-only" reading (run only goldens that explicitly reference that fixture). The "overlay all goldens with fixture" behavior is a different feature for Phase 5+ to name explicitly. | Decision 7 (`--fixture` semantic pinned to filter-only) |
| 17 / Desktop | Desktop | Use `Cube::snapshot()` + `Cube::rollback_to()` for the between-goldens reset instead of full canonical-inputs reload (perf: ~50 ms vs 3.7 s). Phase 4 LLM iteration loop depends on this. | Decision 3 (resolution order updated) + Decision 9 gate #11 (perf gate < 500 ms; stretch < 200 ms) |
| 18 / Desktop | Desktop | Spec CSV file path resolution explicitly: relative to YAML file's directory; reject paths that escape (no `../../`). | Decision 1 (CSV path resolution paragraph) + Decision 6 (MC2022 covers path-escape) + Decision 9 gate #10 (path-escape test) |
| 19 / Desktop | Desktop | Pin the CSV columns binding contract: last column reserved as cell value; header byte-exact match `columns:`; value parse type derived from row's Measure declaration; type mismatch = MC2018; NaN = MC2021. | Decision 1 (CSV columns contract paragraph) + Decision 9 gate #9 (three specific negative fixtures) |
| 20 / Desktop | Desktop | Clarify schema_version 1.0 stability contract: adding codes is backwards-compatible (no bump); removing/repurposing requires bump; adding new Diagnostic field requires bump to 1.1. Phase 3C ships at 1.0 unchanged. | Decision 9 gate #12 (binding stability rule + assertion that Phase 3B's lint snapshots produce zero diffs at Phase 3C completion) |

No remaining open questions. Phase 3C handoff at [`../handoffs/phase-3c-handoff.md`](../handoffs/phase-3c-handoff.md) is the implementation contract.

---

## Alternatives considered (whole-ADR scope)

1. **Skip Phase 3C; jump straight to Phase 3D (friendly formulas) or Phase 4 (LLM).** Rejected — the Acme-name special case in `mc-cli/main.rs:253` is a user-visible bug for any non-Acme model with input-dependent goldens. Closing it is a precondition for any later phase that exercises `mc model test` on non-Acme models (which is all of Phase 4, all of Phase 5, all of Phase 6).
2. **Single `test_fixtures:` block, no `canonical_inputs:` distinction.** Rejected per amendment #11. Conflating "always-load this" with "per-test setup" makes the simple case (Acme: just load these inputs) require ceremony (declare a default fixture, mark it as default). Two concepts, used independently or together as needed.
3. **Inline-only (no sibling CSV).** Rejected per Decision 1 + amendment #12. Acme's 2,520 rows would dominate the YAML; sibling CSV is the obvious right answer at that scale.
4. **Sibling-only (no inline forms).** Rejected per Decision 1. Small fixtures (5-row what-if scenarios) author cleanly inline; forcing them into sibling files adds friction with no benefit.
5. **Bundle Phase 3C + Phase 3D (formulas) + Phase 4 (LLM).** Rejected — three separate concerns with three separate scopes. Bundling fails the same way "we'll do it all at once" projects always fail (Phase 3D doesn't get exercised before Phase 4 layers on top).
6. **Push fixture data into the kernel** (`mc-core` gains a `default_inputs:` field). Rejected — `mc-core` is locked. Test fixtures are an authoring/testing concern, not a kernel concern. Same separation Phase 3A drew between `mc-model` (parser) and `mc-core` (engine).
7. **Make the Acme migration optional in Phase 3C.** Rejected — leaving the Acme-name special case in place after Phase 3C ships would mean two independent paths for canonical inputs (the Acme-only Rust path AND the generic YAML/CSV path), which is exactly the kind of duplication that produces silent divergence between paths.

---

## Cross-links

- [`../roadmap/MASTER_PHASE_PLAN.md`](../roadmap/MASTER_PHASE_PLAN.md) — Phase 3C and Phase 3D rows; updated at this ADR's acceptance per Roadmap impact section.
- [`../CURRENT_STATE.md`](../CURRENT_STATE.md) — Phase status; will be updated to add Phase 3C once this ADR is Accepted.
- [`0004-phase-3a-model-definition-format.md`](0004-phase-3a-model-definition-format.md) — Phase 3A ADR; Decision 4 originally named Phase 3C as friendly formulas (now Phase 3D per this ADR's Roadmap impact section).
- [`0005-phase-3b-model-qa-linter-diagnostics.md`](0005-phase-3b-model-qa-linter-diagnostics.md) — Phase 3B ADR; introduces the `Diagnostic` shape + JSON envelope that Phase 3C's new validators emit through.
- [`../reports/phase-3b-completion-report.md`](../reports/phase-3b-completion-report.md) §4.3 — flags the `mc-cli/main.rs:253` Acme-name special case as the Phase 3C scope trigger.
- [`../specs/phase-1-rust-kernel-build-brief.md`](../specs/phase-1-rust-kernel-build-brief.md) §1 — out-of-scope list; Phase 3C touches none of the items there.
- [`../../CLAUDE.md`](../../CLAUDE.md) §1 — allowed runtime deps; Phase 3C inherits the ban (no new `mc-core` deps).
- [`../../crates/mc-cli/src/main.rs`](../../crates/mc-cli/src/main.rs) line 253 — the visible hack this phase removes.

---

## Notes

This ADR is the strategic gate for Phase 3C the way ADR-0003 was for Phase 2, ADR-0004 was for Phase 3A, and ADR-0005 was for Phase 3B. It scopes the next sub-phase once so the Phase 3C handoff can be a build contract rather than a debate.

If this ADR is amended after Acceptance, the amendment lands as `0006-amendment-N.md` (append-only, mirroring the ADR-0003 / 0004 / 0005 pattern).

**The MC code namespace continues to grow**, with retired codes staying retired forever (per ADR-0005 amendment #11's CVE-style discipline). Phase 3C adds MC2012–MC2025 (or fewer if duplicates drop). Future phases inherit a registry doc in `mc-model/src/diagnostic.rs` that lists every active and retired code.

**The Phase 3C handoff is NOT drafted yet** (per Open Question 9). Drafting in parallel risks the handoff getting wedded to assumptions this ADR may revise during review. If/when this ADR is Accepted with amendments, the handoff is drafted at that moment.
