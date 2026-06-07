//! The embedded timezone database: a sorted table of `(name, TZif bytes)`.
//!
//! The table lives in `generated.rs`, produced ahead of time by the `xtask`
//! generator (`cargo run -p xtask`), which unpacks `zoneinfo.zip` into the
//! committed `src/zoneinfo/` files and emits one `include_bytes!` per entry.
//! Building the library never touches the archive; lookups are a binary search
//! over `&'static` data.

// `pub static ENTRIES: &[(&str, &[u8])]`, sorted by name.
include!("generated.rs");

/// Returns the canonical name and bytes of the entry named `name`.
pub fn find(name: &str) -> Option<(&'static str, &'static [u8])> {
    ENTRIES
        .binary_search_by_key(&name, |&(n, _)| n)
        .ok()
        .map(|i| ENTRIES[i])
}

/// Returns just the bytes of the entry named `name`.
pub fn file(name: &str) -> Option<&'static [u8]> {
    find(name).map(|(_, data)| data)
}

/// Returns an iterator over every entry name, in sorted order.
pub fn names() -> impl Iterator<Item = &'static str> {
    ENTRIES.iter().map(|&(n, _)| n)
}
