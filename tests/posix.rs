//! Ports of gotz's posixtz_test.go.

use timezone_data::{parse_posix_tz, RuleKind};

#[test]
fn parse_simple() {
    let p = parse_posix_tz("EST5EDT,M3.2.0,M11.1.0").unwrap();
    assert_eq!(p.std_abbrev, "EST");
    assert_eq!(p.std_offset, -5 * 3600);
    assert_eq!(p.dst_abbrev, "EDT");
    assert_eq!(p.dst_offset, -4 * 3600);
    assert!(p.has_dst());
    assert_eq!(p.start.kind, RuleKind::MonthWeekDay);
    assert_eq!((p.start.mon, p.start.week, p.start.day), (3, 2, 0));
    assert_eq!(p.end.kind, RuleKind::MonthWeekDay);
    assert_eq!((p.end.mon, p.end.week, p.end.day), (11, 1, 0));
}

#[test]
fn parse_no_dst() {
    let p = parse_posix_tz("JST-9").unwrap();
    assert_eq!(p.std_abbrev, "JST");
    assert_eq!(p.std_offset, 9 * 3600);
    assert!(!p.has_dst());
}

#[test]
fn parse_quoted() {
    let p = parse_posix_tz("<-05>5<-04>,M3.2.0,M11.1.0").unwrap();
    assert_eq!(p.std_abbrev, "-05");
    assert_eq!(p.dst_abbrev, "-04");
}

#[test]
fn parse_with_time() {
    let p = parse_posix_tz("CET-1CEST,M3.5.0/2,M10.5.0/3").unwrap();
    assert_eq!(p.std_abbrev, "CET");
    assert_eq!(p.std_offset, 3600);
    assert_eq!(p.dst_abbrev, "CEST");
    assert_eq!(p.start.time, 7200);
    assert_eq!(p.end.time, 10800);
}

#[test]
fn display_round_trip() {
    for s in [
        "EST5EDT,M3.2.0,M11.1.0",
        "JST-9",
        "CET-1CEST,M3.5.0,M10.5.0/3",
    ] {
        let p = parse_posix_tz(s).unwrap();
        assert_eq!(p.to_string(), s, "round-trip of {s:?}");
    }
}

#[test]
fn lookup() {
    let p = parse_posix_tz("EST5EDT,M3.2.0,M11.1.0").unwrap();
    // 2024-01-15 12:00:00 UTC — EST.
    assert_eq!(p.lookup(1_705_320_000), ("EST", -18000, false));
    // 2024-07-15 12:00:00 UTC — EDT.
    assert_eq!(p.lookup(1_721_044_800), ("EDT", -14400, true));
}

#[test]
fn transitions_for_year() {
    let p = parse_posix_tz("EST5EDT,M3.2.0,M11.1.0").unwrap();
    let (start, end) = p.transitions_for_year(2024).unwrap();
    // 2024 DST starts March 10 07:00:00 UTC; ends November 3 06:00:00 UTC.
    assert_eq!(start, 1_710_054_000);
    assert_eq!(end, 1_730_613_600);
}

#[test]
fn no_dst_lookup() {
    let p = parse_posix_tz("JST-9").unwrap();
    assert_eq!(p.lookup(1_705_320_000), ("JST", 9 * 3600, false));
    assert!(p.transitions_for_year(2024).is_none());
}
