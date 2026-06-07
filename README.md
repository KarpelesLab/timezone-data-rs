# timezone-data

[![Tests](https://github.com/KarpelesLab/timezone-data-rs/actions/workflows/test.yml/badge.svg)](https://github.com/KarpelesLab/timezone-data-rs/actions/workflows/test.yml)
[![crates.io](https://img.shields.io/crates/v/timezone-data.svg)](https://crates.io/crates/timezone-data)
[![docs.rs](https://img.shields.io/docsrs/timezone-data)](https://docs.rs/timezone-data)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A `#![no_std]`, allocation-free Rust crate that exposes IANA timezone data most
libraries keep private: transitions, zone types, POSIX `TZ` rules, leap seconds,
and per-zone metadata.

Every zone from the [official IANA source](https://data.iana.org/time-zones/releases/)
is **pre-parsed at build time into static Rust objects** and embedded in the
crate, so there is no dependency on the host system's timezone files, nothing is
parsed at runtime, and there are **no external crate dependencies at all**. A
lookup is a binary search over `&'static` data.

This is a Rust port of the Go package [`gotz`](https://github.com/KarpelesLab/gotz).

## Highlights

- **`no_std` + no `alloc`.** Accessors return slices that point straight at
  `&'static` data. Nothing is heap-allocated.
- **Nothing parsed at runtime, no build script.** Each zone is materialised as
  static `ZoneType` / `Transition` arrays + a const POSIX rule; `load()` just
  binary-searches a static table.
- **Zero dependencies.**
- **Complete IANA database** embedded (598 zones, plus the `zone1970.tab` /
  `iso3166.tab` metadata tables).

## Usage

```rust
use timezone_data::load;

let z = load("America/New_York")?;

// Inspect zone types (EST, EDT, ...).
for zt in z.types() {
    println!("{}  offset={}  dst={}", zt.abbrev, zt.offset, zt.is_dst);
}

// Iterate historical transitions.
for t in z.transitions() {
    let zt = z.type_at(t.type_idx);
    println!("{} -> {}", t.when, zt.abbrev);
}

// Look up the active zone at a specific Unix timestamp.
let zt = z.lookup(1_700_000_000);
println!("zone: {} (UTC offset {})", zt.abbrev, zt.offset);

// Get the POSIX TZ rule for future transitions.
if let Some(rule) = z.extend() {
    let (start, end) = rule.transitions_for_year(2025).unwrap();
    println!("DST starts: {start}, ends: {end}");
}

// Country and coordinates metadata.
if let Some(m) = z.meta() {
    if let Some(c) = m.countries().next() {
        println!("country: {} ({})", c.name, c.code);
    }
    println!("location: {}, {}", m.lat, m.lon);
}
# Ok::<(), timezone_data::Error>(())
```

Times are expressed as `i64` Unix seconds — the crate is timezone-library
agnostic. If you need a `chrono`/`time` value, convert at the boundary.

## API overview

| Item | Description |
|------|-------------|
| `load(name) -> Result<Zone, Error>` | Load a zone by IANA name (`""`/`"UTC"` → UTC). |
| `load_insensitive(name)` | Load with case-insensitive fallback. |
| `names()` | Iterate every IANA zone name. |
| `Zone::types()` / `type_at(i)` | Local time types (abbrev, offset, DST flag). |
| `Zone::transitions()` | Stored transition records. |
| `Zone::leap_seconds()` | Leap-second records. |
| `Zone::lookup(unix)` | Zone type in effect at an instant. |
| `Zone::transitions_for_range(start, end)` | Stored + POSIX-generated transitions in `[start, end)`. |
| `Zone::extend()` / `extend_raw()` | Parsed / raw POSIX `TZ` footer. |
| `Zone::meta()` | Country + coordinate metadata. |
| `parse_posix_tz(s)` | Parse a POSIX `TZ` string directly. |

## Updating the embedded data

`zoneinfo.zip` is committed to the repository purely as the data source for the
generator; it is **not** part of the published crate. The generated table
(`src/generated.rs`) and the metadata tables (`src/zone1970.tab`,
`src/iso3166.tab`) are committed and are what gets shipped.

To refresh from a new IANA release:

1. Compile the tzdata with `zic` and repackage it as a **STORE** (uncompressed)
   zip — the generator does not implement inflate. The original `gotz`
   repository's `update.sh` / `mkzip.go` produce a compatible archive.
2. Replace `zoneinfo.zip` in this crate's root.
3. Run `cargo run --manifest-path xtask/Cargo.toml` to regenerate
   `src/generated.rs` and the `.tab` files, then commit the result.

CI re-runs the generator and fails if the committed output is stale, so a
release always ships the current data.

## License

MIT. Timezone data is in the public domain (IANA).
