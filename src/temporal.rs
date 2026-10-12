use chrono::{DateTime, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use polars::prelude::TimeUnit;
use serde::{Deserialize, Serialize};

const NS_PER_US: i128 = 1_000;
const NS_PER_MS: i128 = 1_000_000;
const NS_PER_S: i128 = 1_000_000_000;

const ISO_DATE: &str = "%Y-%m-%d";

const ISO_WALL_CLOCK_FORMATS: &[&str] = &[
    "%Y-%m-%dT%H:%M:%S%.f",
    "%Y-%m-%d %H:%M:%S%.f",
    "%Y-%m-%dT%H:%M",
    "%Y-%m-%d %H:%M",
];

const ISO_OFFSET_FORMATS: &[&str] = &[
    "%Y-%m-%dT%H:%M:%S%.f%#z",
    "%Y-%m-%d %H:%M:%S%.f%#z",
    "%Y-%m-%dT%H:%M%#z",
    "%Y-%m-%d %H:%M%#z",
];

const ISO_TIME_FORMATS: &[&str] = &["%H:%M:%S%.f", "%H:%M"];

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

    pub fn polars(self) -> TimeUnit {
        match self {
            Unit::Ns => TimeUnit::Nanoseconds,
            Unit::Us => TimeUnit::Microseconds,
            Unit::Ms => TimeUnit::Milliseconds,
        }
    }

    pub fn from_polars(unit: TimeUnit) -> Self {
        match unit {
            TimeUnit::Nanoseconds => Unit::Ns,
            TimeUnit::Microseconds => Unit::Us,
            TimeUnit::Milliseconds => Unit::Ms,
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Unit::Ns => "ns",
            Unit::Us => "us",
            Unit::Ms => "ms",
        }
    }
}

pub fn parse_days_since_epoch(s: &str, format: Option<&str>) -> Option<i32> {
    let date = match format {
        None => NaiveDate::parse_from_str(s, ISO_DATE).ok()?,
        Some(fmt) => NaiveDate::parse_from_str(s, fmt)
            .or_else(|_| NaiveDateTime::parse_from_str(s, fmt).map(|dt| dt.date()))
            .ok()?,
    };
    let epoch = DateTime::UNIX_EPOCH.date_naive();
    i32::try_from((date - epoch).num_days()).ok()
}

pub fn parse_nanos_since_midnight(s: &str, format: Option<&str>) -> Option<i64> {
    let time = match format {
        None => ISO_TIME_FORMATS
            .iter()
            .find_map(|f| NaiveTime::parse_from_str(s, f).ok())?,
        Some(fmt) => NaiveTime::parse_from_str(s, fmt)
            .or_else(|_| NaiveDateTime::parse_from_str(s, fmt).map(|dt| dt.time()))
            .ok()?,
    };
    let since_midnight = time - NaiveTime::MIN;
    since_midnight.num_nanoseconds()
}

pub enum DatetimeKind {
    WallClock,
    Instant,
}

pub fn classify_datetime(s: &str) -> Option<DatetimeKind> {
    Some(match parse_datetime(s, None)? {
        ParsedDatetime::Instant(_) => DatetimeKind::Instant,
        ParsedDatetime::WallClock(_) => DatetimeKind::WallClock,
    })
}

pub fn format_date(days_since_epoch: i32) -> Option<String> {
    let date = DateTime::UNIX_EPOCH
        .date_naive()
        .checked_add_signed(chrono::TimeDelta::try_days(days_since_epoch.into())?)?;
    Some(date.format(ISO_DATE).to_string())
}

pub fn format_timestamp(
    count_in_unit: i64,
    unit: Unit,
    zone: Option<&chrono_tz::Tz>,
) -> Option<String> {
    let instant = match unit {
        Unit::Ns => DateTime::from_timestamp_nanos(count_in_unit),
        Unit::Us => DateTime::from_timestamp_micros(count_in_unit)?,
        Unit::Ms => DateTime::from_timestamp_millis(count_in_unit)?,
    };
    Some(match zone {
        None => instant
            .naive_utc()
            .format("%Y-%m-%dT%H:%M:%S%.f")
            .to_string(),
        Some(tz) => instant
            .with_timezone(tz)
            .format("%Y-%m-%dT%H:%M:%S%.f%:z")
            .to_string(),
    })
}

pub fn format_time(nanos_since_midnight: i64) -> String {
    let since_midnight = chrono::TimeDelta::nanoseconds(nanos_since_midnight);
    let time = NaiveTime::MIN.overflowing_add_signed(since_midnight).0;
    time.format("%H:%M:%S%.f").to_string()
}

pub fn format_duration(count_in_unit: i64, unit: Unit) -> String {
    format!("{count_in_unit}{}", unit.suffix())
}

enum ParsedDatetime {
    Instant(DateTime<Utc>),
    WallClock(NaiveDateTime),
}

fn parse_datetime(s: &str, format: Option<&str>) -> Option<ParsedDatetime> {
    let instant = |dt: DateTime<chrono::FixedOffset>| ParsedDatetime::Instant(dt.to_utc());
    if let Some(fmt) = format {
        return DateTime::parse_from_str(s, fmt)
            .map(instant)
            .or_else(|_| NaiveDateTime::parse_from_str(s, fmt).map(ParsedDatetime::WallClock))
            .or_else(|_| {
                NaiveDate::parse_from_str(s, fmt)
                    .map(|d| ParsedDatetime::WallClock(d.and_time(NaiveTime::MIN)))
            })
            .ok();
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(instant(dt));
    }
    if let Some(dt) = ISO_OFFSET_FORMATS
        .iter()
        .find_map(|f| DateTime::parse_from_str(s, f).ok())
    {
        return Some(instant(dt));
    }
    if let Some(dt) = ISO_WALL_CLOCK_FORMATS
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(s, f).ok())
    {
        return Some(ParsedDatetime::WallClock(dt));
    }
    let midnight = NaiveDate::parse_from_str(s, ISO_DATE)
        .ok()?
        .and_time(NaiveTime::MIN);
    Some(ParsedDatetime::WallClock(midnight))
}

pub fn parse_timestamp(
    s: &str,
    format: Option<&str>,
    unit: Unit,
    wall_clock_zone: Option<&chrono_tz::Tz>,
) -> Option<i64> {
    let instant = match (parse_datetime(s, format)?, wall_clock_zone) {
        (ParsedDatetime::Instant(dt), _) => dt,
        (ParsedDatetime::WallClock(dt), None) => dt.and_utc(),
        (ParsedDatetime::WallClock(dt), Some(tz)) => match tz.from_local_datetime(&dt) {
            LocalResult::Single(t) => t.to_utc(),
            LocalResult::Ambiguous(earlier, _later) => earlier.to_utc(),
            LocalResult::None => return None,
        },
    };
    let ns =
        i128::from(instant.timestamp()) * NS_PER_S + i128::from(instant.timestamp_subsec_nanos());
    i64::try_from(ns.div_euclid(unit.ns())).ok()
}

pub fn parse_duration(s: &str, unit: Unit) -> Option<i64> {
    if let Ok(count_in_unit) = s.parse::<i64>() {
        return Some(count_in_unit);
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
        total_ns = total_ns.checked_add(exact_decimal_product(num, unit_ns)?)?;
        rest = tail;
    }
    let value = total_ns / unit.ns();
    i64::try_from(if negative { -value } else { value }).ok()
}

fn exact_decimal_product(decimal: &str, factor: i128) -> Option<i128> {
    let (int, frac) = decimal.split_once('.').unwrap_or((decimal, ""));
    if int.is_empty() && frac.is_empty() {
        return None;
    }
    let int: i128 = if int.is_empty() { 0 } else { int.parse().ok()? };
    let mut product = int.checked_mul(factor)?;
    let mut place = factor;
    for digit in frac.chars() {
        place /= 10;
        product = product.checked_add(i128::from(digit.to_digit(10)?) * place)?;
    }
    Some(product)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_york() -> chrono_tz::Tz {
        "America/New_York".parse().unwrap()
    }

    #[test]
    fn pre_epoch_timestamps_round_toward_negative_infinity() {
        assert_eq!(
            parse_timestamp("1969-12-31T23:59:59.9995", None, Unit::Ms, None),
            Some(-1)
        );
    }

    #[test]
    fn bare_date_is_midnight() {
        assert_eq!(
            parse_timestamp("1970-01-02", None, Unit::Ms, None),
            Some(86_400_000)
        );
    }

    #[test]
    fn offset_wins_over_the_dtype_time_zone() {
        let ny = new_york();
        assert_eq!(
            parse_timestamp("1970-01-01 00:00:00Z", None, Unit::Ms, Some(&ny)),
            Some(0)
        );
        assert_eq!(
            parse_timestamp("1970-01-01 00:00:00", None, Unit::Ms, Some(&ny)),
            Some(5 * 3_600_000)
        );
    }

    #[test]
    fn offsets_parse_without_seconds_or_minutes() {
        let at_ten_utc = parse_timestamp("2026-01-01T10:00:00Z", None, Unit::Ms, None);
        assert_eq!(
            parse_timestamp("2026-01-01T10:00Z", None, Unit::Ms, None),
            at_ten_utc
        );
        assert_eq!(
            parse_timestamp("2026-01-01 12:00+02", None, Unit::Ms, None),
            at_ten_utc
        );
    }

    #[test]
    fn wall_clock_in_a_dst_gap_is_null() {
        assert_eq!(
            parse_timestamp("2026-03-08 02:30", None, Unit::Ms, Some(&new_york())),
            None
        );
    }

    #[test]
    fn ambiguous_wall_clock_takes_the_earlier_instant() {
        let earlier = parse_timestamp("2026-11-01 05:30:00Z", None, Unit::Ms, None);
        assert_eq!(
            parse_timestamp("2026-11-01 01:30", None, Unit::Ms, Some(&new_york())),
            earlier
        );
    }

    #[test]
    fn bare_integer_duration_is_already_in_the_target_unit() {
        assert_eq!(parse_duration("42", Unit::Ms), Some(42));
        assert_eq!(parse_duration("42s", Unit::Ms), Some(42_000));
    }

    #[test]
    fn fractional_durations_are_exact() {
        assert_eq!(parse_duration("0.3s", Unit::Ns), Some(300_000_000));
        assert_eq!(parse_duration("-1d0.5h", Unit::Ms), Some(-88_200_000));
    }

    #[test]
    fn fraction_that_tips_the_total_past_i128_is_null() {
        let whole_us = (i128::MAX / NS_PER_US).to_string();
        assert_eq!(parse_duration(&format!("{whole_us}.999us"), Unit::Ns), None);
    }
}
