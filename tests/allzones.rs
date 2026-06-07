//! Exercise every embedded zone to ensure the generated data is well-formed.

#[test]
fn load_all_zones() {
    let mut count = 0;
    let mut with_extend = 0;
    for name in timezone_data::names() {
        let z = timezone_data::load(name).unwrap_or_else(|e| panic!("load {name}: {e}"));
        // Touch every accessor.
        let _ = z.types().len();
        let _ = z.transitions().len();
        let _ = z.leap_seconds().len();
        let _ = z.lookup(1_700_000_000);
        let _ = z
            .transitions_for_range(1_704_067_200, 1_798_761_600)
            .count();
        if z.extend().is_some() {
            with_extend += 1;
        }
        count += 1;
    }
    eprintln!("exercised {count} zones, {with_extend} with POSIX extend");
    assert_eq!(count, 598);
}
