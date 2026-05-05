//! Minimal 5-field cron expression parser and next-fire calculator.
//!
//! Supports:
//! - Standard 5-field: `minute hour dom month dow`
//! - Wildcards (`*`), ranges (`1-5`), steps (`*/5`, `1-30/2`), lists (`1,3,5`)
//! - Named days: `MON`-`SUN` (case-insensitive), named months: `JAN`-`DEC`
//! - Presets: `@hourly`, `@daily`, `@weekly`, `@monthly`, `@yearly`
//! - Sub-minute intervals: `@every Ns`, `@every Nm`, `@every Nh`

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::TesseraError;

/// A parsed cron expression that can compute the next fire time.
#[derive(Debug, Clone)]
pub enum CronExpr {
    /// Standard 5-field cron.
    Fields {
        /// Allowed minutes (0-59).
        minutes: Vec<u8>,
        /// Allowed hours (0-23).
        hours: Vec<u8>,
        /// Allowed days of month (1-31).
        doms: Vec<u8>,
        /// Allowed months (1-12).
        months: Vec<u8>,
        /// Allowed days of week (0-6, 0=Sunday).
        dows: Vec<u8>,
    },
    /// Sub-minute interval (`@every Xs/Xm/Xh`).
    Every {
        /// Interval duration.
        interval: Duration,
    },
}

impl CronExpr {
    /// Parse a cron expression string.
    pub fn parse(expr: &str) -> Result<Self, TesseraError> {
        let trimmed = expr.trim();

        // Sub-minute intervals
        if let Some(rest) = trimmed.strip_prefix("@every ") {
            return Self::parse_every(rest.trim());
        }

        // Presets
        let fields_str = match trimmed {
            "@hourly" => "0 * * * *",
            "@daily" => "0 0 * * *",
            "@weekly" => "0 0 * * 0",
            "@monthly" => "0 0 1 * *",
            "@yearly" | "@annually" => "0 0 1 1 *",
            other => other,
        };

        let parts: Vec<&str> = fields_str.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(TesseraError::SidecarInconsistent {
                message: format!(
                    "cron expression must have 5 fields, got {}: {expr:?}",
                    parts.len()
                ),
            });
        }

        let minutes = parse_field(parts[0], 0, 59, &[])?;
        let hours = parse_field(parts[1], 0, 23, &[])?;
        let doms = parse_field(parts[2], 1, 31, &[])?;
        let months = parse_field(parts[3], 1, 12, MONTH_NAMES)?;
        let dows = parse_field(parts[4], 0, 6, DOW_NAMES)?;

        Ok(CronExpr::Fields {
            minutes,
            hours,
            doms,
            months,
            dows,
        })
    }

    /// Compute the next fire time strictly after `after`.
    pub fn next_fire(&self, after: SystemTime) -> SystemTime {
        match self {
            CronExpr::Every { interval } => after + *interval,
            CronExpr::Fields {
                minutes,
                hours,
                doms,
                months,
                dows,
            } => next_fire_fields(after, minutes, hours, doms, months, dows),
        }
    }

    fn parse_every(s: &str) -> Result<Self, TesseraError> {
        let err = || TesseraError::SidecarInconsistent {
            message: format!("invalid @every interval: {s:?}"),
        };

        if s.is_empty() {
            return Err(err());
        }

        let (num_str, multiplier) = if let Some(n) = s.strip_suffix('s') {
            (n, 1u64)
        } else if let Some(n) = s.strip_suffix('m') {
            (n, 60u64)
        } else if let Some(n) = s.strip_suffix('h') {
            (n, 3600u64)
        } else {
            return Err(err());
        };

        let n: u64 = num_str.parse().map_err(|_| err())?;
        if n == 0 {
            return Err(err());
        }

        Ok(CronExpr::Every {
            interval: Duration::from_secs(n * multiplier),
        })
    }
}

const MONTH_NAMES: &[(&str, u8)] = &[
    ("JAN", 1),
    ("FEB", 2),
    ("MAR", 3),
    ("APR", 4),
    ("MAY", 5),
    ("JUN", 6),
    ("JUL", 7),
    ("AUG", 8),
    ("SEP", 9),
    ("OCT", 10),
    ("NOV", 11),
    ("DEC", 12),
];

const DOW_NAMES: &[(&str, u8)] = &[
    ("SUN", 0),
    ("MON", 1),
    ("TUE", 2),
    ("WED", 3),
    ("THU", 4),
    ("FRI", 5),
    ("SAT", 6),
];

/// Parse a single cron field into a sorted list of allowed values.
fn parse_field(
    field: &str,
    min: u8,
    max: u8,
    names: &[(&str, u8)],
) -> Result<Vec<u8>, TesseraError> {
    let err = |msg: &str| TesseraError::SidecarInconsistent {
        message: format!("cron field parse error for {field:?}: {msg}"),
    };

    let mut result = Vec::new();

    for part in field.split(',') {
        let (range_part, step) = if let Some((r, s)) = part.split_once('/') {
            let step_val: u8 = s.parse().map_err(|_| err("invalid step value"))?;
            if step_val == 0 {
                return Err(err("step cannot be zero"));
            }
            (r, Some(step_val))
        } else {
            (part, None)
        };

        let (start, end) = if range_part == "*" {
            (min, max)
        } else if let Some((lo, hi)) = range_part.split_once('-') {
            let lo_val = parse_value(lo, min, max, names).map_err(|m| err(&m))?;
            let hi_val = parse_value(hi, min, max, names).map_err(|m| err(&m))?;
            (lo_val, hi_val)
        } else {
            let val = parse_value(range_part, min, max, names).map_err(|m| err(&m))?;
            if let Some(step_val) = step {
                // e.g. `5/15` means starting at 5, step by 15
                let mut v = val;
                while v <= max {
                    result.push(v);
                    v = v.saturating_add(step_val);
                }
                continue;
            }
            (val, val)
        };

        match step {
            Some(step_val) => {
                let mut v = start;
                while v <= end {
                    result.push(v);
                    v = v.saturating_add(step_val);
                }
            }
            None => {
                for v in start..=end {
                    result.push(v);
                }
            }
        }
    }

    result.sort_unstable();
    result.dedup();

    if result.is_empty() {
        return Err(err("empty field"));
    }

    Ok(result)
}

/// Parse a single value token, resolving named values.
fn parse_value(s: &str, min: u8, max: u8, names: &[(&str, u8)]) -> Result<u8, String> {
    // Try named lookup first
    let upper = s.to_uppercase();
    for &(name, val) in names {
        if upper == name {
            return Ok(val);
        }
    }

    let val: u8 = s.parse().map_err(|_| format!("invalid value: {s:?}"))?;

    if val < min || val > max {
        return Err(format!("value {val} out of range [{min}, {max}]"));
    }
    Ok(val)
}

/// Compute the next matching time after `after` for a 5-field cron expression.
/// Brute-forces forward minute-by-minute (capped at ~4 years to avoid infinite loops).
fn next_fire_fields(
    after: SystemTime,
    minutes: &[u8],
    hours: &[u8],
    doms: &[u8],
    months: &[u8],
    dows: &[u8],
) -> SystemTime {
    let secs_since_epoch = after
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Start at the next minute boundary after `after`.
    let start_secs = (secs_since_epoch / 60 + 1) * 60;

    // Cap search at 4 years (~2_102_400 minutes).
    let max_iter: u64 = 4 * 366 * 24 * 60;

    for i in 0..max_iter {
        let candidate_secs = start_secs + i * 60;
        let (y, mo, d, h, mi, _) = unix_to_ymdhms(candidate_secs);

        if !months.contains(&(mo as u8)) {
            continue;
        }
        if !doms.contains(&(d as u8)) {
            continue;
        }
        if !hours.contains(&(h as u8)) {
            continue;
        }
        if !minutes.contains(&(mi as u8)) {
            continue;
        }

        // Day of week check.
        let dow = day_of_week(y, mo, d);
        if !dows.contains(&dow) {
            continue;
        }

        return UNIX_EPOCH + Duration::from_secs(candidate_secs);
    }

    // Fallback: should not happen for valid expressions within 4 years.
    after + Duration::from_secs(3600)
}

/// Compute day-of-week (0=Sunday) using Tomohiko Sakamoto's algorithm.
fn day_of_week(year: i32, month: u32, day: u32) -> u8 {
    static T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    let dow = (y + y / 4 - y / 100 + y / 400 + T[(month - 1) as usize] + day as i32) % 7;
    if dow < 0 {
        (dow + 7) as u8
    } else {
        dow as u8
    }
}

/// Unix-secs (UTC) -> (year, month, day, hour, minute, second).
/// Howard Hinnant's algorithm (same as runner.rs — copied to avoid
/// depending on runner's private function).
fn unix_to_ymdhms(secs: u64) -> (i32, u32, u32, u32, u32, u32) {
    let s = secs as i64;
    let days = s.div_euclid(86_400);
    let rem = s.rem_euclid(86_400) as u32;
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    let ss = rem % 60;

    let z = days + 719_468;
    let era = if z >= 0 {
        z / 146_097
    } else {
        (z - 146_096) / 146_097
    };
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32, hh, mm, ss)
}

/// Format unix seconds as RFC3339 UTC timestamp.
fn now_rfc3339_from(secs: u64) -> String {
    let (y, m, d, hh, mm, ss) = unix_to_ymdhms(secs);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// Get current time as RFC3339 UTC string.
pub(crate) fn now_rfc3339() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    now_rfc3339_from(now.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_every_seconds() {
        let expr = CronExpr::parse("@every 30s").unwrap();
        match expr {
            CronExpr::Every { interval } => assert_eq!(interval, Duration::from_secs(30)),
            _ => panic!("expected Every"),
        }
    }

    #[test]
    fn parse_every_minutes() {
        let expr = CronExpr::parse("@every 5m").unwrap();
        match expr {
            CronExpr::Every { interval } => assert_eq!(interval, Duration::from_secs(300)),
            _ => panic!("expected Every"),
        }
    }

    #[test]
    fn parse_preset_hourly() {
        let expr = CronExpr::parse("@hourly").unwrap();
        match expr {
            CronExpr::Fields { minutes, hours, .. } => {
                assert_eq!(minutes, vec![0]);
                assert_eq!(hours, (0..=23).collect::<Vec<u8>>());
            }
            _ => panic!("expected Fields"),
        }
    }

    #[test]
    fn parse_standard_five_field() {
        let expr = CronExpr::parse("*/15 9-17 * * MON-FRI").unwrap();
        match expr {
            CronExpr::Fields {
                minutes,
                hours,
                doms,
                months,
                dows,
            } => {
                assert_eq!(minutes, vec![0, 15, 30, 45]);
                assert_eq!(hours, (9..=17).collect::<Vec<u8>>());
                assert_eq!(doms, (1..=31).collect::<Vec<u8>>());
                assert_eq!(months, (1..=12).collect::<Vec<u8>>());
                assert_eq!(dows, vec![1, 2, 3, 4, 5]);
            }
            _ => panic!("expected Fields"),
        }
    }

    #[test]
    fn next_fire_every() {
        let expr = CronExpr::parse("@every 60s").unwrap();
        let base = UNIX_EPOCH + Duration::from_secs(1000);
        let next = expr.next_fire(base);
        assert_eq!(next.duration_since(UNIX_EPOCH).unwrap().as_secs(), 1060);
    }

    #[test]
    fn next_fire_hourly() {
        let expr = CronExpr::parse("@hourly").unwrap();
        // 2026-01-01 00:30:00 UTC
        let base = UNIX_EPOCH + Duration::from_secs(1_767_225_000);
        let next = expr.next_fire(base);
        let next_secs = next.duration_since(UNIX_EPOCH).unwrap().as_secs();
        // Should fire at the next :00 minute mark
        assert_eq!(next_secs % 3600, 0);
        assert!(next_secs > 1_767_225_000);
    }

    #[test]
    fn parse_named_months_and_days() {
        let expr = CronExpr::parse("0 0 1 JAN,JUL SUN").unwrap();
        match expr {
            CronExpr::Fields { months, dows, .. } => {
                assert_eq!(months, vec![1, 7]);
                assert_eq!(dows, vec![0]);
            }
            _ => panic!("expected Fields"),
        }
    }

    #[test]
    fn parse_invalid_returns_error() {
        assert!(CronExpr::parse("bad").is_err());
        assert!(CronExpr::parse("@every 0s").is_err());
        assert!(CronExpr::parse("@every").is_err());
    }

    #[test]
    fn day_of_week_known_dates() {
        // 2026-05-05 is a Tuesday (2)
        assert_eq!(day_of_week(2026, 5, 5), 2);
        // 2024-01-01 is a Monday (1)
        assert_eq!(day_of_week(2024, 1, 1), 1);
        // 2023-01-01 is a Sunday (0)
        assert_eq!(day_of_week(2023, 1, 1), 0);
    }
}
