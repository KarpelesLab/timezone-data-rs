//! `timezone-data` provides direct, allocation-free access to IANA timezone
//! data, exposing the transitions, zone types, POSIX TZ rules, leap seconds,
//! and metadata that most timezone libraries keep private.
//!
//! Timezone data is compiled from the official IANA source and embedded in the
//! crate as an uncompressed zip archive, so there is no dependency on the host
//! system's timezone files. The crate is `#![no_std]` and never allocates:
//! every accessor borrows into the embedded bytes and decodes records lazily.
//!
//! # Example
//!
//! ```
//! let z = timezone_data::load("America/New_York").unwrap();
//!
//! // Inspect zone types (EST, EDT, ...).
//! for zt in z.types() {
//!     // zt.abbrev, zt.offset, zt.is_dst
//!     let _ = zt;
//! }
//!
//! // Look up the active zone at a specific Unix timestamp.
//! let zt = z.lookup(1_700_000_000);
//! assert_eq!(zt.abbrev, "EST");
//!
//! // Compute future transitions from the POSIX TZ rule.
//! if let Some(rule) = z.extend() {
//!     let (start, end) = rule.transitions_for_year(2025).unwrap();
//!     assert!(start < end);
//! }
//! ```
#![no_std]
#![forbid(unsafe_code)]

mod error;
mod meta;
mod parse;
mod posix;
mod zipstore;

pub use error::Error;
pub use meta::{meta, parse_iso6709, Country, ZoneMeta};
pub use parse::{parse, LeapSecond, RangeTransition, Transition, Zone, ZoneType};
pub use posix::{parse_posix_tz, PosixTz, RuleKind, TransitionRule};

/// The embedded IANA timezone database (an uncompressed zip archive).
pub(crate) static ZONEINFO_ZIP: &[u8] = include_bytes!("../zoneinfo.zip");

/// Loads a [`Zone`] by IANA timezone name from the embedded database.
///
/// An empty name or `"UTC"` resolves to the `UTC` zone.
pub fn load(name: &str) -> Result<Zone<'static>, Error> {
    let query = if name.is_empty() { "UTC" } else { name };
    let (canonical, data) = zipstore::find_named(ZONEINFO_ZIP, query)?;
    parse(canonical, data)
}

/// Loads a [`Zone`] by name, falling back to case-insensitive matching.
pub fn load_insensitive(name: &str) -> Result<Zone<'static>, Error> {
    if let Ok(z) = load(name) {
        return Ok(z);
    }
    for canonical in names() {
        if canonical.eq_ignore_ascii_case(name) {
            return load(canonical);
        }
    }
    Err(Error::NotFound)
}

/// Returns an iterator over every entry name in the embedded database.
///
/// This includes the data tables `iso3166.tab` and `zone1970.tab`, which are
/// not loadable as zones.
pub fn names() -> impl Iterator<Item = &'static str> {
    zipstore::names(ZONEINFO_ZIP)
}

impl Zone<'_> {
    /// Returns metadata (countries, coordinates) for this timezone, or `None`.
    pub fn meta(&self) -> Option<ZoneMeta<'static>> {
        meta(self.name())
    }
}
