# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.2.1](https://github.com/KarpelesLab/timezone-data-rs/compare/v0.2.0...v0.2.1) - 2026-06-07

### Other

- add CHANGELOG with 0.1.0 and 0.2.0 history
- make the root a single crate again (xtask is its own workspace)

## [0.2.0] - 2026-06-07

The embedded data is now **pre-parsed at build time into static Rust objects**
instead of being parsed from an embedded archive at runtime. A `load()` is a
binary search over `&'static` data — no parsing, no allocation, and no build
script. This involved several breaking API changes.

### Changed

- **Breaking:** `Zone`, `ZoneType`, and `RangeTransition` no longer take a
  lifetime parameter — their data is `'static`.
- **Breaking:** `Zone::types`, `Zone::transitions`, and `Zone::leap_seconds`
  now return `&'static [_]` slices instead of iterators.

### Removed

- **Breaking:** `parse()` (the runtime TZif byte parser) and `Zone::raw_data()`
  — the crate no longer parses arbitrary TZif bytes or retains raw bytes.
- **Breaking:** the `Error::BadData` and `Error::BadZip` variants, which
  corresponded to failure modes that no longer exist.

### Added

- Expanded test coverage: `load_insensitive`, pre-first-transition and UTC
  lookups, southern-hemisphere transition ordering, POSIX Julian (`Jn`) and
  day-of-year (`n`) rules, parse-error paths, and the `Error` trait impls.

## [0.1.0] - 2026-06-04

Initial release: a `#![no_std]`, no-alloc IANA timezone-data crate with the full
tz database embedded, exposing transitions, zone types, POSIX `TZ` rules, leap
seconds, and per-zone metadata. Rust port of
[`gotz`](https://github.com/KarpelesLab/gotz).
