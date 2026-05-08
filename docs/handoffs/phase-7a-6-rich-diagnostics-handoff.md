# Phase 7A.6 Handoff — Rich Diagnostic Rendering

> **Audience:** the Claude Code instance that implements Phase 7A.6.
> **You inherit `main` at 1055 / 0 tests. You'll work on the branch
> `phase-7a-6/rich-diagnostics`.**
>
> **This phase gives every Mosaic diagnostic exact source context.**
> Instead of `MC2015 [Error] measures.CPC.rules[0].body: measure not found`,
> users and agents see Rust-style output with file, line, underline,
> and help text — self-locating errors that don't require grep.
>
> **The binding design is in
> [`docs/decisions/0024-rich-diagnostic-rendering.md`](../decisions/0024-rich-diagnostic-rendering.md).
> Read it in full before starting.**

---

## The one paragraph you must internalize

Today's `Diagnostic` struct in `mc-model/src/diagnostic.rs` has
`code`, `severity`, `path: ModelPath`, `message`, `suggestion`. The
`ModelPath` carries `file`, `yaml_pointer`, `model_path`, and an
optional `Span { line, column }` (used only by parse errors). There
are no byte offsets, no multi-line spans, no related spans, no
machine-applicable suggestions, and no Rust-style renderer. Phase
7A.6 adds a new `mc-diagnostics` crate with `SourceSpan` (byte
offsets), a unified `RichDiagnostic` type, `RelatedSpan`,
`Suggestion::Replace`, and a hand-rolled renderer (~250 lines)
that produces Rust-style output with colors, underlines, and help
text. Then it wires these into `mc model validate` (MC2xxx) and the
formula parser (MC1xxx) to prove the pattern — including the
critical offset-composition case where a formula parse error inside
a YAML string needs `with_inner_offset` to point at the right
character in the file.

---

## Existing code you need to know

### `crates/mc-model/src/diagnostic.rs`

The current diagnostic infrastructure:

```rust
pub struct Diagnostic {
    pub code: DiagnosticCode,        // &'static str, e.g., "MC2015"
    pub severity: Severity,          // Error | Warning | Info
    pub path: ModelPath,             // file + yaml_pointer + model_path + optional Span
    pub message: String,
    pub suggestion: Option<String>,
}

pub struct ModelPath {
    pub file: PathBuf,
    pub span: Option<Span>,          // Some for parse errors, None elsewhere
    pub yaml_pointer: String,        // RFC-6901, e.g., "/measures/0/aggregation"
    pub model_path: String,          // e.g., "measures.CPC.aggregation"
}

pub struct Span {
    pub line: usize,
    pub column: usize,
}
```

`diagnostics_to_text()` renders the flat format. `diagnostics_to_json()`
renders the JSON envelope. `sort_diagnostics()` applies deterministic
emission order. These are the existing public functions.

### `crates/mc-model/src/formula.rs`

The formula parser already tracks byte offsets:

```rust
pub struct FormulaError {
    pub code: &'static str,          // "MC1003", "MC1005", etc.
    pub message: String,
    pub offset: usize,               // byte offset within the formula string
}
```

This `offset` is the position within the formula string, NOT the
position within the YAML file. To render correctly, you need
`yaml_offset_of(formula_string_value) + formula_error.offset`.
This is the offset-composition case that `SourceSpan::with_inner_offset`
solves.

### `crates/mc-model/src/validate.rs`

Emits MC2xxx diagnostics. Currently constructs `Diagnostic` with
`ModelPath::new(file, yaml_pointer, model_path)` — no span. These
need to gain `SourceSpan` via the location-map side-table.

### `crates/mc-model/src/lib.rs`

The `load()` pipeline: YAML bytes → `serde_yaml::from_str::<ParsedModel>()` →
`validate()` → `resolve_inputs()` → `compile()`. The `serde_yaml` parse
step is where source positions are available but currently discarded.

---

## What gets built (3 sub-phases, ~4 sessions total)

### D1 Session 1 (~3-4h): `mc-diagnostics` crate + renderer

**Goal:** New crate with types + renderer. Compiles and passes golden
tests. No integration with existing crates yet.

**Deliverables:**

1. **New crate** `crates/mc-diagnostics/`:

   ```toml
   [package]
   name = "mc-diagnostics"
   version.workspace = true
   edition.workspace = true
   rust-version.workspace = true
   description = "Mosaic diagnostic types and Rust-style source-span renderer"

   [dependencies]
   serde = { version = "1", features = ["derive"] }
   ```

   Add to workspace `members` in root `Cargo.toml`.

2. **Core types** in `src/span.rs`:

   ```rust
   /// Byte-offset span in a source file. Line/column computed on render.
   #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
   pub struct SourceSpan {
       pub file: String,
       pub start_byte: usize,
       pub end_byte: usize,
   }

   impl SourceSpan {
       pub fn new(file: impl Into<String>, start: usize, end: usize) -> Self { ... }

       /// Compose with an inner offset (formula-within-YAML).
       pub fn with_inner_offset(&self, inner_start: usize, inner_len: usize) -> Self { ... }

       /// Resolve byte offset to (line, column) given source text.
       pub fn resolve_position(&self, source: &str) -> (usize, usize) { ... }
   }
   ```

3. **Diagnostic types** in `src/diagnostic.rs`:

   ```rust
   #[derive(Debug, Clone, Serialize)]
   pub struct RichDiagnostic {
       pub code: String,
       pub severity: DiagSeverity,
       pub message: String,
       pub primary_span: Option<SourceSpan>,
       pub related: Vec<RelatedSpan>,
       pub notes: Vec<String>,
       pub help: Vec<String>,
       pub suggestion: Option<Suggestion>,
   }

   #[derive(Debug, Clone, Serialize)]
   pub struct RelatedSpan {
       pub span: SourceSpan,
       pub label: String,
   }

   #[derive(Debug, Clone, Serialize)]
   pub struct Suggestion {
       pub message: String,
       pub kind: SuggestionKind,
   }

   #[derive(Debug, Clone, Serialize)]
   pub enum SuggestionKind {
       Help(String),
       Replace { span: SourceSpan, replacement: String },
   }

   #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
   pub enum DiagSeverity { Error, Warning, Info }
   ```

4. **Renderer** in `src/render.rs` (~250 lines):

   ```rust
   pub enum ColorMode { Always, Never, Auto }

   pub fn render_diagnostic(
       diag: &RichDiagnostic,
       source_provider: impl Fn(&str) -> Option<String>,
       color: ColorMode,
   ) -> String
   ```

   The renderer:
   - Extracts the relevant source lines from `source_provider(span.file)`
   - Computes line/column from byte offsets
   - Draws line-number gutter, source line, underline with `^^^^^`
   - Renders related spans (each with label)
   - Renders `= note:` and `= help:` lines
   - Renders `Suggestion::Replace` as `help: replace with: ...`
   - Normalizes tabs to 4 spaces before drawing underlines
   - Respects `ColorMode` (ANSI for Always/Auto-TTY, plain for Never)
   - Handles multi-line spans (underline start on first line, `...` for
     middle lines, end on last line)
   - Graceful degradation: missing source file renders code+message only

5. **10 golden renderer tests** (per ADR-0024 Decision 8):
   1. Single-line underline
   2. Multi-line underline
   3. Related spans in two locations
   4. Help and note rendering
   5. `Suggestion::Replace` rendering
   6. Tab alignment (tabs → 4 spaces)
   7. UTF-8 content before the span
   8. `ColorMode::Never` has zero ANSI escapes
   9. Missing source file degrades gracefully
   10. Empty/None span renders code+message only

---

### D1 Session 2 (~3-4h): Wire into `mc model validate` + formula parser

**Goal:** `mc model validate` renders MC2xxx errors with exact YAML
location. Formula parse errors (MC1xxx) underline the bad token within
the YAML string.

**Deliverables:**

1. **Location map for `mc-model`:**

   After `serde_yaml::from_str::<serde_yaml::Value>()`, walk the `Value`
   tree and build a `HashMap<String, SourceSpan>` keyed by yaml_pointer
   (e.g., `/measures/0/body` → `SourceSpan { start_byte, end_byte }`).

   This is the transitional side-table from ADR-0024 Decision 4. It's
   explicitly temporary — marked with `// TODO(saphyr): replace with
   single-pass LocatedValue parsing`.

   ```rust
   pub struct LocationMap {
       spans: HashMap<String, SourceSpan>,
       source_text: String,
       file_path: String,
   }

   impl LocationMap {
       pub fn build(file_path: &str, yaml_content: &str) -> Self { ... }
       pub fn get(&self, yaml_pointer: &str) -> Option<&SourceSpan> { ... }
   }
   ```

   The `build` function parses with `serde_yaml::Value`, walks the tree
   using `serde_yaml`'s `Mapping` keys to track positions. For each
   scalar/mapping/sequence node, records the yaml_pointer → byte offset
   range.

   **Note on `serde_yaml` position tracking:** `serde_yaml` 0.9 exposes
   `Location` via error types but not per-node positions on `Value`.
   If per-node positions aren't available through the `Value` API, fall
   back to a line-scanning approach: for each yaml_pointer, search the
   source text for the key string and record its byte offset. This is
   approximate but better than nothing for v1. Mark with
   `// TODO(saphyr): precise positions via single-pass parser`.

2. **Integrate `LocationMap` into validate pipeline:**

   In `mc-model`'s `load()` or `validate()`, build the location map
   from the YAML content. When constructing each `Diagnostic`, look up
   the `yaml_pointer` in the location map and attach the `SourceSpan`.

   Add a conversion: `Diagnostic` (existing type) → `RichDiagnostic`
   (new type from mc-diagnostics):

   ```rust
   impl Diagnostic {
       pub fn to_rich(&self, loc_map: Option<&LocationMap>) -> RichDiagnostic { ... }
   }
   ```

3. **Formula parser offset composition:**

   When the formula parser returns `FormulaError { offset, ... }`, the
   validate pipeline knows the YAML byte offset of the formula string
   (from the location map). Compose:

   ```rust
   let formula_span = yaml_span.with_inner_offset(
       formula_error.offset,
       formula_error.token_len.unwrap_or(1),
   );
   ```

   This produces a `SourceSpan` pointing at the exact bad token within
   the YAML file.

4. **CLI rendering:**

   Update `mc-cli`'s `mc model validate` output path to call
   `render_diagnostic()` when a `RichDiagnostic` has a primary span.
   Fall back to the existing `diagnostics_to_text()` for diagnostics
   without spans.

   Detect TTY: `ColorMode::Auto` checks `atty::is(atty::Stream::Stdout)`
   — or use a simpler heuristic: check `$NO_COLOR` env var and
   `$TERM` existence. No need for the `atty` crate; a 5-line helper
   suffices.

**Regression tests (5+ minimum):**
1. `test_validate_error_has_source_span`
2. `test_formula_error_points_at_bad_token`
3. `test_formula_offset_composition_within_yaml`
4. `test_rich_diagnostic_renders_with_underline`
5. `test_validate_output_matches_golden` (snapshot of full rendered output)

---

### D2 (~2-3h): Lint + narrative template validation

**Goal:** `mc model lint` (MC3xxx) and `mc model narrate` template
validation (MC7xxx) emit rich diagnostics.

**Deliverables:**

1. **Lint diagnostics** (`mc-model/src/lint.rs`):

   Same pattern as validate: look up yaml_pointer in LocationMap,
   attach SourceSpan to each lint Diagnostic, convert to RichDiagnostic.

2. **Narrative template diagnostics** (`mc-narrative/src/lib.rs`):

   `NarrativeError` types (MC7001-MC7055) gain optional `SourceSpan`.
   The template loader builds a LocationMap per YAML template file.

   The MC7050 priority-collision case is the canonical multi-location
   diagnostic: two related spans in two different templates (possibly
   in different files).

3. **Add `mc-diagnostics` as a dependency of `mc-narrative`.**

**Regression tests (3+ minimum):**
1. `test_lint_warning_has_source_span`
2. `test_narrative_mc7050_two_related_spans`
3. `test_narrative_error_renders_with_underline`

---

### D3 (~1-2h): JSON envelope + polish

**Goal:** The `rendered` field appears in JSON diagnostic output.
All acceptance gates green.

**Deliverables:**

1. **JSON envelope update:**

   `diagnostics_to_json()` gains an optional `rendered` field per
   diagnostic:

   ```json
   {
     "code": "MC2015",
     "severity": "Error",
     "message": "measure referenced in rule body not found",
     "span": { "file": "model.yaml", "start_byte": 1847, "end_byte": 1855 },
     "rendered": "error[MC2015]: measure ...\n  --> model.yaml:87:22\n   |\n87 | ..."
   }
   ```

   **Stability contract:** structured fields are the API. `rendered` is
   best-effort, non-stable, may change format between versions. Agents
   consume structured fields, not rendered text.

2. **PPTX profile diagnostics** (MC706x): add SourceSpan to PPTX
   profile validation errors.

3. **Polish:** ensure `cargo fmt`, `cargo clippy`, all gates green.

**Regression tests (2+ minimum):**
1. `test_json_envelope_includes_rendered_field`
2. `test_rendered_field_stability_contract_structured_fields_present`

---

## Hard Rules (binding)

1. **`mc-core`, `mc-fixtures` locked.** Zero diff.
2. **`mc-diagnostics` has zero runtime deps** beyond `serde` (for JSON serialization). No `ariadne`, no `codespan-reporting`, no `miette`.
3. **Existing `Diagnostic` type in `mc-model` is not deleted** — it gains a `.to_rich()` conversion. Backward compatibility with existing JSON envelope consumers.
4. **The renderer is pure.** Takes diagnostic + source provider → string. No I/O, no global state.
5. **Tab normalization:** renderer expands tabs to 4 spaces before drawing underlines.
6. **ColorMode::Auto:** strips ANSI when stdout isn't a TTY. Check `$NO_COLOR` env var and TTY detection (no external crate).
7. **Location map is explicitly temporary** — marked `// TODO(saphyr)` at every usage site. New diagnostic surfaces should not extend it.
8. **Per-session commits (Rule 11).** At least 3 commits.

---

## Acceptance Gates

- [ ] `cargo fmt --check --all` + `cargo clippy --all-targets --workspace -- -D warnings` + `cargo build --release --workspace` all exit 0
- [ ] `cargo test --workspace` passes (1055 → expect ~1070)
- [ ] `mc-diagnostics` crate compiles with zero deps beyond `serde`
- [ ] `SourceSpan::with_inner_offset` correctly composes formula-within-YAML positions
- [ ] `mc model validate` renders MC2xxx errors with file:line:column and underline
- [ ] Formula parse error (MC1xxx) underlines the exact bad token within a YAML string
- [ ] Multi-line span renders correctly
- [ ] `ColorMode::Never` output contains zero ANSI sequences
- [ ] Tab-containing YAML renders with aligned underlines
- [ ] 10 golden renderer tests pass
- [ ] `rendered` field appears in JSON envelope diagnostics
- [ ] Locked surfaces (mc-core, mc-fixtures): zero diff

---

## SPEC QUESTION candidates

- D1: `serde_yaml::Value` may not expose per-node byte positions
  cleanly. If the location map falls back to line-scanning, should
  the spans point at the YAML key or the YAML value? (PM default:
  point at the value — that's what the user needs to change. The key
  is context, not the error site.)

- D1: Should the renderer show the `yaml_pointer` alongside the file
  location? (PM default: no — the file:line:column is sufficient for
  navigation. The yaml_pointer stays in the structured JSON for
  programmatic consumers.)

- D2: Should `mc model lint --format json` include `rendered` in v1?
  (PM default: yes — all JSON envelope output includes `rendered`
  once D3 ships. D1-D2 can ship without it; D3 adds it.)

---

*End of handoff. Phase 7A.6 gives every Mosaic diagnostic the
Rust-compiler treatment: exact file, line, column, underline, help
text. For humans it's self-locating errors. For agents it's the exact
byte offset to fix. The ~250-line hand-rolled renderer ships in a new
`mc-diagnostics` crate that every surface can import.*
