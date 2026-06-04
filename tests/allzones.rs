#[test]
fn load_all_zones() {
    let mut count = 0;
    let mut with_extend = 0;
    for name in timezone_data::names() {
        if name == "iso3166.tab" || name == "zone1970.tab" {
            continue;
        }
        let z = timezone_data::load(name).unwrap_or_else(|e| panic!("load {name}: {e}"));
        // Exercise every accessor to force full decode.
        let _ = z.types().count();
        let _ = z.transitions().count();
        let _ = z.leap_seconds().count();
        let _ = z.lookup(1_700_000_000);
        let _ = z
            .transitions_for_range(1_704_067_200, 1_798_761_600)
            .count();
        if z.extend().is_some() {
            with_extend += 1;
        }
        count += 1;
    }
    eprintln!("parsed {count} zones, {with_extend} with POSIX extend");
    assert_eq!(count, 598);
}
