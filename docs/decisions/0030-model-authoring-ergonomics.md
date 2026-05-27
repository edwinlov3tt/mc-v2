# ADR-0030: Model Authoring Ergonomics — Auto-Element Population + JSON Schema

**Status:** Proposed
**Date:** 2026-05-26
**Deciders:** project owner
**Phase:** 3K (model authoring ergonomics)
**Crate:** `mc-model` only

---

## Context

A real-world cube authoring session (MLB totals model from claw-core's 2,353-game parquet) hit 10 schema validation errors in sequence. Most were 30-second fixes once the author found the right pattern in an existing example, but cumulatively they made the first authoring pass take much longer than the underlying complexity warranted.

Two of those friction points are worth fixing because they affect every new cube author, not just this one session:

1. **Dimension elements must be declared explicitly even when they're trivially derivable from `canonical_inputs`.** The MLB cube required 2,353 generated `- { name: "..." }` lines just to declare the Game dimension, even though every distinct game value was already present in the inputs CSV.

2. **No editor-side schema validation.** The author hit 8 of the 10 errors at `mc model validate` time (CLI), not at edit time. Field names like `target_measure`, `declared_dependencies`, and `method` (vs `type`) aren't discoverable from the YAML itself — the author had to grep existing examples after each error.

The other 8 errors from the session are either deliberate design choices (rules need `target_measure` separately from `name` because the rule and its output are distinct concepts) or generic YAML quirks (nested-quote handling). Those don't get changed here.

This ADR ships the two improvements that eliminate ~80% of the friction without changing any binding semantics.

---

## Decisions

### Decision 1: Auto-populate dimension elements from `canonical_inputs`

When a `Standard` dimension declares `elements: []` (or omits the field entirely) AND a `canonical_inputs` declaration is present AND the input CSV contains a column matching the dimension name, the loader populates `elements` automatically from the distinct values in that column.

Order of resolution (binding):
1. Parse YAML → `ParsedModel` (elements may be empty)
2. Resolve `canonical_inputs` → `ResolvedInputSet`
3. **(NEW) Auto-populate empty dimensions from resolved inputs**
4. Validate (existing flow — now sees populated elements)
5. Compile

**Rules:**
- Only fires when `elements` is empty or absent. If the author declares ANY elements, the loader does not auto-populate — explicit wins.
- Only applies to `Standard` and `Time` dimensions. `Scenario`, `Version`, and `Measure` dimensions are NOT auto-populated (they have semantic meaning beyond the data).
- Element ordering: distinct values in CSV order (first-seen wins for ties). NOT sorted alphabetically — preserves input file ordering for downstream consistency.
- If no matching column exists in the CSV, validation fails with the existing MC1001 error (missing elements) — same behavior as today.
- The auto-populated elements are visible in `mc model inspect` output, flagged with a `(from canonical_inputs)` annotation so authors know which dimensions were derived.

**New diagnostic codes:**
- MC1015 (info, not error): "Dimension '{name}' populated automatically from canonical_inputs ({N} elements)"

### Decision 2: Generate JSON schema for editor autocomplete

Add `#[derive(JsonSchema)]` to the public model types in `mc-model/src/schema.rs` via the `schemars` crate. Add a binary `mc-model-schema` that writes the schema to `docs/specs/mosaic-model-schema.json`.

Authors add this directive at the top of their YAML to enable editor validation:

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/edwinlov3tt/mc-v2/main/docs/specs/mosaic-model-schema.json
model_format_version: 1
metadata:
  name: my-model
```

VSCode's YAML extension, JetBrains' built-in YAML support, and Helix all honor this directive. Authors get:
- Field name autocomplete (typing `target_` → `target_measure`)
- Inline error highlighting (red squiggle on unknown fields)
- Hover documentation (from `#[doc]` comments on struct fields)
- Required-field warnings before save

**Schema regeneration:**
- The schema is checked in (not generated at build time) so external consumers can pin a known schema
- `mc-model-schema` binary regenerates it; run manually after schema changes
- CI gate: `cargo run --bin mc-model-schema --quiet | diff - docs/specs/mosaic-model-schema.json` fails if the schema drifts from the structs

**Dependency:** `schemars = "0.8"` (pure Rust, no system deps, MSRV 1.65 — compatible with our 1.78 pin)

### Decision 3: Document the auto-population behavior in YAML comments

Update the `/mosaic-author` skill's templates to omit `elements: []` for Standard dimensions when `canonical_inputs` is present. Add a one-line comment:

```yaml
- name: Game
  kind: Standard
  # elements auto-populated from canonical_inputs CSV
```

This makes the auto-population visible to authors looking at templates without reading the schema docs.

---

## Implementation plan

### Step 1: Auto-element population (mc-model/src/inputs.rs)

After `resolve_input_set` returns the resolved CSV data, walk each dimension in the parsed model:
- Skip if `elements.len() > 0` (explicit wins)
- Skip if kind is `Scenario`, `Version`, or `Measure`
- Look up the dimension name in the resolved column headers
- If found, derive distinct values in first-seen order
- Inject as `Element { name: ..., parent: None }` entries
- Emit MC1015 info diagnostic

### Step 2: JSON schema generation

- Add `schemars = "0.8"` to `mc-model/Cargo.toml`
- Add `#[derive(JsonSchema)]` to `ParsedModel`, `ParsedDimension`, `ParsedMeasure`, `ParsedRule`, `ParsedFittedModel`, and all transitively-referenced types in `schema.rs`
- Add doc comments (`#[doc = "..."]` or `///`) to fields that don't already have them — these become hover tooltips
- Create `crates/mc-model/src/bin/mc-model-schema.rs`:
  ```rust
  use schemars::schema_for;
  use mc_model::schema::ParsedModel;
  fn main() {
      let schema = schema_for!(ParsedModel);
      println!("{}", serde_json::to_string_pretty(&schema).unwrap());
  }
  ```
- Run once, commit `docs/specs/mosaic-model-schema.json`
- Add to project CI: `cargo run --bin mc-model-schema --quiet | diff - docs/specs/mosaic-model-schema.json`

### Step 3: Update templates and docs

- `/mosaic-author` plugin skill: omit `elements: []` for Standard dimensions when canonical_inputs is present
- `docs/specs/mosaic-model-schema.json`: regenerated
- `crates/mc-model/examples/acme.yaml`: add the `# yaml-language-server: $schema=...` directive as a reference example

---

## Acceptance criteria

1. A model with empty `Game` dimension elements + `canonical_inputs` containing a `Game` column validates and compiles successfully
2. The same model with one explicit element (`elements: [{name: "X"}]`) is NOT auto-populated — explicit wins
3. Auto-population skipped for Scenario/Version/Measure dimensions even when the column exists in CSV
4. MC1015 diagnostic fires with element count when auto-population happens
5. `cargo run --bin mc-model-schema` writes valid JSON Schema (draft-07) to stdout
6. The generated schema includes all `ParsedModel` fields with descriptions sourced from doc comments
7. VSCode YAML extension applies the schema when the `$schema=` directive is present (manual verification, not automated)
8. The MLB cube from this session validates with `elements:` removed from the Game dimension (acceptance test: rewrite mlb-totals.yaml to use the new behavior)
9. All existing tests pass unchanged — auto-population is additive
10. `cargo test --workspace` passes
11. `cargo clippy --all-targets --workspace -- -D warnings` passes
12. No changes to `mc-core`

---

## Alternatives considered

### Alt 1: Auto-populate from CSV unconditionally

Even when the author declares some elements, merge in any additional values found in the CSV.

**Rejected:** Surprising behavior. An author who declares `elements: [{name: "A"}, {name: "B"}]` and has C, D, E in the CSV would not expect C/D/E to silently appear. Explicit wins is the safer default.

### Alt 2: Generate the JSON schema at build time

Run `mc-model-schema` automatically as a `build.rs` step on every cargo build.

**Rejected:** Slows builds for a file that changes rarely. Manual regeneration with a CI drift check is simpler.

### Alt 3: Smarter error messages instead of auto-population

Improve the "missing elements" error to suggest `elements:` could be omitted if a matching CSV column exists.

**Rejected:** Better errors are nice, but the author still has to do the work of writing `elements: []` or generating the list. Auto-population eliminates the work entirely.

### Alt 4: Add a `from_csv_column` directive on dimensions

Explicit opt-in marker: `elements: { from_csv_column: "Game" }`.

**Rejected:** Adds vocabulary for a behavior that should just work. The auto-population only fires when elements is empty AND a matching column exists — the trigger is unambiguous.

---

## Out of scope

- Auto-populating hierarchies from data (would require domain-specific rollup rules)
- Auto-generating measure declarations from CSV columns (measures have semantic role/aggregation that can't be derived)
- Schema validation at `mc model validate` time using the JSON schema (validation is already implemented in Rust; JSON schema is for editors only)
- LSP integration (covered by the yaml-language-server, which most editors already use)
- Auto-detecting which CSV column matches which dimension when names don't match (column must match dimension name exactly)

---

## Cross-links

- **MLB session that motivated this:** chat reference 2026-05-26 (mlb-totals.yaml authoring)
- **Phase 3 series:** ADRs 0004 (3A), 0005 (3B), 0006 (3C — fixtures + canonical_inputs), 0011-0018 (formula language)
- **Phase 1 brief:** schema definitions (`docs/specs/phase-1-rust-kernel-build-brief.md`)
- **mc-model loader:** `crates/mc-model/src/lib.rs`
- **mc-model inputs:** `crates/mc-model/src/inputs.rs`
- **mc-model schema:** `crates/mc-model/src/schema.rs`
- **Mosaic plugin authoring skill:** `mosaic-plugin/skills/...`

---

## Notes

This is a small, additive phase — no architectural changes, no kernel changes, no breaking changes to existing models. Both improvements were validated against a real authoring session (MLB cube from claw-core data) where they would have eliminated ~80% of the friction.

The JSON schema is the higher-leverage of the two: it catches errors at edit time across every future cube authored by any user. Auto-element-population is narrower (only helps when canonical_inputs is present) but eliminates the most tedious authoring task (generating thousands of element lines from data).

Effort estimate: 1 session for auto-population, 1 session for schema generation + doc comment cleanup. Combined: 1-2 sessions total.
