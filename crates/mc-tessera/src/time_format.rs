//! Hand-rolled strptime subset used to parse non-ISO date columns at
//! row-transform time.
//!
//! Phase 6A.1 MAJ-1: `time_format` is declared on `ColumnMappingConfig`
//! and validated by MC5030, but until this module landed it was never
//! actually consumed. Models that set `time_format: "%m/%d/%Y"` were
//! silently parsed against the Time-dim element names as raw strings.
//!
//! Per process-notes Rule 5 ("hand-rolled wins over deps"): no `chrono`,
//! no `time` crate. We support the format tokens the project already
//! documents, which is the minimum that makes existing recipe authors'
//! `time_format` declarations actually work.
//!
//! Supported tokens:
//!
//! | Token | Meaning                        | Example  |
//! |-------|--------------------------------|----------|
//! | `%Y`  | 4-digit year                   | `2026`   |
//! | `%m`  | zero-padded 2-digit month      | `01`–`12`|
//! | `%d`  | zero-padded 2-digit day        | `01`–`31`|
//! | `%H`  | zero-padded 2-digit hour       | `00`–`23`|
//! | `%M`  | zero-padded 2-digit minute     | `00`–`59`|
//! | `%S`  | zero-padded 2-digit second     | `00`–`59`|
//! | `%V`  | zero-padded ISO week (numeric) | `01`–`53`|
//! | `%b`  | abbreviated English month      | `Jan`–`Dec` |
//! | `%%`  | literal `%`                    | `%`      |
//!
//! Anything else in the format string is matched as a literal — including
//! whitespace, dashes, slashes, `T`, etc. Time-zone tokens are not in
//! scope (timezone normalization is MC5031/5032's territory).
//!
//! Example: `parse_strptime("01/15/2026", "%m/%d/%Y")` → `Ok(parts)` with
//! `year=2026, month=1, day=15`.

/// Parts extracted by [`parse_strptime`]. All fields are optional; only
/// what the format string explicitly asked for is filled in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParsedTimeParts {
    pub year: Option<i32>,
    pub month: Option<u32>,
    pub day: Option<u32>,
    pub hour: Option<u32>,
    pub minute: Option<u32>,
    pub second: Option<u32>,
    pub iso_week: Option<u32>,
}

/// Parse failure mode — always a per-row data error in production;
/// callers wrap this into a `TesseraErrorOwned` (MC5034) for the
/// `on_error` policy machinery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
}

/// Parse `input` against the strptime-style `format`. See module doc for
/// supported tokens. Returns the extracted [`ParsedTimeParts`] or a
/// [`ParseError`] describing the mismatch.
pub fn parse_strptime(input: &str, format: &str) -> Result<ParsedTimeParts, ParseError> {
    let in_bytes = input.as_bytes();
    let fmt_bytes = format.as_bytes();
    let mut i = 0usize;
    let mut f = 0usize;
    let mut parts = ParsedTimeParts::default();

    while f < fmt_bytes.len() {
        if fmt_bytes[f] != b'%' {
            if i >= in_bytes.len() || in_bytes[i] != fmt_bytes[f] {
                return Err(ParseError {
                    message: format!(
                        "expected literal {:?} at position {} of input {:?}",
                        fmt_bytes[f] as char, i, input
                    ),
                });
            }
            i += 1;
            f += 1;
            continue;
        }
        // `%` token
        if f + 1 >= fmt_bytes.len() {
            return Err(ParseError {
                message: format!("dangling % at end of format {format:?}"),
            });
        }
        let tok = fmt_bytes[f + 1];
        f += 2;
        match tok {
            b'%' => {
                if i >= in_bytes.len() || in_bytes[i] != b'%' {
                    return Err(ParseError {
                        message: format!("expected literal % at position {i} of input {input:?}"),
                    });
                }
                i += 1;
            }
            b'Y' => {
                let v = take_fixed_digits(in_bytes, &mut i, 4, "%Y", input)?;
                parts.year = Some(v as i32);
            }
            b'm' => {
                let v = take_fixed_digits(in_bytes, &mut i, 2, "%m", input)?;
                if !(1..=12).contains(&v) {
                    return Err(ParseError {
                        message: format!("month {v} out of range 1..=12 in input {input:?}"),
                    });
                }
                parts.month = Some(v);
            }
            b'd' => {
                let v = take_fixed_digits(in_bytes, &mut i, 2, "%d", input)?;
                if !(1..=31).contains(&v) {
                    return Err(ParseError {
                        message: format!("day {v} out of range 1..=31 in input {input:?}"),
                    });
                }
                parts.day = Some(v);
            }
            b'H' => {
                let v = take_fixed_digits(in_bytes, &mut i, 2, "%H", input)?;
                if v > 23 {
                    return Err(ParseError {
                        message: format!("hour {v} out of range 0..=23 in input {input:?}"),
                    });
                }
                parts.hour = Some(v);
            }
            b'M' => {
                let v = take_fixed_digits(in_bytes, &mut i, 2, "%M", input)?;
                if v > 59 {
                    return Err(ParseError {
                        message: format!("minute {v} out of range 0..=59 in input {input:?}"),
                    });
                }
                parts.minute = Some(v);
            }
            b'S' => {
                let v = take_fixed_digits(in_bytes, &mut i, 2, "%S", input)?;
                if v > 60 {
                    return Err(ParseError {
                        message: format!("second {v} out of range 0..=60 in input {input:?}"),
                    });
                }
                parts.second = Some(v);
            }
            b'V' => {
                let v = take_fixed_digits(in_bytes, &mut i, 2, "%V", input)?;
                if !(1..=53).contains(&v) {
                    return Err(ParseError {
                        message: format!("ISO week {v} out of range 1..=53 in input {input:?}"),
                    });
                }
                parts.iso_week = Some(v);
            }
            b'b' => {
                let m = take_month_abbrev(in_bytes, &mut i, input)?;
                parts.month = Some(m);
            }
            other => {
                return Err(ParseError {
                    message: format!(
                        "unsupported format specifier %{} (supported: %Y %m %d %H %M %S %V %b %%)",
                        other as char
                    ),
                });
            }
        }
    }

    if i != in_bytes.len() {
        return Err(ParseError {
            message: format!("trailing data {:?} after format consumed", &input[i..]),
        });
    }
    Ok(parts)
}

fn take_fixed_digits(
    in_bytes: &[u8],
    cursor: &mut usize,
    n: usize,
    token: &str,
    input: &str,
) -> Result<u32, ParseError> {
    if *cursor + n > in_bytes.len() {
        return Err(ParseError {
            message: format!(
                "{token} expects {n} digits at position {} of input {input:?}",
                *cursor
            ),
        });
    }
    let slice = &in_bytes[*cursor..*cursor + n];
    let mut v: u32 = 0;
    for &b in slice {
        if !b.is_ascii_digit() {
            return Err(ParseError {
                message: format!(
                    "{token} expects digits at position {} of input {input:?}",
                    *cursor
                ),
            });
        }
        v = v * 10 + (b - b'0') as u32;
    }
    *cursor += n;
    Ok(v)
}

fn take_month_abbrev(in_bytes: &[u8], cursor: &mut usize, input: &str) -> Result<u32, ParseError> {
    if *cursor + 3 > in_bytes.len() {
        return Err(ParseError {
            message: format!(
                "%b expects 3-letter month at position {} of input {input:?}",
                *cursor
            ),
        });
    }
    let abbr = &in_bytes[*cursor..*cursor + 3];
    let upper: [u8; 3] = [
        abbr[0].to_ascii_uppercase(),
        abbr[1].to_ascii_uppercase(),
        abbr[2].to_ascii_uppercase(),
    ];
    let m = match &upper {
        b"JAN" => 1,
        b"FEB" => 2,
        b"MAR" => 3,
        b"APR" => 4,
        b"MAY" => 5,
        b"JUN" => 6,
        b"JUL" => 7,
        b"AUG" => 8,
        b"SEP" => 9,
        b"OCT" => 10,
        b"NOV" => 11,
        b"DEC" => 12,
        _ => {
            return Err(ParseError {
                message: format!(
                    "%b: unknown month abbreviation {:?} at position {} of input {input:?}",
                    std::str::from_utf8(abbr).unwrap_or("?"),
                    *cursor
                ),
            });
        }
    };
    *cursor += 3;
    Ok(m)
}

/// Canonicalize parsed time parts into a Time-element name string under
/// the given period. Returns `None` if the parts don't carry the fields
/// the period needs (e.g., `period: "month"` requires both year + month).
///
/// Period mapping:
///
/// | period      | output form     | example      |
/// |-------------|-----------------|--------------|
/// | `"year"`    | `YYYY`          | `2026`       |
/// | `"quarter"` | `YYYY-Qn`       | `2026-Q1`    |
/// | `"month"`   | `YYYY-MM`       | `2026-01`    |
/// | `"week"`    | `YYYY-Www`      | `2026-W03`   |
/// | `"day"`     | `YYYY-MM-DD`    | `2026-01-15` |
///
/// `"week"` requires `parts.iso_week` (parsed via `%V`). ISO-week
/// computation from a y/m/d triple is intentionally not implemented in
/// Phase 6A.1 — recipes that need week-bucketing must include `%V` in
/// their `time_format`.
pub fn canonicalize_period(parts: &ParsedTimeParts, period: &str) -> Option<String> {
    let year = parts.year?;
    match period {
        "year" => Some(format!("{year:04}")),
        "quarter" => {
            let m = parts.month?;
            let q = ((m - 1) / 3) + 1;
            Some(format!("{year:04}-Q{q}"))
        }
        "month" => {
            let m = parts.month?;
            Some(format!("{year:04}-{m:02}"))
        }
        "week" => {
            let w = parts.iso_week?;
            Some(format!("{year:04}-W{w:02}"))
        }
        "day" => {
            let m = parts.month?;
            let d = parts.day?;
            Some(format!("{year:04}-{m:02}-{d:02}"))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_us_locale_date() {
        let p = parse_strptime("01/15/2026", "%m/%d/%Y").unwrap();
        assert_eq!(p.year, Some(2026));
        assert_eq!(p.month, Some(1));
        assert_eq!(p.day, Some(15));
    }

    #[test]
    fn parse_iso_month() {
        let p = parse_strptime("2026-03", "%Y-%m").unwrap();
        assert_eq!(p.year, Some(2026));
        assert_eq!(p.month, Some(3));
        assert_eq!(p.day, None);
    }

    #[test]
    fn parse_iso_full_date() {
        let p = parse_strptime("2026-05-05", "%Y-%m-%d").unwrap();
        assert_eq!(p.year, Some(2026));
        assert_eq!(p.month, Some(5));
        assert_eq!(p.day, Some(5));
    }

    #[test]
    fn parse_with_time_of_day() {
        let p = parse_strptime("2026-05-05 13:42:07", "%Y-%m-%d %H:%M:%S").unwrap();
        assert_eq!(p.hour, Some(13));
        assert_eq!(p.minute, Some(42));
        assert_eq!(p.second, Some(7));
    }

    #[test]
    fn parse_iso_week() {
        let p = parse_strptime("2026-W03", "%Y-W%V").unwrap();
        assert_eq!(p.year, Some(2026));
        assert_eq!(p.iso_week, Some(3));
    }

    #[test]
    fn parse_dd_mon_yyyy() {
        let p = parse_strptime("15-Jan-2026", "%d-%b-%Y").unwrap();
        assert_eq!(p.day, Some(15));
        assert_eq!(p.month, Some(1));
        assert_eq!(p.year, Some(2026));
    }

    #[test]
    fn parse_rejects_trailing_data() {
        let err = parse_strptime("2026-01extra", "%Y-%m").unwrap_err();
        assert!(err.message.contains("trailing data"));
    }

    #[test]
    fn parse_rejects_bad_literal() {
        let err = parse_strptime("2026/01", "%Y-%m").unwrap_err();
        assert!(err.message.contains("expected literal"));
    }

    #[test]
    fn parse_rejects_out_of_range_month() {
        let err = parse_strptime("13/01/2026", "%m/%d/%Y").unwrap_err();
        assert!(err.message.contains("month 13"));
    }

    #[test]
    fn parse_rejects_unknown_token() {
        let err = parse_strptime("anything", "%Q").unwrap_err();
        assert!(err.message.contains("unsupported format specifier"));
    }

    #[test]
    fn canonicalize_to_month() {
        let p = parse_strptime("01/15/2026", "%m/%d/%Y").unwrap();
        assert_eq!(canonicalize_period(&p, "month").unwrap(), "2026-01");
    }

    #[test]
    fn canonicalize_to_quarter() {
        let p = parse_strptime("07/15/2026", "%m/%d/%Y").unwrap();
        assert_eq!(canonicalize_period(&p, "quarter").unwrap(), "2026-Q3");
    }

    #[test]
    fn canonicalize_to_year() {
        let p = parse_strptime("07/15/2026", "%m/%d/%Y").unwrap();
        assert_eq!(canonicalize_period(&p, "year").unwrap(), "2026");
    }

    #[test]
    fn canonicalize_to_day() {
        let p = parse_strptime("07/15/2026", "%m/%d/%Y").unwrap();
        assert_eq!(canonicalize_period(&p, "day").unwrap(), "2026-07-15");
    }

    #[test]
    fn canonicalize_to_week_requires_iso_week() {
        let p = parse_strptime("01/15/2026", "%m/%d/%Y").unwrap();
        assert_eq!(canonicalize_period(&p, "week"), None);
        let p = parse_strptime("2026-W03", "%Y-W%V").unwrap();
        assert_eq!(canonicalize_period(&p, "week").unwrap(), "2026-W03");
    }

    #[test]
    fn canonicalize_unknown_period_returns_none() {
        let p = parse_strptime("01/15/2026", "%m/%d/%Y").unwrap();
        assert_eq!(canonicalize_period(&p, "fortnight"), None);
    }
}
