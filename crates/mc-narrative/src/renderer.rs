//! Template string substitution and format hints.
//!
//! Handles `{placeholder}` substitution in template strings,
//! including inline format specifiers (`{value:.1f}`, `{value:,.0f}`)
//! and named format hints (`currency`, `percent_1`, `count`, etc.).
//!
//! Session 4: adds named format hints from the planning doc.

use crate::evaluator::{Ctx, Val};
use std::collections::{BTreeMap, HashMap};

/// Substitute `{placeholder}` tokens in a template string.
///
/// Looks up each placeholder name in `bindings` first, then `ctx`.
/// Supports inline format specifiers (`{name:format}`) and named
/// format hints from the template's `format:` map.
pub fn substitute(
    template: &str,
    bindings: &HashMap<String, Val>,
    ctx: &Ctx,
    format_hints: &BTreeMap<String, String>,
) -> String {
    let mut result = String::with_capacity(template.len() * 2);
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' {
            let mut placeholder = String::new();
            while let Some(&c) = chars.peek() {
                if c == '}' {
                    chars.next();
                    break;
                }
                placeholder.push(c);
                chars.next();
            }
            // Parse format spec: {name:format} or just {name}
            let (name, inline_fmt) = match placeholder.split_once(':') {
                Some((n, f)) => (n.trim(), Some(f.trim())),
                None => (placeholder.trim(), None),
            };

            // Lookup in bindings first, then context.
            let val = bindings
                .get(name)
                .or_else(|| ctx.get(name))
                .cloned()
                .unwrap_or(Val::Str("N/A".into()));

            // Named format hint takes precedence over inline spec.
            let formatted = if let Some(hint) = format_hints.get(name) {
                apply_named_format(&val, hint)
            } else {
                format_val(&val, inline_fmt)
            };

            result.push_str(&formatted);
        } else {
            result.push(ch);
        }
    }

    collapse_whitespace(&result)
}

/// Apply a named format hint to a value.
///
/// Named hints per the planning doc:
/// - `currency` → $11,500
/// - `percent_0/1/2` → 23% / 23.4% / 23.41%
/// - `count` → 8,420
/// - `count_short` → 8.4K / 1.2M
/// - `delta_signed` → +47 / -312
/// - `date_short` → Mar 2026
/// - `date_long` → March 2026
/// - `decimal_2` → 0.42
/// - `period_relative` → pass-through (future extension)
pub fn apply_named_format(val: &Val, hint: &str) -> String {
    match val {
        Val::Num(n) => match hint {
            "currency" => {
                let formatted = format_comma(n.abs(), 0);
                if *n < 0.0 {
                    format!("-${formatted}")
                } else {
                    format!("${formatted}")
                }
            }
            "percent_0" => format!("{:.0}%", n),
            "percent_1" => format!("{:.1}%", n),
            "percent_2" => format!("{:.2}%", n),
            "count" => format_comma(*n, 0),
            "count_short" => format_short(*n),
            "delta_signed" => {
                let formatted = format_comma(n.abs(), 0);
                if *n >= 0.0 {
                    format!("+{formatted}")
                } else {
                    format!("-{formatted}")
                }
            }
            "decimal_2" => format!("{:.2}", n),
            _ => val.to_display(),
        },
        Val::Str(s) => match hint {
            "date_short" | "date_long" | "period_relative" => s.clone(),
            _ => s.clone(),
        },
        _ => val.to_display(),
    }
}

/// Format a number in abbreviated form (8.4K, 1.2M).
fn format_short(n: f64) -> String {
    let abs = n.abs();
    let sign = if n < 0.0 { "-" } else { "" };
    if abs >= 1_000_000.0 {
        format!("{sign}{:.1}M", abs / 1_000_000.0)
    } else if abs >= 1_000.0 {
        format!("{sign}{:.1}K", abs / 1_000.0)
    } else {
        format!("{sign}{:.0}", abs)
    }
}

/// Format a value with an optional inline format specifier.
pub fn format_val(val: &Val, fmt: Option<&str>) -> String {
    let fmt = match fmt {
        Some(f) => f,
        None => return val.to_display(),
    };

    match val {
        Val::Num(n) => {
            let use_comma = fmt.contains(',');
            let decimals = fmt
                .chars()
                .skip_while(|c| *c != '.')
                .skip(1)
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<usize>()
                .unwrap_or(0);

            if use_comma {
                format_comma(*n, decimals)
            } else {
                format!("{:.prec$}", n, prec = decimals)
            }
        }
        _ => val.to_display(),
    }
}

/// Format a number with comma-separated thousands.
pub fn format_comma(n: f64, decimals: usize) -> String {
    let rounded = if decimals == 0 {
        n.round() as i64
    } else {
        let factor = 10f64.powi(decimals as i32);
        (n * factor).round() as i64 / factor.round() as i64
    };
    let abs = rounded.unsigned_abs();
    let s = abs.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    if rounded < 0 {
        result.push('-');
    }
    result.chars().rev().collect()
}

/// Collapse runs of whitespace into a single space and trim.
fn collapse_whitespace(s: &str) -> String {
    let mut cleaned = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                cleaned.push(' ');
            }
            prev_space = true;
        } else {
            cleaned.push(ch);
            prev_space = false;
        }
    }
    cleaned.trim().to_string()
}

/// Convert a raw category string to a human-readable name.
pub fn readable_name(s: &str) -> String {
    let out = s.replace('_', " ");
    collapse_whitespace(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute() {
        let mut bindings = HashMap::new();
        bindings.insert("name".into(), Val::Str("Tablet".into()));
        bindings.insert("pct".into(), Val::Num(83.5));
        let ctx = HashMap::new();
        let hints = BTreeMap::new();
        let result = substitute("Device {name} at {pct:.1f}%", &bindings, &ctx, &hints);
        assert_eq!(result, "Device Tablet at 83.5%");
    }

    #[test]
    fn test_substitute_with_named_format() {
        let mut bindings = HashMap::new();
        bindings.insert("spend".into(), Val::Num(11500.0));
        bindings.insert("clicks".into(), Val::Num(8420.0));
        let ctx = HashMap::new();
        let mut hints = BTreeMap::new();
        hints.insert("spend".into(), "currency".into());
        hints.insert("clicks".into(), "count".into());
        let result = substitute(
            "Spent {spend}, got {clicks} clicks",
            &bindings,
            &ctx,
            &hints,
        );
        assert_eq!(result, "Spent $11,500, got 8,420 clicks");
    }

    #[test]
    fn test_format_comma() {
        assert_eq!(format_comma(55757.0, 0), "55,757");
        assert_eq!(format_comma(1000000.0, 0), "1,000,000");
        assert_eq!(format_comma(42.0, 0), "42");
    }

    #[test]
    fn test_named_format_hints() {
        assert_eq!(
            apply_named_format(&Val::Num(11500.0), "currency"),
            "$11,500"
        );
        assert_eq!(apply_named_format(&Val::Num(23.4), "percent_1"), "23.4%");
        assert_eq!(apply_named_format(&Val::Num(23.0), "percent_0"), "23%");
        assert_eq!(apply_named_format(&Val::Num(8420.0), "count"), "8,420");
        assert_eq!(apply_named_format(&Val::Num(8420.0), "count_short"), "8.4K");
        assert_eq!(
            apply_named_format(&Val::Num(1200000.0), "count_short"),
            "1.2M"
        );
        assert_eq!(apply_named_format(&Val::Num(47.0), "delta_signed"), "+47");
        assert_eq!(
            apply_named_format(&Val::Num(-312.0), "delta_signed"),
            "-312"
        );
        assert_eq!(apply_named_format(&Val::Num(0.42), "decimal_2"), "0.42");
    }

    #[test]
    fn test_readable_name() {
        assert_eq!(readable_name("Jul_2025"), "Jul 2025");
        assert_eq!(readable_name("Mobile_Phone"), "Mobile Phone");
    }
}
