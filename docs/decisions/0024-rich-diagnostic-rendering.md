# ADR-0024 — Rich Diagnostic Rendering (Rust-Style Source Spans)

**Status:** Proposed  
**Date:** 2026-05-08  
**Author:** Edwin Lovett III  
**Depends on:** mc-model diagnostics (MC1xxx-MC3xxx), mc-narrative validation (MC7xxx)  
**Research note:** [`../research-notes/rich-diagnostic-rendering.md`](../research-notes/rich-diagnostic-rendering.md)

---

## Context

Mosaic emits ~80 diagnostic codes across 5 surfaces (model validation MC2xxx, lint MC3xxx, formula parsing MC1xxx, narrative templates MC7xxx, PPTX profiles MC706x). Today these diagnostics are flat structs with a logical path (`measures[3].rules[0].body`) but no source location. When a user or coding agent gets `error[MC2015]: measure referenced in rule body not found`, they must grep the file to find it.

Rust's compiler, TypeScript, and Elm all ship diagnostics with exact source context — file, line, column, underline pointer, help text. This is a proven UX pattern that makes errors self-locating:

```
error[MC2015]: measure referenced in rule body not found
  --> models/scotts-rv/model.yaml:87:22
   |
87 |     body: "Custmers * AOV"
   |            ^^^^^^^^ did you mean `Customers`?
   |
   = note: available measures: Customers, AOV, Revenue, Spend, Impressions
```

This is a shared diagnostic substrate — not a feature of any one crate. Every diagnostic surface benefits, and the visual template editor (Phase 7B) can eventually highlight the exact YAML field inline.

---

## Decisions

### Decision 1: Byte-offset `SourceSpan`, not line/column/length

```rust
/// A span in a source file, for diagnostic rendering.
/// Uses byte offsets; line/column computed lazily on render.
#[derive(Debug, Clone)]
pub struct SourceSpan {
    pub file: String,          // relative path
    pub start_byte: usize,    // byte offset, file-absolute
    pub end_byte: usize,      // byte offset, file-absolute
}

impl SourceSpan {
    /// Resolve a span within an inner string back to file coordinates.
    /// Used to lift formula-parser positions to YAML-file positions.
    pub fn with_inner_offset(&self, inner_start: usize, inner_len: usize) -> Self {
        Self {
            file: self.file.clone(),
            start_byte: self.start_byte + inner_start,
            end_byte: self.start_byte + inner_start + inner_len,
        }
    }
}
```

**Why byte offsets:** multi-line spans (block scalar formulas), offset composition (formula-within-YAML), and UTF-8 safety. Line/column is a rendering concern computed from bytes + source text at display time. The formula parser already tracks internal byte positions — `with_inner_offset` composes them with the YAML string's file position in one addition.

**Why not line/column/length:** a `SourceSpan { line, column, length }` can't represent multi-line spans. A formula error in a YAML block scalar:

```yaml
body: |
  Customers
    * AOV
    + Misspeled
```

spans lines 3-4. Byte offsets handle this naturally; line/column/length requires `end_line, end_column` and gets awkward.

---

### Decision 2: Unified `Diagnostic` type across all surfaces

```rust
pub struct Diagnostic {
    pub code: &'static str,         // "MC2015"
    pub severity: Severity,
    pub path: Option<ModelPath>,    // logical path (backward compat)
    pub message: String,
    pub primary_span: Option<SourceSpan>,
    pub related: Vec<RelatedSpan>,
    pub notes: Vec<String>,
    pub help: Vec<String>,
    pub suggestion: Option<Suggestion>,
}

pub struct RelatedSpan {
    pub span: SourceSpan,
    pub label: String,
}

pub struct Suggestion {
    pub message: String,
    pub kind: SuggestionKind,
}

pub enum SuggestionKind {
    /// Free-form help text.
    Help(String),
    /// Machine-applicable fix: replace span content with `with`.
    Replace { span: SourceSpan, with: String },
}
```

`RelatedSpan` handles multi-location diagnostics like MC7050 (priority collision between two templates in different files). `Suggestion::Replace` enables future `mc model fix` that auto-applies safe fixes — build the type now so every diagnostic site doesn't need revisiting later.

**Where this lives:** a new `crates/mc-diagnostics/` crate. All diagnostic surfaces (`mc-model`, `mc-narrative`, PPTX profiles) import the shared types rather than each inventing their own slightly different `Diagnostic`. The crate is tiny (types + renderer, ~300 lines, no runtime deps beyond `std`).

**Why a crate, not a module:** diagnostics cross crate boundaries. `mc-model` and `mc-narrative` both emit diagnostics; the CLI renders them; the JSON envelope serializes them. A shared crate is the natural home. It has zero runtime deps (types + renderer only).

---

### Decision 3: Pure renderer with `ColorMode`

```rust
pub enum ColorMode {
    Always,
    Never,
    Auto,   // detect TTY; strip colors when stdout isn't a terminal
}

/// Render a diagnostic to a string with Rust-style source context.
/// source_provider returns file content by path for multi-file diagnostics.
pub fn render_diagnostic(
    diag: &Diagnostic,
    source_provider: impl Fn(&str) -> Option<&str>,
    color: ColorMode,
) -> String
```

The renderer is pure — takes diagnostic + source text, returns a string. No I/O. The `source_provider` closure (not a single `&str`) lets one render call pull text from multiple files for cross-file related-span diagnostics.

**Color convention (matching Rust):**
- `error` → bold red
- `warning` → bold yellow
- `note`/`help` → bold cyan
- Code spans → bold white
- Line numbers → blue
- Underlines → severity color

**Tab handling:** the renderer normalizes tabs to spaces (4-space width, configurable) before drawing underlines. Otherwise `^^^^^` won't align with the source line when tabs are present.

**~250 lines estimated** for the full renderer including color, tab handling, multi-line spans, and related-span support. Hand-rolled, no external deps.

---

### Decision 4: YAML parser strategy

**Long-term architecture:** single-pass parsing through `yaml-rust2`/`saphyr` which exposes `Marker { line, col }` on every YAML node. Parse once, walk the tree for both typed data and source positions. One source of truth.

**v1 transitional path:** `mc-model` currently uses `serde_yaml` (0.9.34, pinned since Phase 3A). Full parser migration is not required for Phase 7A.6. Instead:

1. Parse YAML to `serde_yaml::Value` first (preserves some location info via the `Mapping` API)
2. Build a `LocationMap` side-table mapping logical paths → byte offsets
3. Deserialize `Value` to typed structs as before

**This transitional path is explicitly marked as temporary.** New diagnostic surfaces (PPTX profiles, Tessera recipes) should NOT use the side-table approach — they should parse with `saphyr` directly. The `mc-model` migration to `saphyr` is a Phase 7B or later housekeeping task when `serde_yaml`'s unmaintained status becomes a liability.

**Why not swap immediately:** `serde_yaml` is deeply integrated into `mc-model` (Phase 3A), `mc-narrative` (schema.rs), and PPTX profiles. Swapping all three in a diagnostic-UX phase creates toolchain risk. The transitional side-table is less elegant but scoped.

---

### Decision 5: "Did you mean?" is a separate concern

The rendered example shows `"Custmers" → did you mean "Customers"?`. That requires an edit-distance pass over available identifiers — a suggestion engine, not a rendering feature.

**The renderer displays suggestions. It does not calculate them.** The suggestion engine is orthogonal work (Phase 7A.6.5 or 7B scope):

```rust
// Suggestion engine (separate from renderer)
pub fn suggest_identifier(input: &str, candidates: &[&str]) -> Option<String> {
    // Levenshtein / edit-distance, threshold ≤ 2
}
```

Diagnostic sites that want "did you mean?" call the suggestion engine, populate `Suggestion::Help(...)` or `Suggestion::Replace { ... }`, and the renderer displays it. The two concerns are cleanly separated.

---

### Decision 6: JSON envelope stability contract

The JSON diagnostic envelope gains a `rendered` field alongside structured data:

```json
{
  "code": "MC2015",
  "severity": "error",
  "message": "measure referenced in rule body not found",
  "span": { "file": "model.yaml", "start_byte": 1847, "end_byte": 1855 },
  "rendered": "error[MC2015]: measure referenced...\n  --> model.yaml:87:22\n   |\n87 | ..."
}
```

**Stability contract:**
- **Structured fields** (`code`, `severity`, `message`, `span`, `related`, `suggestion`) are the stable API. Agents should consume these.
- **`rendered`** is human-facing, best-effort, and may change format between versions. Agents must NOT parse the rendered ASCII text as an API. It exists for human readability in logs and terminal output.

---

### Decision 7: Diagnostic codes remain in their current crates

MC2xxx codes stay in `mc-model`. MC7xxx codes stay in `mc-narrative`. MC706x codes stay in `mc-demo-server`. The `mc-diagnostics` crate provides the shared `Diagnostic` type and renderer — it does NOT own diagnostic codes or their emission logic.

**Why:** diagnostic codes are domain-specific. A narrative template validation error belongs in `mc-narrative`. Moving the codes to a shared crate would create a god-crate that knows about models, narratives, and PPTX profiles. Instead, each crate constructs `Diagnostic` structs using the shared types and passes them to the shared renderer.

---

### Decision 8: Golden renderer tests

The renderer must have golden output tests for:

1. Single-line underline
2. Multi-line underline (block scalar formula)
3. Related spans in two file locations (MC7050 cross-template collision)
4. Help and note rendering
5. `Suggestion::Replace` rendering
6. Tab alignment (tabs normalized to 4 spaces)
7. UTF-8 content before the span (byte-offset correctness)
8. `ColorMode::Never` produces zero ANSI escape sequences
9. Missing source file degrades gracefully (renders without context)
10. Empty/None span degrades gracefully (renders code + message only)

These prevent silent regressions in the rendering output.

---

## Scope — Phase 7A.6

### Phase 7A.6-D1: Diagnostic core + `mc model validate` + formula parser

Ship the `mc-diagnostics` crate with `SourceSpan`, `Diagnostic`, `RelatedSpan`, `Suggestion`, `ColorMode`, and the renderer. Wire into:

- `mc model validate` — MC2xxx validation errors point to the YAML field
- Formula parser — MC1xxx parse errors underline the exact token within a YAML formula string (proves offset composition via `with_inner_offset`)

**Why formula parser in D1:** formula errors are the highest-value target (character-level mistakes are most common) and they exercise the offset-composition API. Deferring formulas to D2 risks locking an API in D1 that doesn't compose.

### Phase 7A.6-D2: Lint + narrative template validation

Wire into `mc model lint` (MC3xxx) and `mc model narrate` template validation (MC7xxx). The MC7050 priority-collision diagnostic is the canonical multi-location test case.

### Phase 7A.6-D3: PPTX profiles + JSON envelope

Wire into PPTX profile validation (MC706x). Ship the `rendered` field in the JSON diagnostic envelope. Complete the agent UX: agents get structured fields for programmatic fixes AND rendered text for human-style understanding.

---

## Alternatives considered

### External crate (`ariadne`, `codespan-reporting`, `miette`) (rejected)

All three are excellent but pull significant transitive dependencies. The rendering logic (~250 lines) is simple enough (line extraction, underline drawing, ANSI color codes, tab normalization) to hand-roll. Consistent with the project's "hand-rolled over deps" convention (process-notes §5).

### Line/column/length spans (rejected)

Can't represent multi-line spans. Block scalar formulas, related spans across lines, and multi-line YAML values all need either byte offsets or `(start_line, start_col, end_line, end_col)`. Byte offsets are simpler and compose with inner-grammar positions via addition.

### `SourceSpan` as a type in each crate (rejected)

Every crate inventing its own diagnostic shape leads to 3+ slightly different `Diagnostic` types with different field names. A shared `mc-diagnostics` crate with one `Diagnostic` type eliminates the divergence.

---

## Success criteria

- [ ] `mc-diagnostics` crate compiles with zero deps beyond `std` (+ `serde` for JSON serialization)
- [ ] `SourceSpan::with_inner_offset` correctly composes formula-within-YAML positions
- [ ] `mc model validate` renders MC2xxx errors with exact YAML line/column and underline
- [ ] Formula parse error (MC1xxx) underlines the exact bad token within a YAML string
- [ ] Multi-line span renders correctly for block scalar formulas
- [ ] `ColorMode::Never` output contains zero ANSI sequences
- [ ] Tab-containing YAML renders with aligned underlines
- [ ] `rendered` field appears in JSON envelope diagnostics
- [ ] 10 golden renderer tests pass (per Decision 8 list)
- [ ] Locked surfaces (mc-core, mc-fixtures): zero diff
- [ ] `cargo test --workspace` passes

---

*Rich diagnostics are a UX multiplier across every Mosaic surface. For humans reading CLI output, for coding agents fixing validation errors, and for the future visual editor rendering errors inline. The pattern is proven (Rust, TypeScript, Elm), the implementation is small (~250 lines), and the shared `mc-diagnostics` crate ensures every surface benefits from the same rendering quality.*
