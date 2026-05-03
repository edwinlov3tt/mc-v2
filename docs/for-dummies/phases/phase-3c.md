# Phase 3C — For Dummies

> **In one line:** Phase 3B's `mc model test` only worked on the Acme demo because of an embarrassing hack hardcoded into the CLI. Phase 3C deleted the hack and made model files own their own test data — every model is now self-contained.

> **Shipped 2026-05-03** at commit `8d2691a`, tag `phase-3c-fixtures-and-inputs`. See [completion report](../../reports/phase-3c-completion-report.md) for the full audit.

[Technical version → handoff](../../handoffs/phase-3c-handoff.md) · [ADR-0006](../../decisions/0006-phase-3c-model-test-fixtures.md) · [completion report](../../reports/phase-3c-completion-report.md)

---

## The analogy: a recipe with the ingredients packet stapled to it

You're following a recipe. It says "this should make 24 cookies and they should weigh 18 grams each." Great target. But to actually bake them, you need the ingredients — and the recipe doesn't list them. You have to look them up in a separate ingredients book.

That's roughly where Phase 3B left things. The model file (`acme.yaml`) said *"Spend at Tampa-Paid_Search-March should be $11,500"* (a golden test). But the input data — the actual numbers that flow into the cube — lived in Rust code (`mc_fixtures::write_canonical_inputs`), not in the model file. The CLI had a hidden hack:

```rust
// crates/mc-cli/src/main.rs:253 — the embarrassing line
if model.parsed.metadata.name == "Acme_MarketingFinance" {
    // Special case: load Acme's hardcoded inputs from Rust
    write_canonical_inputs(&mut cube, &refs)?;
}
```

If you wrote a YAML model and called it anything *other* than "Acme_MarketingFinance," `mc model test` would silently load no inputs and your goldens would all fail. The model wasn't actually self-contained; it depended on the CLI knowing your model by name.

Phase 3C is the recipe that now comes with the ingredients packet stapled to it. The model file declares what inputs it needs; the CLI loads them generically; no special cases.

## What Phase 3C actually shipped

Five concrete pieces of work:

**(1) Two new YAML schema sections.** Models can now declare:

```yaml
# Always loaded before any test runs (replaces the Acme hack)
canonical_inputs:
  source: "acme.inputs.csv"
  columns: [Scenario, Version, Time, Channel, Market, Measure, value]

# Per-test scenario data (load on top of canonical_inputs for specific goldens)
test_fixtures:
  - name: aggressive_q1
    source: "fixtures/aggressive_q1.csv"
  - name: conservative_drawdown
    inline:                        # small fixtures can stay inline
      columns: [Scenario, Version, Time, Channel, Market, Measure, value]
      rows:
        - [Conservative, Working, Q1_2026, Paid_Search, Tampa, Spend, 8000.0]
```

`canonical_inputs` is "always load this." `test_fixtures` is "for these specific goldens, load this on top." Most models use one, both, or neither.

**(2) A hand-rolled CSV parser.** Acme's input data is 2,520 rows — way too many to author inline in YAML (would dominate the model file). CSV is the natural fit. Phase 3C ships a strict CSV parser supporting:

- UTF-8, required header row, comma-separated
- No quoted fields, no embedded commas, no embedded newlines, no comments
- Numeric value column (last column)

That's about 80 lines of Rust. **No `csv` crate dep** — pulling in a full-featured CSV library would have added thousands of lines of transitive code for parser features Phase 3C doesn't need (real CSV import with quoted fields and weird encodings is Phase 5's job, not Phase 3C's).

The path resolution rule: `source: "acme.inputs.csv"` is relative to the YAML file's directory. Paths that try to escape (`source: "../escape.csv"`) are rejected.

**(3) 14 new validators (MC2012 through MC2025).** Each catches a specific class of fixture-authoring mistake:

| Code | Catches |
|---|---|
| MC2012 | Typo in a column name (`Scenrio` instead of `Scenario`) |
| MC2013 | Element value not in the dim (`Mar2026` instead of `Mar_2026`) |
| MC2014 | Reference to an unknown measure |
| MC2015 | Trying to write a derived measure (those are computed, not assignable) |
| MC2016 | Duplicate fixture names |
| MC2017 | Golden test references a fixture that doesn't exist |
| MC2018 | Value type mismatch (writing a string where F64 is declared) |
| MC2019 | Coordinate missing a required dimension |
| MC2020 | Writing to a consolidated cell (only leaves are writable) |
| MC2021 | NaN value (caught at load time, before reaching the kernel) |
| MC2022 | Source CSV file not found (or path-escape attempt) |
| MC2023 | CSV row column count doesn't match `columns:` |
| MC2024 | CSV header row doesn't byte-match the declared `columns:` |
| MC2025 | Same coordinate written twice in the same input set |

Each of these has a one-fixture test that triggers exactly that rule.

**(4) A new CLI flag: `mc model test --fixture <name>`.** Run only goldens that explicitly reference that fixture. Useful for "I just changed the aggressive_q1 fixture; rerun only the goldens that depend on it."

**(5) The Acme migration.** All 2,520 of Acme's canonical inputs moved out of Rust code (`write_canonical_inputs`) and into a sibling CSV file (`crates/mc-model/examples/acme.inputs.csv`). The hardcoded `if metadata.name == "Acme_MarketingFinance"` branch in `mc-cli/main.rs:253` got deleted.

The headline acceptance gate was brutal:

```bash
grep -c "Acme_MarketingFinance" crates/mc-cli/src/main.rs    # must return 0
```

Plus an equivalence test that loads the YAML+CSV path AND the original Rust path and asserts they produce **byte-identical** cube state across all 2,520 input coordinates. (They do.)

## A note on speed

`mc model test acme.yaml` runs in about **30 milliseconds**. That matters because Phase 4 (LLM authoring, when it happens) will use `mc model test` as a tight feedback loop — every LLM iteration runs goldens. A 3-second baseline would have made the LLM iteration cycle painfully slow.

The trick: between goldens, instead of reloading all 2,520 canonical inputs, the kernel takes a snapshot once and rolls back to it between goldens. Snapshot is ~30 µs; rollback is ~75 µs; total overhead is ~50 ms across all 9 goldens. (Without the snapshot trick, naive reload between goldens would be 9 × 2,520 writes × ~165 µs = ~3.7 seconds.)

## Why we care / what would have gone wrong without it

Three things would have stayed broken:

**(1) Every YAML model except Acme would have been broken.** The Acme-name special case was the load-bearing hack that made Phase 3B's `mc model test` look like it worked. Anyone authoring a different model (a future customer, an LLM in Phase 4, an analyst in Phase 6) would write a YAML with goldens, run `mc model test`, watch all goldens fail (because no inputs loaded), and conclude the system was broken. Phase 3C closed that.

**(2) Phase 4 (LLM authoring) had no story for input data.** When Phase 4 lands, an LLM will generate a YAML model from a natural-language prompt. The LLM also needs to generate the input data the goldens depend on. Without Phase 3C's `canonical_inputs:` schema, the LLM had nowhere to put that data — it would have had to either embed the data in Rust code (which the LLM can't do) or rely on the user to author it separately (which defeats the LLM's value). Now the LLM emits one YAML + one CSV; the system loads them.

**(3) Phase 5 (real-world data import) had no foundation.** Phase 5 will load actual planning data from external sources (CSVs from media platforms, API responses from ad networks). Phase 3C's `canonical_inputs:` schema is the precursor — it establishes the pattern of "data lives with the model, not hardcoded into the engine." Phase 5 generalizes the same pattern to external data sources.

## One thing that's easy to get wrong

The biggest temptation when adding a CSV parser is to reach for the `csv` crate. It's a perfectly fine library; people use it all the time. Phase 3C deliberately did **not** pull it in.

Why? Because the project's "minimum dep churn" pattern says: don't add a dependency unless you can prove the hand-rolled equivalent would be substantially worse. Acme's CSV is pure ASCII, pure numeric, no quoted fields, no escaped commas, no multi-line cells. The hand-rolled parser is ~80 lines of straightforward Rust. The `csv` crate would have added ~30,000 lines of transitive code for features Phase 3C doesn't need. Real CSV import with all the messy real-world quirks is Phase 5's job, when actuals from real platforms come in.

The other thing easy to misread is **what `canonical_inputs` is vs `test_fixtures`**:

- `canonical_inputs` is "always load this baseline before any test runs." A model has at-most-one. If your goldens all assume the same starting state, this is what you want.
- `test_fixtures` is "for THIS specific golden, load this scenario data on top of canonical_inputs." A model can have many named fixtures. Useful for "what if Q1 spend is 15% higher under the Aggressive scenario?"

The two were deliberately kept separate. Conflating them ("just one fixtures concept with a default flag") would have made the simple case (Acme: just baseline data) require ceremony.

## What Phase 3C is and isn't

| It is | It isn't |
|---|---|
| Model-owned test data (CSV sibling files + inline YAML) | Real-world actuals import (Phase 5) |
| The end of the Acme-name special case in `mc-cli` | A change to the kernel or fixtures (both untouched) |
| A strict CSV parser (~80 lines, hand-rolled) | A full-featured CSV library (no `csv` crate) |
| 14 new validators (MC2012–MC2025) | New lint rules (zero new lints; this is pure validation) |
| `mc model test --fixture <name>` filter flag | An overlay-on-everything semantic (deferred to Phase 5) |
| Snapshot/rollback between goldens for ~30ms test runs | A perf-oriented phase (no benches; just enough perf for the LLM loop) |
| The schema foundation for Phase 4 (LLM) and Phase 5 (actuals) | Either of those phases — both deferred |

## How long it took

About a day of focused implementation work. The biggest pieces:

- New `canonical_inputs:` and `test_fixtures:` schema (~300 lines added to schema.rs)
- New `csv.rs` module (~80 lines body + 9 unit tests)
- New `inputs.rs` module (resolve_inputs + apply_canonical_inputs + path-escape rejection)
- 14 new validators in validate.rs (~30 lines each)
- 35 new tests across 5 new test files + 14 negative fixtures + 2 sibling CSVs
- The Acme migration: 2,520-row `acme.inputs.csv` generated via a one-shot Rust binary that called `write_canonical_inputs` and dumped to CSV (then deleted)
- Removing the embarrassing `mc-cli/main.rs:253` special case

Test count: 293/0 → **328/0** (+35 tests). Headline gate cleared by removing the special case while keeping every existing test green AND adding the equivalence test that proves the new generic path produces byte-identical results to the old Rust path.

---

*Tied to: [phase-3a.md](./phase-3a.md) (the Phase that introduced YAML models), [phase-3b.md](./phase-3b.md) (the Phase that added `mc model test` — and the embarrassing scaffolding hack this Phase removed), [`../research-notes/totals-vs-formulas.md`](../research-notes/totals-vs-formulas.md) (the conceptual difference between input data and computed totals).*
