//! Parser and evaluator for POSIX-style `TZ` strings.
//!
//! A TZif file may carry a trailing POSIX TZ rule (the "footer") describing how
//! daylight-saving transitions continue past the last stored transition. This
//! module parses such strings (e.g. `EST5EDT,M3.2.0,M11.1.0`) and computes the
//! offset in effect at any instant, all without allocating.

use core::fmt;

use crate::error::Error;

/// Identifies the type of transition rule in a POSIX TZ string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleKind {
    /// `Jn` format: Julian day (1-365); February 29 is never counted.
    Julian,
    /// `n` format: zero-based day of year (0-365); February 29 is counted.
    DayOfYear,
    /// `Mm.w.d` format: month, week, and day-of-week.
    MonthWeekDay,
}

/// Specifies when a DST transition occurs within a year.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionRule {
    /// Which interpretation [`day`](Self::day) takes.
    pub kind: RuleKind,
    /// Julian day (1-365), day-of-year (0-365), or day-of-week (0 = Sunday).
    pub day: i32,
    /// Week of month (1-5); only meaningful for [`RuleKind::MonthWeekDay`].
    pub week: i32,
    /// Month (1-12); only meaningful for [`RuleKind::MonthWeekDay`].
    pub mon: i32,
    /// Seconds after midnight (default 7200 = 02:00:00).
    pub time: i32,
}

/// A parsed POSIX-style `TZ` string, e.g. `EST5EDT,M3.2.0,M11.1.0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PosixTz<'a> {
    /// Standard time abbreviation.
    pub std_abbrev: &'a str,
    /// Standard time UTC offset in seconds (positive = east of UTC).
    pub std_offset: i32,
    /// DST abbreviation; empty if the rule defines no daylight saving time.
    pub dst_abbrev: &'a str,
    /// DST UTC offset in seconds (positive = east of UTC).
    pub dst_offset: i32,
    /// Rule for the standard → DST transition.
    pub start: TransitionRule,
    /// Rule for the DST → standard transition.
    pub end: TransitionRule,
}

impl<'a> PosixTz<'a> {
    /// Reports whether the rule defines daylight saving time.
    pub fn has_dst(&self) -> bool {
        !self.dst_abbrev.is_empty()
    }

    /// Returns the abbreviation, UTC offset, and DST flag in effect at the given
    /// Unix timestamp according to this rule.
    pub fn lookup(&self, unix: i64) -> (&'a str, i32, bool) {
        if !self.has_dst() {
            return (self.std_abbrev, self.std_offset, false);
        }

        let (year, yday, sec) = unix_to_yday_sec(unix);
        let year_sec = yday * 86400 + sec;

        let start_sec = rule_to_year_sec(self.start, year, self.std_offset);
        let end_sec = rule_to_year_sec(self.end, year, self.dst_offset);

        let in_dst = if start_sec < end_sec {
            // Northern hemisphere: DST between start and end.
            year_sec >= start_sec && year_sec < end_sec
        } else {
            // Southern hemisphere: DST outside [end, start).
            year_sec >= start_sec || year_sec < end_sec
        };

        if in_dst {
            (self.dst_abbrev, self.dst_offset, true)
        } else {
            (self.std_abbrev, self.std_offset, false)
        }
    }

    /// Returns the DST start and end times as Unix timestamps for `year`,
    /// or `None` if the rule defines no DST.
    pub fn transitions_for_year(&self, year: i32) -> Option<(i64, i64)> {
        if !self.has_dst() {
            return None;
        }
        let year_start = year_to_unix(year);
        let start_sec = rule_to_year_sec(self.start, year, self.std_offset);
        let end_sec = rule_to_year_sec(self.end, year, self.dst_offset);
        Some((year_start + start_sec as i64, year_start + end_sec as i64))
    }
}

impl fmt::Display for PosixTz<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_name(f, self.std_abbrev)?;
        write_offset(f, -self.std_offset)?;

        if !self.has_dst() {
            return Ok(());
        }

        write_name(f, self.dst_abbrev)?;
        if self.dst_offset != self.std_offset + 3600 {
            write_offset(f, -self.dst_offset)?;
        }

        f.write_str(",")?;
        write_rule(f, self.start)?;
        f.write_str(",")?;
        write_rule(f, self.end)
    }
}

/// Parses a POSIX-style `TZ` string.
pub fn parse_posix_tz(s: &str) -> Result<PosixTz<'_>, Error> {
    // Standard time name.
    let (std_abbrev, rest) = parse_tz_name(s)?;
    if std_abbrev.is_empty() {
        return Err(Error::BadPosixTz("empty standard timezone name"));
    }

    // Standard time offset. POSIX offsets are positive *west* of UTC (opposite
    // of ISO), so we negate to store seconds east of UTC.
    let (off, rest) = parse_tz_offset(rest)?;
    let std_offset = -off;

    let mut p = PosixTz {
        std_abbrev,
        std_offset,
        dst_abbrev: "",
        dst_offset: 0,
        start: DEFAULT_RULE,
        end: DEFAULT_RULE,
    };

    if rest.is_empty() {
        return Ok(p); // No DST.
    }

    // DST name.
    let (dst_abbrev, rest) = parse_tz_name(rest)?;
    if dst_abbrev.is_empty() {
        return Err(Error::BadPosixTz("empty DST timezone name"));
    }
    p.dst_abbrev = dst_abbrev;

    // Optional DST offset (default: std offset + 1 hour).
    let rest = if !rest.is_empty() && !rest.starts_with(',') {
        let (off, rest) = parse_tz_offset(rest)?;
        p.dst_offset = -off;
        rest
    } else {
        p.dst_offset = p.std_offset + 3600;
        rest
    };

    // Transition rules.
    if rest.is_empty() {
        // Default US rules: M3.2.0,M11.1.0
        p.start = TransitionRule {
            kind: RuleKind::MonthWeekDay,
            mon: 3,
            week: 2,
            day: 0,
            time: 7200,
        };
        p.end = TransitionRule {
            kind: RuleKind::MonthWeekDay,
            mon: 11,
            week: 1,
            day: 0,
            time: 7200,
        };
        return Ok(p);
    }

    let rest = rest
        .strip_prefix(',')
        .ok_or(Error::BadPosixTz("expected ',' before transition rules"))?;

    let (start, rest) = parse_tz_rule(rest)?;
    p.start = start;

    let rest = rest
        .strip_prefix(',')
        .ok_or(Error::BadPosixTz("expected ',' between transition rules"))?;

    let (end, _rest) = parse_tz_rule(rest)?;
    p.end = end;

    Ok(p)
}

const DEFAULT_RULE: TransitionRule = TransitionRule {
    kind: RuleKind::MonthWeekDay,
    day: 0,
    week: 0,
    mon: 0,
    time: 7200,
};

// --- Parsing helpers ---

fn parse_tz_name(s: &str) -> Result<(&str, &str), Error> {
    if s.is_empty() {
        return Ok(("", ""));
    }
    let b = s.as_bytes();
    if b[0] == b'<' {
        // Quoted name: <...>
        let end = s
            .find('>')
            .ok_or(Error::BadPosixTz("unterminated '<' in TZ name"))?;
        return Ok((&s[1..end], &s[end + 1..]));
    }
    // Unquoted: letters only.
    let mut i = 0;
    while i < b.len() && is_alpha(b[i]) {
        i += 1;
    }
    Ok((&s[..i], &s[i..]))
}

fn parse_tz_offset(s: &str) -> Result<(i32, &str), Error> {
    if s.is_empty() {
        return Err(Error::BadPosixTz("expected offset"));
    }
    let mut rest = s;
    let mut neg = false;
    if let Some(r) = rest.strip_prefix('-') {
        neg = true;
        rest = r;
    } else if let Some(r) = rest.strip_prefix('+') {
        rest = r;
    }

    let (hours, mut rest) = parse_tz_num(rest, 0, 167)?;
    let mut mins = 0;
    let mut secs = 0;
    if let Some(r) = rest.strip_prefix(':') {
        let (m, r) = parse_tz_num(r, 0, 59)?;
        mins = m;
        rest = r;
        if let Some(r) = rest.strip_prefix(':') {
            let (sx, r) = parse_tz_num(r, 0, 59)?;
            secs = sx;
            rest = r;
        }
    }

    let mut offset = hours * 3600 + mins * 60 + secs;
    if neg {
        offset = -offset;
    }
    Ok((offset, rest))
}

fn parse_tz_rule(s: &str) -> Result<(TransitionRule, &str), Error> {
    if s.is_empty() {
        return Err(Error::BadPosixTz("empty transition rule"));
    }
    let mut r = TransitionRule {
        kind: RuleKind::DayOfYear,
        day: 0,
        week: 0,
        mon: 0,
        time: 7200,
    };

    let b = s.as_bytes();
    let mut rest;
    if b[0] == b'M' {
        // Mm.w.d
        r.kind = RuleKind::MonthWeekDay;
        let (mon, after) = parse_tz_num(&s[1..], 1, 12)?;
        r.mon = mon;
        rest = after
            .strip_prefix('.')
            .ok_or(Error::BadPosixTz("expected '.' after month in rule"))?;
        let (week, after) = parse_tz_num(rest, 1, 5)?;
        r.week = week;
        rest = after
            .strip_prefix('.')
            .ok_or(Error::BadPosixTz("expected '.' after week in rule"))?;
        let (day, after) = parse_tz_num(rest, 0, 6)?;
        r.day = day;
        rest = after;
    } else if b[0] == b'J' {
        // Jn (1-365, no leap day)
        r.kind = RuleKind::Julian;
        let (day, after) = parse_tz_num(&s[1..], 1, 365)?;
        r.day = day;
        rest = after;
    } else {
        // n (0-365, with leap day)
        r.kind = RuleKind::DayOfYear;
        let (day, after) = parse_tz_num(s, 0, 365)?;
        r.day = day;
        rest = after;
    }

    // Optional time component: /time
    if let Some(after) = rest.strip_prefix('/') {
        let (off, after) = parse_tz_offset(after)?;
        r.time = off;
        rest = after;
    }

    Ok((r, rest))
}

fn parse_tz_num(s: &str, min: i32, max: i32) -> Result<(i32, &str), Error> {
    let b = s.as_bytes();
    if b.is_empty() || !is_digit(b[0]) {
        return Err(Error::BadPosixTz("expected digit"));
    }
    let mut n: i32 = 0;
    let mut i = 0;
    while i < b.len() && is_digit(b[i]) {
        n = n * 10 + (b[i] - b'0') as i32;
        i += 1;
    }
    if n < min || n > max {
        return Err(Error::BadPosixTz("number out of range"));
    }
    Ok((n, &s[i..]))
}

fn is_alpha(c: u8) -> bool {
    c.is_ascii_alphabetic()
}

fn is_digit(c: u8) -> bool {
    c.is_ascii_digit()
}

// --- Time computation helpers ---

fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

const DAYS_IN_MONTH: [i32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// Returns the Unix timestamp for January 1 00:00:00 UTC of `year`.
fn year_to_unix(year: i32) -> i64 {
    let y = year as i64 - 1970;
    let mut days = 365 * y;

    // Add leap days. Count leap years in [1970, year).
    if year > 1970 {
        days += (y + 1) / 4;
        days -= (y + 69) / 100;
        days += (y + 369) / 400;
    } else if year < 1970 {
        days += (y - 2) / 4;
        days -= (y - 30) / 100;
        days += (y - 30) / 400;
    }

    days * 86400
}

/// Returns the calendar year (UTC) containing the given Unix timestamp.
pub(crate) fn year_of(unix: i64) -> i32 {
    unix_to_yday_sec(unix).0
}

/// Converts a Unix timestamp to (year, day-of-year [0-based], second-of-day).
fn unix_to_yday_sec(unix: i64) -> (i32, i32, i32) {
    let mut unix = unix;
    let mut sec = (unix % 86400) as i32;
    if sec < 0 {
        sec += 86400;
        unix -= 86400;
    }
    let days = (unix / 86400) as i32;

    // Compute year from days since epoch, adjusting from an estimate.
    let mut year = 1970 + days / 365;
    loop {
        let year_start = (year_to_unix(year) / 86400) as i32;
        if year_start <= days {
            let mut year_end = year_start + 365;
            if is_leap_year(year) {
                year_end += 1;
            }
            if days < year_end {
                return (year, days - year_start, sec);
            }
            year += 1;
        } else {
            year -= 1;
        }
    }
}

/// Converts a [`TransitionRule`] to seconds since the start of the year in UTC
/// (wall-clock seconds adjusted by `offset`).
fn rule_to_year_sec(r: TransitionRule, year: i32, offset: i32) -> i32 {
    let leap = is_leap_year(year);

    let yday = match r.kind {
        RuleKind::Julian => {
            // Jn: 1-365, Feb 29 is never counted.
            let mut d = r.day - 1;
            if leap && d >= 59 {
                d += 1; // after Feb 28
            }
            d
        }
        RuleKind::DayOfYear => {
            // n: 0-365.
            r.day
        }
        RuleKind::MonthWeekDay => {
            // Mm.w.d: month, week (1-5), day-of-week (0 = Sunday).
            let m = (r.mon - 1) as usize; // 0-indexed

            // Day of year for the 1st of the month.
            let mut first_yday = 0;
            for (i, &dim) in DAYS_IN_MONTH.iter().enumerate().take(m) {
                first_yday += dim;
                if i == 1 && leap {
                    first_yday += 1;
                }
            }

            // Day of week for Jan 1 of this year (0 = Sunday).
            // 1970-01-01 was a Thursday (4).
            let jan1_wday = (((year_to_unix(year) / 86400) % 7 + 4 + 7 * 53) % 7) as i32;

            // Day of week for the 1st of the month.
            let first_wday = (jan1_wday + first_yday) % 7;

            // Days until the target day-of-week from the 1st.
            let days_until = (r.day - first_wday + 7) % 7;

            // Advance to the target week.
            let mut y = first_yday + days_until + (r.week - 1) * 7;

            // week=5 means "last in month". Clamp to the month's length.
            let mut month_days = DAYS_IN_MONTH[m];
            if m == 1 && leap {
                month_days += 1;
            }
            while y - first_yday >= month_days {
                y -= 7;
            }
            y
        }
    };

    // Seconds from the start of the year, plus the transition time, then adjust
    // from wall time to UTC.
    yday * 86400 + r.time - offset
}

// --- String formatting helpers ---

fn write_name(f: &mut fmt::Formatter<'_>, name: &str) -> fmt::Result {
    let needs_quote = name.bytes().any(|c| !is_alpha(c));
    if needs_quote {
        write!(f, "<{name}>")
    } else {
        f.write_str(name)
    }
}

fn write_offset(f: &mut fmt::Formatter<'_>, posix_off: i32) -> fmt::Result {
    let mut v = posix_off;
    if v < 0 {
        f.write_str("-")?;
        v = -v;
    }
    let hours = v / 3600;
    let mins = (v % 3600) / 60;
    let secs = v % 60;

    write!(f, "{hours}")?;
    if mins != 0 || secs != 0 {
        write!(f, ":{mins:02}")?;
        if secs != 0 {
            write!(f, ":{secs:02}")?;
        }
    }
    Ok(())
}

fn write_rule(f: &mut fmt::Formatter<'_>, r: TransitionRule) -> fmt::Result {
    match r.kind {
        RuleKind::Julian => write!(f, "J{}", r.day)?,
        RuleKind::DayOfYear => write!(f, "{}", r.day)?,
        RuleKind::MonthWeekDay => write!(f, "M{}.{}.{}", r.mon, r.week, r.day)?,
    }
    if r.time != 7200 {
        f.write_str("/")?;
        write_offset(f, r.time)?;
    }
    Ok(())
}
