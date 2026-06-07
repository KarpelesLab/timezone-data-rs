//! The embedded timezone database: a sorted table of `(name, Zone)`.
//!
//! The table lives in `generated.rs`, produced ahead of time by the `xtask`
//! generator (`cargo run -p xtask`), which pre-parses every TZif file in
//! `zoneinfo.zip` into static Rust objects. Lookups are a binary search over
//! `&'static` data — nothing is parsed at runtime.

use crate::Zone;

// `pub static ENTRIES: &[(&str, Zone)]`, sorted by name. The module wrapper
// keeps lints off the large generated file.
#[allow(unused_imports, clippy::all, clippy::pedantic, clippy::nursery)]
mod generated {
    include!("generated.rs");
}
use generated::ENTRIES;

/// Returns the [`Zone`] named `name`, or `None`.
pub fn find(name: &str) -> Option<Zone> {
    ENTRIES
        .binary_search_by_key(&name, |&(n, _)| n)
        .ok()
        .map(|i| ENTRIES[i].1)
}

/// Returns an iterator over every zone name, in sorted order.
pub fn names() -> impl Iterator<Item = &'static str> {
    ENTRIES.iter().map(|&(n, _)| n)
}
