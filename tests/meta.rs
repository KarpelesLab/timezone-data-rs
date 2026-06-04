//! Ports of gotz's zonemeta_test.go.

use timezone_data::{load, parse_iso6709};

#[test]
fn meta_new_york() {
    let z = load("America/New_York").unwrap();
    let m = z.meta().expect("meta is None");
    let countries: Vec<_> = m.countries().collect();
    assert!(!countries.is_empty());
    assert_eq!(countries[0].code, "US");
    assert!(!countries[0].name.is_empty());
    assert!((m.lat - 40.7142).abs() < 0.5, "lat = {}", m.lat);
    assert!((m.lon - (-74.0060)).abs() < 0.5, "lon = {}", m.lon);
}

#[test]
fn meta_tokyo() {
    let z = load("Asia/Tokyo").unwrap();
    let m = z.meta().expect("meta is None");
    assert_eq!(m.countries().next().unwrap().code, "JP");
    assert!(m.lat > 35.0 && m.lat < 36.0, "lat = {}", m.lat);
}

#[test]
fn meta_multiple_countries() {
    // Asia/Dubai covers AE,OM,RE,SC,TF.
    let z = load("Asia/Dubai").unwrap();
    let m = z.meta().expect("meta is None");
    let countries: Vec<_> = m.countries().collect();
    assert!(countries.len() >= 2, "len = {}", countries.len());
    assert_eq!(countries[0].code, "AE");
}

#[test]
fn meta_utc() {
    let z = load("UTC").unwrap();
    // UTC has no zone1970.tab entry.
    assert!(z.meta().is_none());
}

#[test]
fn parse_iso6709_cases() {
    let cases = [
        ("+4030-07400", 40.5, -74.0),
        ("+3431+06912", 34.5167, 69.2),
        ("+352439+1394744", 35.4108, 139.7956),
    ];
    for (s, want_lat, want_lon) in cases {
        let (lat, lon) = parse_iso6709(s);
        assert!((lat - want_lat).abs() < 0.01, "{s}: lat = {lat}");
        assert!((lon - want_lon).abs() < 0.01, "{s}: lon = {lon}");
    }
}
