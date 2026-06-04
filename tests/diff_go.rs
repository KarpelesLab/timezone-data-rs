//! Differential test against Go's standard library (ground truth).
//! Compares Zone::lookup abbrev + offset for several zones/timestamps.

#[test]
fn matches_go_stdlib() {
    let csv = include_str!("go_expected.csv");
    let mut checked = 0;
    for line in csv.lines() {
        if line.is_empty() {
            continue;
        }
        let mut f = line.split(',');
        let zone = f.next().unwrap();
        let unix: i64 = f.next().unwrap().parse().unwrap();
        let want_abbrev = f.next().unwrap();
        let want_offset: i32 = f.next().unwrap().parse().unwrap();

        let z = timezone_data::load(zone).unwrap();
        let zt = z.lookup(unix);
        assert_eq!(zt.abbrev, want_abbrev, "{zone} @ {unix}: abbrev");
        assert_eq!(zt.offset, want_offset, "{zone} @ {unix}: offset");
        checked += 1;
    }
    assert_eq!(checked, 42);
}
