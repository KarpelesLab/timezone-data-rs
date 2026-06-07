//! `timezone-data` provides direct, allocation-free access to IANA timezone
//! data, exposing the transitions, zone types, POSIX TZ rules, leap seconds,
//! and metadata that most timezone libraries keep private.
//!
//! Timezone data is compiled from the official IANA source and embedded in the
//! crate as pre-parsed static objects — one per zone — so there is no
//! dependency on the host system's timezone files and nothing is parsed at
//! runtime. The crate is `#![no_std]` and never allocates: a lookup is a binary
//! search over `&'static` data.
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

mod db;
mod error;
mod meta;
mod posix;
mod zone;

pub use error::Error;
pub use meta::{meta, parse_iso6709, Country, ZoneMeta};
pub use posix::{parse_posix_tz, PosixTz, RuleKind, TransitionRule};
pub use zone::{LeapSecond, RangeIter, RangeTransition, Transition, Zone, ZoneType};

/// Loads a [`Zone`] by IANA timezone name from the embedded database.
///
/// An empty name or `"UTC"` resolves to the `UTC` zone.
pub fn load(name: &str) -> Result<Zone, Error> {
    let query = if name.is_empty() { "UTC" } else { name };
    db::find(query).ok_or(Error::NotFound)
}

/// Loads a [`Zone`] by name, falling back to case-insensitive matching.
pub fn load_insensitive(name: &str) -> Result<Zone, Error> {
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

/// Returns an iterator over every IANA timezone name in the embedded database.
pub fn names() -> impl Iterator<Item = &'static str> {
    db::names()
}

impl Zone {
    /// Returns metadata (countries, coordinates) for this timezone, or `None`.
    pub fn meta(&self) -> Option<ZoneMeta<'static>> {
        meta(self.name())
    }
}
