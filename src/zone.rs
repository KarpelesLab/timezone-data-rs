//! The [`Zone`] type and its supporting records.
//!
//! Every embedded zone is pre-parsed into static Rust objects by the `xtask`
//! generator (see `src/generated.rs`), so there is no parsing at runtime: the
//! accessors return slices that point directly at `&'static` data.

use crate::posix::{year_of, PosixTz};

/// Describes a local time type (e.g. `EST`, `EDT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoneType {
    /// Abbreviated name.
    pub abbrev: &'static str,
    /// Seconds east of UTC.
    pub offset: i32,
    /// True if this is a daylight-saving time type.
    pub is_dst: bool,
}

/// A moment when the timezone rule changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transition {
    /// Unix timestamp at which the transition takes effect.
    pub when: i64,
    /// Index into the zone's [`types`](Zone::types).
    pub type_idx: usize,
    /// True if the transition time is standard (not wall clock).
    pub is_std: bool,
    /// True if the transition time is UT (not local).
    pub is_ut: bool,
}

/// A leap-second record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeapSecond {
    /// Unix timestamp of the leap second.
    pub when: i64,
    /// Cumulative correction in seconds.
    pub correction: i32,
}

/// A transition produced by [`Zone::transitions_for_range`].
///
/// Unlike [`Transition`], this carries the resolved [`ZoneType`] directly, since
/// transitions generated from the POSIX extend rule may name a type that does
/// not appear in the stored type table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeTransition {
    /// Unix timestamp at which the transition takes effect.
    pub when: i64,
    /// The zone type in effect after the transition.
    pub zone_type: ZoneType,
}

/// A parsed IANA timezone, exposing transitions, zone types, leap seconds, and
/// the POSIX TZ extend rule.
///
/// Instances come from [`load`](crate::load) and reference `&'static` data
/// generated at build time; `Zone` is `Copy`.
#[derive(Debug, Clone, Copy)]
pub struct Zone {
    pub(crate) name: &'static str,
    pub(crate) version: u8,
    pub(crate) types: &'static [ZoneType],
    pub(crate) transitions: &'static [Transition],
    pub(crate) leap_seconds: &'static [LeapSecond],
    pub(crate) extend: Option<PosixTz<'static>>,
    pub(crate) extend_raw: &'static str,
}

impl Zone {
    /// The IANA timezone name.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// The TZif format version the data was compiled from (1, 2, 3, or 4).
    pub fn version(&self) -> u8 {
        self.version
    }

    /// The zone's local time types.
    pub fn types(&self) -> &'static [ZoneType] {
        self.types
    }

    /// The stored transition records.
    pub fn transitions(&self) -> &'static [Transition] {
        self.transitions
    }

    /// The leap-second records.
    pub fn leap_seconds(&self) -> &'static [LeapSecond] {
        self.leap_seconds
    }

    /// The parsed POSIX TZ rule for computing future transitions, if any.
    pub fn extend(&self) -> Option<&PosixTz<'static>> {
        self.extend.as_ref()
    }

    /// The raw POSIX TZ footer string (empty if none).
    pub fn extend_raw(&self) -> &'static str {
        self.extend_raw
    }

    /// Returns the `i`-th local time type. Panics if `i >= types().len()`.
    pub fn type_at(&self, i: usize) -> ZoneType {
        self.types[i]
    }

    /// Returns the zone type in effect at the given Unix timestamp.
    ///
    /// Searches stored transitions and falls back to the POSIX TZ rule for
    /// times after the last transition.
    pub fn lookup(&self, unix: i64) -> ZoneType {
        let tr = self.transitions;
        if tr.is_empty() {
            return self.types.first().copied().unwrap_or(ZoneType {
                abbrev: "UTC",
                offset: 0,
                is_dst: false,
            });
        }

        // Number of transitions whose time is <= unix.
        let lo = tr.partition_point(|t| t.when <= unix);

        if lo == 0 {
            // Before the first transition: first non-DST type, else type 0.
            for zt in self.types {
                if !zt.is_dst {
                    return *zt;
                }
            }
            return self.types[0];
        }

        if lo == tr.len() {
            if let Some(ext) = &self.extend {
                let (abbrev, offset, is_dst) = ext.lookup(unix);
                return ZoneType {
                    abbrev,
                    offset,
                    is_dst,
                };
            }
        }

        self.types[tr[lo - 1].type_idx]
    }

    /// Returns transitions in the half-open interval `[start_unix, end_unix)`,
    /// combining stored transitions with ones generated from the POSIX TZ
    /// extend rule. The result is yielded in chronological order.
    pub fn transitions_for_range(&self, start_unix: i64, end_unix: i64) -> RangeIter {
        let last_stored = self.transitions.last().map(|t| t.when).unwrap_or(i64::MIN);
        let generate = self.extend.map(|e| e.has_dst()).unwrap_or(false);
        RangeIter {
            zone: *self,
            start_unix,
            end_unix,
            stored_idx: 0,
            stored_done: false,
            last_stored,
            generate,
            year: year_of(start_unix),
            end_year: year_of(end_unix),
            pending: [None, None],
            pending_i: 0,
        }
    }
}

/// Iterator returned by [`Zone::transitions_for_range`].
pub struct RangeIter {
    zone: Zone,
    start_unix: i64,
    end_unix: i64,
    stored_idx: usize,
    stored_done: bool,
    last_stored: i64,
    generate: bool,
    year: i32,
    end_year: i32,
    pending: [Option<RangeTransition>; 2],
    pending_i: usize,
}

impl Iterator for RangeIter {
    type Item = RangeTransition;

    fn next(&mut self) -> Option<RangeTransition> {
        // Phase 1: stored transitions in range.
        if !self.stored_done {
            let tr = self.zone.transitions;
            while self.stored_idx < tr.len() {
                let t = tr[self.stored_idx];
                if t.when >= self.end_unix {
                    self.stored_done = true;
                    break;
                }
                self.stored_idx += 1;
                if t.when >= self.start_unix {
                    return Some(RangeTransition {
                        when: t.when,
                        zone_type: self.zone.types[t.type_idx],
                    });
                }
            }
            self.stored_done = true;
        }

        // Phase 2: transitions generated from the POSIX extend rule.
        if !self.generate {
            return None;
        }
        let ext = self.zone.extend.expect("generate implies extend");
        loop {
            // Drain any pending transitions buffered for the current year.
            while self.pending_i < 2 {
                let item = self.pending[self.pending_i].take();
                self.pending_i += 1;
                if let Some(t) = item {
                    return Some(t);
                }
            }

            if self.year > self.end_year {
                return None;
            }

            // Compute this year's transitions, filter to range, sort, buffer.
            let year = self.year;
            self.year += 1;
            self.pending = [None, None];
            self.pending_i = 0;

            if let Some((dst_start, dst_end)) = ext.transitions_for_year(year) {
                let dst_type = ZoneType {
                    abbrev: ext.dst_abbrev,
                    offset: ext.dst_offset,
                    is_dst: true,
                };
                let std_type = ZoneType {
                    abbrev: ext.std_abbrev,
                    offset: ext.std_offset,
                    is_dst: false,
                };
                let mut buf: [Option<RangeTransition>; 2] = [None, None];
                let mut n = 0;
                if self.in_range(dst_start) {
                    buf[n] = Some(RangeTransition {
                        when: dst_start,
                        zone_type: dst_type,
                    });
                    n += 1;
                }
                if self.in_range(dst_end) {
                    buf[n] = Some(RangeTransition {
                        when: dst_end,
                        zone_type: std_type,
                    });
                    n += 1;
                }
                // Sort the (at most two) candidates chronologically.
                if n == 2 && buf[0].map(|t| t.when) > buf[1].map(|t| t.when) {
                    buf.swap(0, 1);
                }
                self.pending = buf;
            }
        }
    }
}

impl RangeIter {
    fn in_range(&self, when: i64) -> bool {
        when >= self.start_unix && when < self.end_unix && when > self.last_stored
    }
}
