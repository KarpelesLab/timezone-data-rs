//! TZif (RFC 8536) binary parser and the [`Zone`] type.
//!
//! Parsing records only the byte ranges of each TZif data block; individual
//! records are decoded lazily by the accessor iterators. Nothing is copied out
//! of the source bytes and nothing is allocated.

use crate::error::Error;
use crate::posix::{parse_posix_tz, year_of, PosixTz};

/// Describes a local time type (e.g. `EST`, `EDT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoneType<'a> {
    /// Abbreviated name.
    pub abbrev: &'a str,
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
pub struct RangeTransition<'a> {
    /// Unix timestamp at which the transition takes effect.
    pub when: i64,
    /// The zone type in effect after the transition.
    pub zone_type: ZoneType<'a>,
}

/// A parsed IANA timezone with all raw data exposed.
///
/// `Zone` borrows the TZif bytes it was parsed from; it is cheap to copy.
#[derive(Debug, Clone, Copy)]
pub struct Zone<'a> {
    name: &'a str,
    version: u8,
    data: &'a [u8],
    time_size: usize,
    leap_size: usize,
    timecnt: usize,
    typecnt: usize,
    leapcnt: usize,
    trans_times: &'a [u8],
    trans_types: &'a [u8],
    ttinfo: &'a [u8],
    abbrev: &'a [u8],
    leap: &'a [u8],
    isstd: &'a [u8],
    isut: &'a [u8],
    extend_raw: &'a str,
    extend: Option<PosixTz<'a>>,
}

fn be_i32(b: &[u8]) -> i32 {
    i32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

fn be_i64(b: &[u8]) -> i64 {
    i64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

fn be_u32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

/// Extracts a NUL-terminated string starting at `idx` within `block`.
fn byte_string(block: &[u8], idx: usize) -> &str {
    let s = block.get(idx..).unwrap_or(&[]);
    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    core::str::from_utf8(&s[..end]).unwrap_or("")
}

/// Decodes the `i`-th 6-byte ttinfo record into a [`ZoneType`].
fn decode_type<'a>(ttinfo: &'a [u8], abbrev: &'a [u8], i: usize) -> ZoneType<'a> {
    let r = &ttinfo[i * 6..i * 6 + 6];
    ZoneType {
        offset: be_i32(&r[0..4]),
        is_dst: r[4] != 0,
        abbrev: byte_string(abbrev, r[5] as usize),
    }
}

/// Decodes the `i`-th transition time from `times`.
fn time_at(times: &[u8], time_size: usize, i: usize) -> i64 {
    let off = i * time_size;
    if time_size == 8 {
        be_i64(&times[off..off + 8])
    } else {
        be_i32(&times[off..off + 4]) as i64
    }
}

/// Parses TZif-format binary data into a [`Zone`].
pub fn parse<'a>(name: &'a str, data: &'a [u8]) -> Result<Zone<'a>, Error> {
    if data.len() < 44 {
        return Err(Error::BadData("file too short"));
    }
    if &data[..4] != b"TZif" {
        return Err(Error::BadData("invalid magic number"));
    }

    let version = match data[4] {
        0 => 1u8,
        b'2' => 2,
        b'3' => 3,
        b'4' => 4,
        _ => return Err(Error::BadData("unknown version byte")),
    };

    // Header counts: six big-endian u32 at offset 20.
    let counts = |base: usize| -> Result<[usize; 6], Error> {
        let h = data
            .get(base..base + 24)
            .ok_or(Error::BadData("header truncated"))?;
        Ok([
            be_u32(&h[0..4]) as usize,   // isutcnt
            be_u32(&h[4..8]) as usize,   // isstdcnt
            be_u32(&h[8..12]) as usize,  // leapcnt
            be_u32(&h[12..16]) as usize, // timecnt
            be_u32(&h[16..20]) as usize, // typecnt
            be_u32(&h[20..24]) as usize, // charcnt
        ])
    };

    let [mut isutcnt, mut isstdcnt, mut leapcnt, mut timecnt, mut typecnt, mut charcnt] =
        counts(20)?;

    if typecnt == 0 {
        return Err(Error::BadData("no time types"));
    }

    // Size of the v1 data block, used to skip it for v2+ files.
    let v1_data_size = timecnt * 4 // transition times (int32)
        + timecnt          // transition type indices
        + typecnt * 6      // ttinfo records
        + charcnt          // abbreviation chars
        + leapcnt * 8      // leap second records (v1: 4+4)
        + isstdcnt         // std/wall indicators
        + isutcnt; // UT/local indicators

    if data.len() < 44 + v1_data_size {
        return Err(Error::BadData("v1 data block truncated"));
    }

    let time_size;
    let leap_size;
    let data_off;

    if version >= 2 {
        // Skip the v1 data block and read the v2+ header.
        let v2_hdr = 44 + v1_data_size;
        let h = data
            .get(v2_hdr..v2_hdr + 44)
            .ok_or(Error::BadData("v2 header truncated"))?;
        if &h[..4] != b"TZif" {
            return Err(Error::BadData("v2 magic mismatch"));
        }
        [isutcnt, isstdcnt, leapcnt, timecnt, typecnt, charcnt] = counts(v2_hdr + 20)?;
        if typecnt == 0 {
            return Err(Error::BadData("no time types in v2 block"));
        }
        time_size = 8;
        leap_size = 12;
        data_off = v2_hdr + 44;
    } else {
        time_size = 4;
        leap_size = 8;
        data_off = 44;
    }

    let total_needed = timecnt * time_size
        + timecnt
        + typecnt * 6
        + charcnt
        + leapcnt * leap_size
        + isstdcnt
        + isutcnt;

    let block = data
        .get(data_off..data_off + total_needed)
        .ok_or(Error::BadData("data block truncated"))?;

    // Carve the data block into its constituent slices.
    let mut p = 0;
    let take = |p: &mut usize, n: usize| -> &[u8] {
        let s = &block[*p..*p + n];
        *p += n;
        s
    };
    let trans_times = take(&mut p, timecnt * time_size);
    let trans_types = take(&mut p, timecnt);
    let ttinfo = take(&mut p, typecnt * 6);
    let abbrev = take(&mut p, charcnt);
    let leap = take(&mut p, leapcnt * leap_size);
    let isstd = take(&mut p, isstdcnt);
    let isut = take(&mut p, isutcnt);

    // Validate transition type indices up front.
    for &idx in trans_types {
        if idx as usize >= typecnt {
            return Err(Error::BadData("transition type index out of range"));
        }
    }

    // POSIX TZ footer (v2+ only): "\n<rule>\n".
    let mut extend_raw = "";
    let mut extend = None;
    if version >= 2 {
        let footer = &data[data_off + total_needed..];
        if footer.len() > 1 && footer[0] == b'\n' {
            let rest = &footer[1..];
            if let Some(nl) = rest.iter().position(|&b| b == b'\n') {
                if let Ok(s) = core::str::from_utf8(&rest[..nl]) {
                    extend_raw = s;
                    if !s.is_empty() {
                        extend = parse_posix_tz(s).ok();
                    }
                }
            }
        }
    }

    Ok(Zone {
        name,
        version,
        data,
        time_size,
        leap_size,
        timecnt,
        typecnt,
        leapcnt,
        trans_times,
        trans_types,
        ttinfo,
        abbrev,
        leap,
        isstd,
        isut,
        extend_raw,
        extend,
    })
}

impl<'a> Zone<'a> {
    /// The IANA timezone name.
    pub fn name(&self) -> &'a str {
        self.name
    }

    /// The TZif format version (1, 2, 3, or 4).
    pub fn version(&self) -> u8 {
        self.version
    }

    /// The original TZif binary data this zone was parsed from.
    pub fn raw_data(&self) -> &'a [u8] {
        self.data
    }

    /// The parsed POSIX TZ rule for computing future transitions, if any.
    pub fn extend(&self) -> Option<&PosixTz<'a>> {
        self.extend.as_ref()
    }

    /// The raw POSIX TZ footer string (empty if none).
    pub fn extend_raw(&self) -> &'a str {
        self.extend_raw
    }

    /// The number of local time types.
    pub fn type_count(&self) -> usize {
        self.typecnt
    }

    /// Returns the `i`-th local time type. Panics if `i >= type_count()`.
    pub fn type_at(&self, i: usize) -> ZoneType<'a> {
        decode_type(self.ttinfo, self.abbrev, i)
    }

    /// Iterates over the zone's local time types.
    pub fn types(&self) -> impl Iterator<Item = ZoneType<'a>> + 'a {
        let ttinfo = self.ttinfo;
        let abbrev = self.abbrev;
        (0..self.typecnt).map(move |i| decode_type(ttinfo, abbrev, i))
    }

    /// Iterates over the stored transition records.
    pub fn transitions(&self) -> impl Iterator<Item = Transition> + 'a {
        let times = self.trans_times;
        let types = self.trans_types;
        let isstd = self.isstd;
        let isut = self.isut;
        let time_size = self.time_size;
        (0..self.timecnt).map(move |i| Transition {
            when: time_at(times, time_size, i),
            type_idx: types[i] as usize,
            is_std: isstd.get(i).is_some_and(|&b| b != 0),
            is_ut: isut.get(i).is_some_and(|&b| b != 0),
        })
    }

    /// Iterates over the leap-second records.
    pub fn leap_seconds(&self) -> impl Iterator<Item = LeapSecond> + 'a {
        let leap = self.leap;
        let leap_size = self.leap_size;
        let time_size = self.time_size;
        (0..self.leapcnt).map(move |i| {
            let off = i * leap_size;
            let when = if time_size == 8 {
                be_i64(&leap[off..off + 8])
            } else {
                be_i32(&leap[off..off + 4]) as i64
            };
            LeapSecond {
                when,
                correction: be_i32(&leap[off + time_size..off + time_size + 4]),
            }
        })
    }

    /// Returns the zone type in effect at the given Unix timestamp.
    ///
    /// Searches stored transitions and falls back to the POSIX TZ rule for
    /// times after the last transition.
    pub fn lookup(&self, unix: i64) -> ZoneType<'a> {
        if self.timecnt == 0 {
            if self.typecnt > 0 {
                return self.type_at(0);
            }
            return ZoneType {
                abbrev: "UTC",
                offset: 0,
                is_dst: false,
            };
        }

        // Binary search: lo = number of transitions whose time is <= unix.
        let (mut lo, mut hi) = (0usize, self.timecnt);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if time_at(self.trans_times, self.time_size, mid) <= unix {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }

        if lo == 0 {
            // Before the first transition: first non-DST type, else type 0.
            for zt in self.types() {
                if !zt.is_dst {
                    return zt;
                }
            }
            return self.type_at(0);
        }

        if lo == self.timecnt {
            if let Some(ext) = &self.extend {
                let (abbrev, offset, is_dst) = ext.lookup(unix);
                return ZoneType {
                    abbrev,
                    offset,
                    is_dst,
                };
            }
        }

        self.type_at(self.trans_types[lo - 1] as usize)
    }

    /// Returns transitions in the half-open interval `[start_unix, end_unix)`,
    /// combining stored transitions with ones generated from the POSIX TZ
    /// extend rule. The result is yielded in chronological order.
    pub fn transitions_for_range(&self, start_unix: i64, end_unix: i64) -> RangeIter<'a> {
        let last_stored = if self.timecnt > 0 {
            time_at(self.trans_times, self.time_size, self.timecnt - 1)
        } else {
            i64::MIN
        };
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
pub struct RangeIter<'a> {
    zone: Zone<'a>,
    start_unix: i64,
    end_unix: i64,
    stored_idx: usize,
    stored_done: bool,
    last_stored: i64,
    generate: bool,
    year: i32,
    end_year: i32,
    pending: [Option<RangeTransition<'a>>; 2],
    pending_i: usize,
}

impl<'a> Iterator for RangeIter<'a> {
    type Item = RangeTransition<'a>;

    fn next(&mut self) -> Option<RangeTransition<'a>> {
        // Phase 1: stored transitions in range.
        if !self.stored_done {
            let z = &self.zone;
            while self.stored_idx < z.timecnt {
                let i = self.stored_idx;
                let when = time_at(z.trans_times, z.time_size, i);
                if when >= self.end_unix {
                    self.stored_done = true;
                    break;
                }
                self.stored_idx += 1;
                if when >= self.start_unix {
                    return Some(RangeTransition {
                        when,
                        zone_type: z.type_at(z.trans_types[i] as usize),
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
                let mut buf: [Option<RangeTransition<'a>>; 2] = [None, None];
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
                if n == 2 && buf[0].as_ref().map(|t| t.when) > buf[1].as_ref().map(|t| t.when) {
                    buf.swap(0, 1);
                }
                self.pending = buf;
            }
        }
    }
}

impl RangeIter<'_> {
    fn in_range(&self, when: i64) -> bool {
        when >= self.start_unix && when < self.end_unix && when > self.last_stored
    }
}
