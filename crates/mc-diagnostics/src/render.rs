//! Rust-style diagnostic renderer.
//!
//! Per [ADR-0024](../../../docs/decisions/0024-rich-diagnostic-rendering.md)
//! Decision 3: a pure renderer that takes diagnostic + source provider →
//! string. No I/O, no global state.
//!
//! The renderer produces output matching Rust's compiler diagnostic format:
//!
//! ```text
//! error[MC2015]: measure referenced in rule body not found
//!   --> model.yaml:87:22
//!    |
//! 87 |     body: "Custmers * AOV"
//!    |            ^^^^^^^^ did you mean `Customers`?
//!    |
//!    = note: available measures: Customers, AOV, Revenue
//!    = help: check spelling of measure names
//! ```

use std::fmt::Write;

use crate::{DiagSeverity, RichDiagnostic, SourceSpan, SuggestionKind};

/// Controls ANSI color output.
///
/// Per ADR-0024 Decision 3: `Auto` strips ANSI when stdout isn't a TTY.
/// Detection uses `$NO_COLOR` env var — no external crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// Always emit ANSI escape codes.
    Always,
    /// Never emit ANSI escape codes.
    Never,
    /// Detect: strip colors if `$NO_COLOR` is set.
    Auto,
}

impl ColorMode {
    fn use_color(self) -> bool {
        match self {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => std::env::var_os("NO_COLOR").is_none(),
        }
    }
}

/// Tab width for normalization (tabs → spaces before drawing underlines).
const TAB_WIDTH: usize = 4;

// ANSI escape sequences
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const BLUE: &str = "\x1b[34m";

/// Render a diagnostic to a string with Rust-style source context.
///
/// `source_provider` returns the file content for a given path. It is called
/// once per unique file referenced by the diagnostic's spans.
///
/// The renderer is pure: no I/O, no global state.
pub fn render_diagnostic(
    diag: &RichDiagnostic,
    source_provider: impl Fn(&str) -> Option<String>,
    color: ColorMode,
) -> String {
    let c = color.use_color();
    let mut out = String::new();

    // Header line: error[MC2015]: message
    render_header(&mut out, diag, c);

    // Primary span
    if let Some(ref span) = diag.primary_span {
        if let Some(source) = source_provider(&span.file) {
            render_span_context(&mut out, span, None, &source, diag.severity, c);
        } else {
            // Graceful degradation: no source available
            let (line, col) = (1, 1);
            let _ = writeln!(out, "  --> {}:{}:{}", span.file, line, col);
        }
    }

    // Related spans
    for rel in &diag.related {
        if let Some(source) = source_provider(&rel.span.file) {
            render_span_context(
                &mut out,
                &rel.span,
                Some(&rel.label),
                &source,
                diag.severity,
                c,
            );
        }
    }

    // Notes and help
    if !diag.notes.is_empty() || !diag.help.is_empty() || diag.suggestion.is_some() {
        render_footer(&mut out, diag, c);
    }

    out
}

fn render_header(out: &mut String, diag: &RichDiagnostic, color: bool) {
    let sev = diag.severity;
    if color {
        let sev_color = severity_color(sev);
        let _ = write!(
            out,
            "{}{}{}[{}]{}: {}",
            BOLD,
            sev_color,
            sev.label(),
            diag.code,
            RESET,
            diag.message
        );
    } else {
        let _ = write!(out, "{}[{}]: {}", sev.label(), diag.code, diag.message);
    }
    out.push('\n');
}

fn render_span_context(
    out: &mut String,
    span: &SourceSpan,
    label: Option<&str>,
    source: &str,
    severity: DiagSeverity,
    color: bool,
) {
    let (start_line, start_col) = crate::span::byte_offset_to_line_col(source, span.start_byte);
    let (end_line, _end_col) = if span.end_byte > span.start_byte {
        crate::span::byte_offset_to_line_col(source, span.end_byte.saturating_sub(1))
    } else {
        (start_line, start_col)
    };

    let lines: Vec<&str> = source.lines().collect();
    let max_line_num = end_line;
    let gutter_width = digit_count(max_line_num);

    // Location arrow
    if color {
        let _ = writeln!(
            out,
            "  {}-->{} {}:{}:{}",
            BLUE, RESET, span.file, start_line, start_col
        );
    } else {
        let _ = writeln!(out, "  --> {}:{}:{}", span.file, start_line, start_col);
    }

    // Empty gutter line
    write_gutter_separator(out, gutter_width, color);

    if start_line == end_line {
        // Single-line span
        render_single_line_span(
            out,
            &lines,
            span,
            source,
            start_line,
            gutter_width,
            label,
            severity,
            color,
        );
    } else {
        // Multi-line span
        render_multi_line_span(
            out,
            &lines,
            span,
            source,
            start_line,
            end_line,
            gutter_width,
            label,
            severity,
            color,
        );
    }

    // Trailing gutter separator
    write_gutter_separator(out, gutter_width, color);
}

#[allow(clippy::too_many_arguments)]
fn render_single_line_span(
    out: &mut String,
    lines: &[&str],
    span: &SourceSpan,
    source: &str,
    line_num: usize,
    gutter_width: usize,
    label: Option<&str>,
    severity: DiagSeverity,
    color: bool,
) {
    if line_num == 0 || line_num > lines.len() {
        return;
    }
    let raw_line = lines[line_num - 1];
    let expanded = expand_tabs(raw_line);

    // Source line
    write_gutter_line(out, line_num, gutter_width, color);
    let _ = writeln!(out, " {}", expanded);

    // Underline
    let line_start_byte = byte_offset_of_line(source, line_num);
    let span_start_in_line = span.start_byte.saturating_sub(line_start_byte);
    let span_end_in_line = span
        .end_byte
        .saturating_sub(line_start_byte)
        .min(raw_line.len());
    let span_len = if span_end_in_line > span_start_in_line {
        span_end_in_line - span_start_in_line
    } else {
        1
    };

    // Compute visual column accounting for tab expansion
    let visual_start = visual_column(raw_line, span_start_in_line);
    let visual_len = visual_span_len(raw_line, span_start_in_line, span_len);

    write_gutter_empty(out, gutter_width, color);
    let padding = " ".repeat(visual_start + 1); // +1 for the space after gutter pipe
    let underline = "^".repeat(visual_len.max(1));

    if color {
        let sev_color = severity_color(severity);
        let _ = write!(out, "{}{}{}{}", padding, BOLD, sev_color, underline);
        if let Some(lbl) = label {
            let _ = write!(out, " {}", lbl);
        }
        let _ = writeln!(out, "{}", RESET);
    } else {
        let _ = write!(out, "{}{}", padding, underline);
        if let Some(lbl) = label {
            let _ = write!(out, " {}", lbl);
        }
        out.push('\n');
    }
}

#[allow(clippy::too_many_arguments)]
fn render_multi_line_span(
    out: &mut String,
    lines: &[&str],
    span: &SourceSpan,
    source: &str,
    start_line: usize,
    end_line: usize,
    gutter_width: usize,
    label: Option<&str>,
    severity: DiagSeverity,
    color: bool,
) {
    for line_num in start_line..=end_line {
        if line_num == 0 || line_num > lines.len() {
            continue;
        }

        // For multi-line, show "..." for middle lines if there are many
        if end_line - start_line > 4 && line_num > start_line + 1 && line_num < end_line - 1 {
            if line_num == start_line + 2 {
                write_gutter_dots(out, gutter_width, color);
            }
            continue;
        }

        let raw_line = lines[line_num - 1];
        let expanded = expand_tabs(raw_line);

        write_gutter_line(out, line_num, gutter_width, color);
        let _ = writeln!(out, " {}", expanded);

        // Underline for first and last line
        if line_num == start_line {
            let line_start_byte = byte_offset_of_line(source, line_num);
            let span_start_in_line = span.start_byte.saturating_sub(line_start_byte);
            let visual_start = visual_column(raw_line, span_start_in_line);
            let remaining = raw_line.len().saturating_sub(span_start_in_line);
            let visual_len = visual_span_len(raw_line, span_start_in_line, remaining);

            write_gutter_empty(out, gutter_width, color);
            let padding = " ".repeat(visual_start + 1);
            let underline = "^".repeat(visual_len.max(1));

            if color {
                let sev_color = severity_color(severity);
                let _ = writeln!(
                    out,
                    "{}{}{}{}{}",
                    padding, BOLD, sev_color, underline, RESET
                );
            } else {
                let _ = writeln!(out, "{}{}", padding, underline);
            }
        } else if line_num == end_line {
            let line_start_byte = byte_offset_of_line(source, line_num);
            let span_end_in_line = span
                .end_byte
                .saturating_sub(line_start_byte)
                .min(raw_line.len());
            let visual_len = visual_span_len(raw_line, 0, span_end_in_line);

            write_gutter_empty(out, gutter_width, color);
            let padding = " ";
            let underline = "^".repeat(visual_len.max(1));

            if color {
                let sev_color = severity_color(severity);
                let _ = write!(out, "{}{}{}{}", padding, BOLD, sev_color, underline);
                if let Some(lbl) = label {
                    let _ = write!(out, " {}", lbl);
                }
                let _ = writeln!(out, "{}", RESET);
            } else {
                let _ = write!(out, "{}{}", padding, underline);
                if let Some(lbl) = label {
                    let _ = write!(out, " {}", lbl);
                }
                out.push('\n');
            }
        }
    }
}

fn render_footer(out: &mut String, diag: &RichDiagnostic, color: bool) {
    for note in &diag.notes {
        if color {
            let _ = writeln!(out, "   {}= note:{} {}", CYAN, RESET, note);
        } else {
            let _ = writeln!(out, "   = note: {}", note);
        }
    }
    for h in &diag.help {
        if color {
            let _ = writeln!(out, "   {}= help:{} {}", CYAN, RESET, h);
        } else {
            let _ = writeln!(out, "   = help: {}", h);
        }
    }
    if let Some(ref suggestion) = diag.suggestion {
        match &suggestion.kind {
            SuggestionKind::Help(text) => {
                if color {
                    let _ = writeln!(
                        out,
                        "   {}= help:{} {}: {}",
                        CYAN, RESET, suggestion.message, text
                    );
                } else {
                    let _ = writeln!(out, "   = help: {}: {}", suggestion.message, text);
                }
            }
            SuggestionKind::Replace { replacement, .. } => {
                if color {
                    let _ = writeln!(
                        out,
                        "   {}= help:{} {}: `{}`",
                        CYAN, RESET, suggestion.message, replacement
                    );
                } else {
                    let _ = writeln!(out, "   = help: {}: `{}`", suggestion.message, replacement);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Gutter helpers
// ---------------------------------------------------------------------------

fn write_gutter_separator(out: &mut String, width: usize, color: bool) {
    let pad = " ".repeat(width + 1);
    if color {
        let _ = writeln!(out, "{}{} |{}", pad, BLUE, RESET);
    } else {
        let _ = writeln!(out, "{} |", pad);
    }
}

fn write_gutter_line(out: &mut String, line_num: usize, width: usize, color: bool) {
    if color {
        let _ = write!(
            out,
            " {}{:>width$} |{}",
            BLUE,
            line_num,
            RESET,
            width = width
        );
    } else {
        let _ = write!(out, " {:>width$} |", line_num, width = width);
    }
}

fn write_gutter_empty(out: &mut String, width: usize, color: bool) {
    let pad = " ".repeat(width + 1);
    if color {
        let _ = write!(out, "{}{} |{}", pad, BLUE, RESET);
    } else {
        let _ = write!(out, "{} |", pad);
    }
}

fn write_gutter_dots(out: &mut String, width: usize, color: bool) {
    if color {
        let _ = writeln!(
            out,
            "{}{}...{}",
            " ".repeat(width.saturating_sub(2)),
            BLUE,
            RESET
        );
    } else {
        let _ = writeln!(out, "{}...", " ".repeat(width.saturating_sub(2)));
    }
}

// ---------------------------------------------------------------------------
// Tab / column helpers
// ---------------------------------------------------------------------------

/// Expand tabs to `TAB_WIDTH` spaces.
fn expand_tabs(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for ch in line.chars() {
        if ch == '\t' {
            let spaces = TAB_WIDTH - (out.len() % TAB_WIDTH);
            for _ in 0..spaces {
                out.push(' ');
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Compute the visual column (0-based) for a byte offset within a line,
/// accounting for tab expansion.
fn visual_column(line: &str, byte_offset: usize) -> usize {
    let mut col = 0usize;
    for (i, ch) in line.char_indices() {
        if i >= byte_offset {
            break;
        }
        if ch == '\t' {
            col += TAB_WIDTH - (col % TAB_WIDTH);
        } else {
            col += 1;
        }
    }
    col
}

/// Compute the visual length of a span within a line, accounting for tab
/// expansion.
fn visual_span_len(line: &str, start_byte: usize, byte_len: usize) -> usize {
    let end_byte = start_byte + byte_len;
    let mut col = 0usize;
    let mut start_col = 0usize;
    let mut found_start = false;
    for (i, ch) in line.char_indices() {
        if i >= end_byte {
            break;
        }
        if i == start_byte {
            start_col = col;
            found_start = true;
        }
        if ch == '\t' {
            col += TAB_WIDTH - (col % TAB_WIDTH);
        } else {
            col += 1;
        }
    }
    if !found_start {
        start_col = col;
    }
    col.saturating_sub(start_col).max(1)
}

/// Find the byte offset of the start of a 1-based line number.
fn byte_offset_of_line(source: &str, line_num: usize) -> usize {
    if line_num <= 1 {
        return 0;
    }
    let mut current_line = 1usize;
    for (i, ch) in source.char_indices() {
        if ch == '\n' {
            current_line += 1;
            if current_line == line_num {
                return i + 1;
            }
        }
    }
    source.len()
}

fn digit_count(n: usize) -> usize {
    if n == 0 {
        return 1;
    }
    let mut count = 0;
    let mut val = n;
    while val > 0 {
        count += 1;
        val /= 10;
    }
    count
}

fn severity_color(sev: DiagSeverity) -> &'static str {
    match sev {
        DiagSeverity::Error => RED,
        DiagSeverity::Warning => YELLOW,
        DiagSeverity::Info => CYAN,
    }
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod render_tests;
