//! Ports of gotz's zone_test.go (Unix-timestamp based).

use timezone_data::{load, names, Error};

#[test]
fn load_utc() {
    let z = load("UTC").unwrap();
    assert_eq!(z.name(), "UTC");
    let types = z.types();
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

    let types = z.types();
    assert!(types.len() >= 2);
    assert_eq!(z.type_count(), types.len());

    let mut found_est = false;
    let mut found_edt = false;
    for zt in types {
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

    assert!(z.transitions().len() >= 100);
    assert!(z.extend().is_some());
    assert!(!z.extend_raw().is_empty());
}

#[test]
fn load_tokyo() {
    let z = load("Asia/Tokyo").unwrap();
    let found_jst = z
        .types()
        .iter()
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
fn load_empty_is_utc() {
    assert_eq!(load("").unwrap().name(), "UTC");
}

#[test]
fn load_insensitive_cases() {
    use timezone_data::load_insensitive;
    // Exact match still works.
    assert_eq!(
        load_insensitive("America/New_York").unwrap().name(),
        "America/New_York"
    );
    // Case-insensitive fallback.
    assert_eq!(
        load_insensitive("america/new_york").unwrap().name(),
        "America/New_York"
    );
    assert_eq!(load_insensitive("utc").unwrap().name(), "UTC");
    // Still fails for genuinely unknown zones.
    assert_eq!(
        load_insensitive("Nope/Nowhere").unwrap_err(),
        Error::NotFound
    );
}

#[test]
fn lookup_before_first_transition() {
    // A timestamp far before any stored transition falls back to the first
    // non-DST type (LMT for New York).
    let z = load("America/New_York").unwrap();
    let early = z.lookup(-5_000_000_000); // ~1812
    assert_eq!(early.abbrev, "LMT");
    assert!(!early.is_dst);
}

#[test]
fn lookup_utc_has_no_transitions() {
    // UTC has a single type and no transitions — exercises the empty-transitions
    // branch of lookup.
    let z = load("UTC").unwrap();
    assert!(z.transitions().is_empty());
    assert_eq!(z.lookup(0).abbrev, "UTC");
    assert_eq!(z.lookup(1_700_000_000).offset, 0);
}

#[test]
fn transitions_for_range_southern_hemisphere() {
    // Sydney's DST runs Oct->Apr, so within a calendar year the std (April) and
    // DST (October) transitions arrive in the opposite order from the northern
    // hemisphere. This exercises the chronological-ordering branch.
    let z = load("Australia/Sydney").unwrap();
    let trans: Vec<_> = z.transitions_for_range(Y2024, Y2025).collect();
    assert_eq!(trans.len(), 2);
    // April: DST -> std (AEST).
    assert_eq!(trans[0].zone_type.abbrev, "AEST");
    assert!(!trans[0].zone_type.is_dst);
    // October: std -> DST (AEDT).
    assert_eq!(trans[1].zone_type.abbrev, "AEDT");
    assert!(trans[1].zone_type.is_dst);
    assert!(trans[0].when < trans[1].when, "must be chronological");
}

#[test]
fn load_paris() {
    let z = load("Europe/Paris").unwrap();
    assert_eq!(z.name(), "Europe/Paris");
    let found_cet = z
        .types()
        .iter()
        .any(|zt| zt.abbrev == "CET" && zt.offset == 3600);
    assert!(found_cet, "CET not found");
}

#[test]
fn names_are_zones_only() {
    let all: Vec<_> = names().collect();
    assert_eq!(all.len(), 598);
    assert!(all.contains(&"US/Eastern"));
    // The metadata tables are not loadable zones and are excluded from names().
    assert!(!all.contains(&"iso3166.tab"));
    assert!(!all.contains(&"zone1970.tab"));
}
