//! Parsing atoms into the physical values behind Polars' temporal dtypes.

use chrono::{DateTime, LocalResult, NaiveDate, NaiveDateTime, TimeZone};

const NS_PER_US: i128 = 1_000;
const NS_PER_MS: i128 = 1_000_000;
const NS_PER_S: i128 = 1_000_000_000;

/// Time units as Polars names them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Unit {
    Ns,
    Us,
    Ms,
}

impl Unit {
    fn ns(self) -> i128 {
        match self {
            Unit::Ns => 1,
            Unit::Us => NS_PER_US,
            Unit::Ms => NS_PER_MS,
        }
    }
}

fn epoch() -> NaiveDate {
    NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()
}

/// Days since the epoch. Defaults to ISO `2026-10-12`.
pub fn parse_date(s: &str, format: Option<&str>) -> Option<i32> {
    let date = NaiveDate::parse_from_str(s, format.unwrap_or("%Y-%m-%d")).ok()?;
    i32::try_from((date - epoch()).num_days()).ok()
}

const NAIVE_FORMATS: &[&str] = &[
    "%Y-%m-%dT%H:%M:%S%.f",
    "%Y-%m-%d %H:%M:%S%.f",
    "%Y-%m-%dT%H:%M",
    "%Y-%m-%d %H:%M",
];

const OFFSET_FORMATS: &[&str] = &["%Y-%m-%dT%H:%M:%S%.f%z", "%Y-%m-%d %H:%M:%S%.f%z"];

/// Either an absolute instant (the input had an offset) or a wall-clock time.
enum Parsed {
    Utc(NaiveDateTime),
    Naive(NaiveDateTime),
}

fn parse_any_datetime(s: &str, format: Option<&str>) -> Option<Parsed> {
    let utc = |dt: DateTime<chrono::FixedOffset>| Parsed::Utc(dt.naive_utc());
    if let Some(fmt) = format {
        return DateTime::parse_from_str(s, fmt)
            .map(utc)
            .or_else(|_| NaiveDateTime::parse_from_str(s, fmt).map(Parsed::Naive))
            .ok();
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(utc(dt));
    }
    if let Some(dt) = OFFSET_FORMATS
        .iter()
        .find_map(|f| DateTime::parse_from_str(s, f).ok())
    {
        return Some(utc(dt));
    }
    if let Some(dt) = NAIVE_FORMATS
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(s, f).ok())
    {
        return Some(Parsed::Naive(dt));
    }
    // A bare date means midnight.
    let date = NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()?;
    Some(Parsed::Naive(date.and_hms_opt(0, 0, 0)?))
}

/// Timestamp in `unit` since the epoch, as Polars stores `Datetime(unit, tz)`.
///
/// Inputs with an offset (`Z`, `+02:00`) are absolute instants. Inputs
/// without one are wall-clock times in `tz`, or plain naive values if the
/// dtype has no time zone. Wall-clock times that don't exist in `tz` (DST gaps)
/// are null; ambiguous ones take the earlier instant.
pub fn parse_datetime(
    s: &str,
    format: Option<&str>,
    unit: Unit,
    tz: Option<&chrono_tz::Tz>,
) -> Option<i64> {
    let utc = match (parse_any_datetime(s, format)?, tz) {
        (Parsed::Utc(dt), _) | (Parsed::Naive(dt), None) => dt,
        (Parsed::Naive(dt), Some(tz)) => match tz.from_local_datetime(&dt) {
            LocalResult::Single(t) | LocalResult::Ambiguous(t, _) => t.naive_utc(),
            LocalResult::None => return None,
        },
    };
    let ns = utc
        .and_utc()
        .timestamp_nanos_opt()
        .map(i128::from)
        .or_else(|| {
            // Outside the i64-nanosecond range; coarser units may still fit.
            Some(i128::from(utc.and_utc().timestamp_micros()) * NS_PER_US)
        })?;
    i64::try_from(ns.div_euclid(unit.ns())).ok()
}

/// Duration in `unit`. Accepts compound forms like `1d2h30m`, `1.5s`, `-250ms`
/// (units: w, d, h, m, s, ms, us, ns), or a bare integer already in `unit`.
pub fn parse_duration(s: &str, unit: Unit) -> Option<i64> {
    if let Ok(n) = s.parse::<i64>() {
        return Some(n);
    }
    let (negative, mut rest) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    if rest.is_empty() {
        return None;
    }
    let mut total_ns: i128 = 0;
    while !rest.is_empty() {
        let num_len = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        let (num, tail) = rest.split_at(num_len);
        let unit_len = tail
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(tail.len());
        let (unit_str, tail) = tail.split_at(unit_len);
        let unit_ns: i128 = match unit_str {
            "w" => 7 * 86_400 * NS_PER_S,
            "d" => 86_400 * NS_PER_S,
            "h" => 3_600 * NS_PER_S,
            "m" => 60 * NS_PER_S,
            "s" => NS_PER_S,
            "ms" => NS_PER_MS,
            "us" => NS_PER_US,
            "ns" => 1,
            _ => return None,
        };
        total_ns = total_ns.checked_add(scaled(num, unit_ns)?)?;
        rest = tail;
    }
    let value = total_ns / unit.ns();
    i64::try_from(if negative { -value } else { value }).ok()
}

/// `num` (like `12` or `1.25`) times `unit_ns`, without float rounding.
fn scaled(num: &str, unit_ns: i128) -> Option<i128> {
    let (int, frac) = num.split_once('.').unwrap_or((num, ""));
    if int.is_empty() && frac.is_empty() {
        return None;
    }
    let int: i128 = if int.is_empty() { 0 } else { int.parse().ok()? };
    let mut out = int.checked_mul(unit_ns)?;
    let mut place = unit_ns;
    for d in frac.chars() {
        place /= 10;
        out += i128::from(d.to_digit(10)?) * place;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(parse_date("1970-01-02", None), Some(1));
        assert_eq!(parse_date("1969-12-31", None), Some(-1));
        assert_eq!(
            parse_date("12/10/2026", Some("%d/%m/%Y")),
            parse_date("2026-10-12", None)
        );
        assert_eq!(parse_date("nope", None), None);
    }

    #[test]
    fn datetimes() {
        let s = |x: &str| parse_datetime(x, None, Unit::Ms, None);
        assert_eq!(s("1970-01-01T00:00:01"), Some(1_000));
        assert_eq!(s("1970-01-01 00:00:01.5"), Some(1_500));
        assert_eq!(s("1970-01-01T01:00:00+01:00"), Some(0));
        assert_eq!(s("1970-01-01 00:00:00Z"), Some(0));
        assert_eq!(s("1970-01-02"), Some(86_400_000));
        assert_eq!(s("garbage"), None);
        let ny: chrono_tz::Tz = "America/New_York".parse().unwrap();
        assert_eq!(
            parse_datetime("1970-01-01 00:00:00", None, Unit::Ms, Some(&ny)),
            Some(5 * 3_600_000)
        );
        assert_eq!(
            parse_datetime("1970-01-01 00:00:00Z", None, Unit::Ms, Some(&ny)),
            Some(0)
        );
        // DST gap.
        assert_eq!(
            parse_datetime("2026-03-08 02:30", None, Unit::Ms, Some(&ny)),
            None
        );
    }

    #[test]
    fn durations() {
        assert_eq!(
            parse_duration("1d2h30m", Unit::Ms),
            Some((26 * 60 + 30) * 60_000)
        );
        assert_eq!(parse_duration("1.5s", Unit::Us), Some(1_500_000));
        assert_eq!(parse_duration("-250ms", Unit::Ms), Some(-250));
        assert_eq!(parse_duration("1w", Unit::Ms), Some(7 * 86_400_000));
        assert_eq!(parse_duration("42", Unit::Ns), Some(42));
        assert_eq!(parse_duration("3x", Unit::Ms), None);
        assert_eq!(parse_duration("s", Unit::Ms), None);
        assert_eq!(parse_duration("", Unit::Ms), None);
        assert_eq!(parse_duration("-", Unit::Ms), None);
    }
}
