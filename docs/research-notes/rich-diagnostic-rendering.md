# Research Note: Rich Diagnostic Rendering (Rust-Style Source Spans)

> **Status:** Research (pre-ADR)  
> **Date:** 2026-05-08  
> **Relates to:** mc-model diagnostics (MC1xxx-MC3xxx), mc-narrative validation (MC7xxx), PPTX profile validation (MC706x)  
> **Inspiration:** Rust compiler diagnostic output, `ariadne`, `codespan-reporting`, `miette`

---

## 1. The Vision

Every Mosaic diagnostic that references a YAML source file should render
with exact source context — the line, the span, and a pointer to the
problem — so that humans and coding agents can locate and fix issues
without grep.

```
error[MC7050]: priority collision in explanation group
  --> demo/narratives/explanation-templates.yaml:42:3
   |
42 |   finding_id: impressions_declined_significant
   |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
43 |   explanation_priority: 200
   |   ^^^^^^^^^^^^^^^^^^^^^^^^^ this template has priority 200...
   |
  --> demo/narratives/explanation-templates.yaml:67:3
   |
67 |   finding_id: impressions_declined_significant
68 |   explanation_priority: 200
   |   ^^^^^^^^^^^^^^^^^^^^^^^^^ ...but so does this one
   |
   = help: assign distinct explanation_priority values within the same finding_id group
```

```
warning[MC3002]: measure has no description
  --> models/scotts-rv/model.yaml:31:5
   |
31 |   - name: CPC
   |     ^^^^^^^^^ add a `description:` field for this measure
   |
   = help: descriptions improve LLM authoring, lint output, and template readability
```

```
error[MC2015]: measure referenced in rule body not found
  --> models/scotts-rv/model.yaml:87:22
   |
87 |     body: "Custmers * AOV"
   |            ^^^^^^^^ did you mean `Customers`?
   |
   = note: available measures: Customers, AOV, Revenue, Spend, Impressions
```

This applies to every diagnostic surface:
- `mc model validate` / `mc model lint` / `mc model test` (MC1xxx-MC3xxx)
- `mc model narrate` template validation (MC7xxx)
- PPTX profile validation (MC706x)
- Tessera recipe validation (future)

---

## 2. Current State

Diagnostics today are flat structs:

```rust
// mc-model
pub struct Diagnostic {
    pub code: &'static str,     // "MC2015"
    pub severity: Severity,
    pub path: ModelPath,        // "measures[3].rules[0].body"
    pub message: String,
    pub suggestion: Option<String>,
}

// mc-narrative
pub enum NarrativeError {
    DuplicateTemplateId { template_id: String },
    NotabilityOutOfRange { template_id: String, value: f64 },
    UnresolvedPlaceholder { template_id: String, placeholder: String },
    // ...
}
```

`ModelPath` is a logical path (`measures[3].rules[0].body`), not a
source location. There's no file path, line number, column, or span.
The YAML parser (`serde_yaml`) discards source positions after parsing.

---

## 3. What Needs to Change

### 3.1 Source spans at parse time

`serde_yaml` (0.9) doesn't preserve source locations. Two options:

**Option A: Use `serde_yaml`'s `Mapping` + `Value` types with `Location`.**
`serde_yaml::Value` carries location info via the `Mapping` API in
newer versions. Parse to `Value` first, extract locations, then
deserialize to typed structs. Two-pass: one for locations, one for
typed data.

**Option B: Use `yaml-rust2` directly.** The `yaml-rust2` crate
(successor to `yaml-rust`) exposes `Marker { line, col }` on every
YAML node. Parse once, walk the tree for both typed data and source
positions. More control, more code.

**Recommendation:** Option A for v1 — less disruption to existing
`serde_yaml` usage. The location map is built alongside deserialization
and stored as a side-table.

### 3.2 SourceSpan type

```rust
/// A span in a source file, for diagnostic rendering.
#[derive(Debug, Clone)]
pub struct SourceSpan {
    pub file: String,        // relative path
    pub line: usize,         // 1-based
    pub column: usize,       // 1-based
    pub length: usize,       // span length in chars
}
```

Added to diagnostic types:

```rust
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub path: ModelPath,
    pub message: String,
    pub suggestion: Option<String>,
    pub span: Option<SourceSpan>,        // NEW
    pub related: Vec<RelatedSpan>,       // NEW — for multi-location diagnostics
}

pub struct RelatedSpan {
    pub span: SourceSpan,
    pub label: String,
}
```

Diagnostics without spans (e.g., semantic checks that don't map to a
single YAML node) render without the source context — graceful
degradation.

### 3.3 Renderer

A `render_diagnostic(diag: &Diagnostic, source: &str) -> String`
function that produces the Rust-style output. The source text is read
once and passed in; the renderer uses `span.line` and `span.column`
to extract the context lines and draw the underline.

```rust
pub fn render_diagnostic(diag: &Diagnostic, source: &str) -> String {
    // Header: error[MC2015]: message
    // File pointer: --> file:line:col
    // Context lines with line numbers
    // Underline with ^^^^ span
    // Help/note lines
}
```

The renderer is pure — takes a diagnostic + source text, returns a
string. No I/O. Can be used by CLI (colored terminal), JSON envelope
(embedded in the diagnostic JSON), and future web UI (rendered to HTML).

### 3.4 Color support

Terminal output uses ANSI colors matching Rust's convention:
- `error` in bold red
- `warning` in bold yellow
- `note`/`help` in bold cyan
- Code spans in bold white
- Line numbers in blue
- Underlines in the diagnostic's severity color

JSON output includes the rendered string as a `rendered` field
alongside the structured diagnostic — agents get both.

---

## 4. Where It Applies

| Surface | Diagnostic codes | Source file type | Benefit |
|---|---|---|---|
| `mc model validate` | MC2xxx | model YAML | Point to the exact field that failed validation |
| `mc model lint` | MC3xxx | model YAML | Point to the measure/rule missing description |
| `mc model narrate` | MC7001-MC7055 | template YAML | Point to the `when:` predicate or `finding_id` |
| PPTX profile | MC7060-MC7067 | profile YAML | Point to the section/alias that doesn't match registry |
| Formula parse errors | MC1xxx | model YAML (formula strings) | Point to the character in the formula that failed |
| Tessera recipes | future | recipe YAML | Point to the driver config or mapping that's wrong |

The highest-leverage targets are **formula parse errors** (MC1xxx)
and **template validation** (MC7xxx) — these are the places where
users and agents most often need to know "which line, which character."

---

## 5. Implementation Phasing

**Phase 1 (small, standalone):** Ship the `SourceSpan` type, the
renderer function, and wire it into `mc model validate` for MC2xxx
codes. This proves the pattern with the most mature diagnostic
surface. Estimated: 1-2 sessions.

**Phase 2:** Wire into `mc model lint` (MC3xxx) and formula parser
(MC1xxx). The formula parser already tracks character positions
internally — surfacing them as `SourceSpan` is a data-plumbing
exercise.

**Phase 3:** Wire into `mc model narrate` template validation
(MC7xxx) and PPTX profile validation (MC706x). These are newer
surfaces with fewer diagnostics.

**Phase 4:** JSON envelope integration — the `rendered` field in
diagnostic JSON lets agents consume the visual output alongside
the structured data.

---

## 6. No New Dependencies

The renderer is hand-rolled (~100-150 lines). The Rust ecosystem
crates (`ariadne`, `codespan-reporting`, `miette`) are excellent
but each pulls significant transitive deps. The rendering logic is
simple enough (line extraction, underline drawing, ANSI color codes)
that a hand-rolled version fits the project's "hand-rolled wins over
deps" convention (process-notes §5).

---

## 7. Agent UX

This is particularly valuable for LLM-assisted authoring (Phase 4A
plugin, Phase 7C template generation). When the agent writes a
template and validation fails, the diagnostic with source context
gives the agent the exact line and character to fix — no need to
re-read the entire file to locate the problem.

The JSON envelope with both structured diagnostic and rendered
string means agents can choose: parse the structured fields for
programmatic fixes, or read the rendered string for human-style
understanding of the error.

---

*End of research note. Rich diagnostics are a UX multiplier across
every Mosaic surface — for humans reading CLI output, for agents
fixing validation errors, and for the future web UI rendering errors
inline. The pattern is proven (Rust, TypeScript, Elm all ship this),
the implementation is small (~150 lines for the renderer), and the
phasing lets it land incrementally starting with the most mature
diagnostic surface.*
