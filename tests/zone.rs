//! Ports of gotz's zone_test.go (Unix-timestamp based).

use timezone_data::{load, names, parse, Error};

// Helper: collect a zone's types into a Vec for convenient assertions.
fn types_of(z: &timezone_data::Zone<'static>) -> Vec<timezone_data::ZoneType<'static>> {
    z.types().collect()
}

#[test]
fn load_utc() {
    let z = load("UTC").unwrap();
    assert_eq!(z.name(), "UTC");
    let types: Vec<_> = z.types().collect();
    assert_eq!(types.len(), 1);
    assert_eq!(types[0].abbrev, "UTC");
    assert_eq!(types[0].offset, 0);
    assert!(!types[0].is_dst);
}

#[test]
fn load_new_york() {
    let z = load("America/New_York").unwrap();
    assert_eq!(z.name(), "America/New_York");
    assert!(z.version() >= 2);

    let types = types_of(&z);
    assert!(types.len() >= 2);

    let mut found_est = false;
    let mut found_edt = false;
    for zt in &types {
        match zt.abbrev {
            "EST" => {
                found_est = true;
                assert_eq!(zt.offset, -5 * 3600);
                assert!(!zt.is_dst);
            }
            "EDT" => {
                found_edt = true;
                assert_eq!(zt.offset, -4 * 3600);
                assert!(zt.is_dst);
            }
            _ => {}
        }
    }
    assert!(found_est, "EST not found");
    assert!(found_edt, "EDT not found");

    assert!(z.transitions().count() >= 100);
    assert!(z.extend().is_some());
    assert!(!z.extend_raw().is_empty());
}

#[test]
fn load_tokyo() {
    let z = load("Asia/Tokyo").unwrap();
    let found_jst = z
        .types()
        .any(|zt| zt.abbrev == "JST" && zt.offset == 9 * 3600);
    assert!(found_jst, "JST not found");
}

#[test]
fn lookup() {
    let z = load("America/New_York").unwrap();
    // 2024-01-15 12:00:00 UTC — EST.
    assert_eq!(z.lookup(1_705_320_000).abbrev, "EST");
    // 2024-07-15 12:00:00 UTC — EDT.
    assert_eq!(z.lookup(1_721_044_800).abbrev, "EDT");
}

// 2024-01-01 00:00:00 UTC and yearly boundaries.
const Y2024: i64 = 1_704_067_200;
const Y2025: i64 = 1_735_689_600;
const Y2027: i64 = 1_798_761_600;

#[test]
fn transitions_for_range() {
    let z = load("America/New_York").unwrap();
    let trans: Vec<_> = z.transitions_for_range(Y2024, Y2025).collect();
    assert_eq!(trans.len(), 2);

    // First: std -> DST (March, EDT).
    assert_eq!(trans[0].zone_type.abbrev, "EDT");
    // Second: DST -> std (November, EST).
    assert_eq!(trans[1].zone_type.abbrev, "EST");
    assert!(trans[0].when < trans[1].when);
}

#[test]
fn transitions_for_range_multi_year() {
    let z = load("America/New_York").unwrap();
    let trans: Vec<_> = z.transitions_for_range(Y2024, Y2027).collect();
    assert_eq!(trans.len(), 6);
    for w in trans.windows(2) {
        assert!(
            w[1].when > w[0].when,
            "transitions must be strictly increasing"
        );
    }
}

#[test]
fn transitions_for_range_no_dst() {
    let z = load("Asia/Tokyo").unwrap();
    let trans: Vec<_> = z.transitions_for_range(Y2024, Y2025).collect();
    assert_eq!(trans.len(), 0);
}

#[test]
fn load_not_found() {
    assert_eq!(load("Fake/Timezone").unwrap_err(), Error::NotFound);
}

#[test]
fn parse_paris() {
    let data = {
        // Pull raw bytes back out via a known zone load's raw_data is the same path,
        // but to exercise parse() directly we load and re-parse its raw data.
        let z = load("Europe/Paris").unwrap();
        z.raw_data()
    };
    let z = parse("Europe/Paris", data).unwrap();
    assert_eq!(z.name(), "Europe/Paris");
    let found_cet = z.types().any(|zt| zt.abbrev == "CET" && zt.offset == 3600);
    assert!(found_cet, "CET not found");
}

#[test]
fn names_count() {
    let all: Vec<_> = names().collect();
    assert_eq!(all.len(), 600);
    assert!(all.contains(&"US/Eastern"));
    assert!(all.contains(&"iso3166.tab"));
    assert!(all.contains(&"zone1970.tab"));
}
