//! Mosaic diagnostic types and Rust-style source-span renderer.
//!
//! Per [ADR-0024](../../../docs/decisions/0024-rich-diagnostic-rendering.md):
//! a shared crate providing `SourceSpan` (byte-offset spans), `RichDiagnostic`
//! (unified diagnostic type), and a hand-rolled renderer that produces
//! Rust-compiler-style output with file, line, underline, and help text.
//!
//! Zero runtime deps beyond `serde` (for JSON serialization). No `ariadne`,
//! no `codespan-reporting`, no `miette`.

pub mod diagnostic;
pub mod render;
pub mod span;

pub use diagnostic::{DiagSeverity, RelatedSpan, RichDiagnostic, Suggestion, SuggestionKind};
pub use render::{render_diagnostic, ColorMode};
pub use span::SourceSpan;
