# timezone-data

[![Tests](https://github.com/KarpelesLab/timezone-data-rs/actions/workflows/test.yml/badge.svg)](https://github.com/KarpelesLab/timezone-data-rs/actions/workflows/test.yml)
[![crates.io](https://img.shields.io/crates/v/timezone-data.svg)](https://crates.io/crates/timezone-data)
[![docs.rs](https://img.shields.io/docsrs/timezone-data)](https://docs.rs/timezone-data)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A `#![no_std]`, allocation-free Rust crate that parses IANA TZif timezone files
and exposes the raw timezone data most libraries keep private: transitions, zone
types, POSIX `TZ` rules, leap seconds, and per-zone metadata.

Timezone data is compiled from the [official IANA source](https://data.iana.org/time-zones/releases/)
and embedded in the crate as individual TZif files, so there is no dependency on
the host system's timezone files, no archive to parse at runtime, and **no
external crate dependencies at all**. The files are extracted ahead of time and
committed, so building the crate is just a plain compile — there is no build
script.

This is a Rust port of the Go package [`gotz`](https://github.com/KarpelesLab/gotz).

## Highlights

- **`no_std` + no `alloc`.** Everything borrows into the embedded `&'static`
  bytes and decodes lazily through iterators. Nothing is heap-allocated.
- **No runtime archive parsing and no build script.** Each TZif file is embedded
  directly; a zone lookup is a binary search over a static table.
- **Zero dependencies.**
- **Complete IANA database** embedded (600 entries, including the `zone1970.tab`
  and `iso3166.tab` metadata tables).

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
agnostic. If you need a `chrono`/`time` value, convert at the boundary;
[`Zone::raw_data`] also returns the original TZif bytes for feeding into another
parser.

## API overview

| Item | Description |
|------|-------------|
| `load(name) -> Result<Zone, Error>` | Load a zone by IANA name (`""`/`"UTC"` → UTC). |
| `load_insensitive(name)` | Load with case-insensitive fallback. |
| `parse(name, data)` | Parse arbitrary TZif bytes. |
| `names()` | Iterate every embedded entry name. |
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
generator; it is **not** part of the published crate. The extracted files
(`src/zoneinfo/`) and the lookup table (`src/generated.rs`) are committed and
are what gets shipped, so consumers compile a static table with no extraction.

To refresh from a new IANA release:

1. Compile the tzdata with `zic` and repackage it as a **STORE** (uncompressed)
   zip — the generator does not implement inflate. The original `gotz`
   repository's `update.sh` / `mkzip.go` produce a compatible archive.
2. Replace `zoneinfo.zip` in this crate's root.
3. Run `cargo run -p xtask` to regenerate `src/zoneinfo/` and `src/generated.rs`,
   then commit the result.

CI re-runs the generator and fails if the committed output is stale, so a
release always ships the current data.

## License

MIT. Timezone data is in the public domain (IANA).
