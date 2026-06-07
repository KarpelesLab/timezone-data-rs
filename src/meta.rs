//! Per-zone metadata derived from `zone1970.tab` and `iso3166.tab`.
//!
//! Both tables are embedded in `zoneinfo.zip` and scanned on demand; no index
//! is built and nothing is allocated.

use crate::db;

/// Metadata about a timezone: associated countries and principal coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoneMeta<'a> {
    /// Latitude of the principal location (degrees, north positive).
    pub lat: f64,
    /// Longitude of the principal location (degrees, east positive).
    pub lon: f64,
    /// Optional commentary (e.g. a region description); empty if none.
    pub commentary: &'a str,
    /// The raw comma-separated ISO 3166-1 alpha-2 country codes field.
    codes: &'a str,
}

/// An ISO 3166 country associated with a timezone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Country<'a> {
    /// ISO 3166-1 alpha-2 code (e.g. `US`).
    pub code: &'a str,
    /// Country name (e.g. `United States`); empty if not found.
    pub name: &'a str,
}

impl ZoneMeta<'static> {
    /// Iterates over the countries that overlap this timezone.
    pub fn countries(&self) -> impl Iterator<Item = Country<'static>> {
        self.codes.split(',').map(|code| Country {
            code,
            name: iso_name(code),
        })
    }
}

/// Returns metadata for the timezone named `name`, or `None` if unavailable.
pub fn meta(name: &str) -> Option<ZoneMeta<'static>> {
    let data = db::file("zone1970.tab")?;
    let text = core::str::from_utf8(data).ok()?;
    for line in text.split('\n') {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split('\t');
        let codes = fields.next()?;
        let coord = fields.next()?;
        let zname = match fields.next() {
            Some(z) => z,
            None => continue,
        };
        if zname != name {
            continue;
        }
        let commentary = fields.next().unwrap_or("");
        let (lat, lon) = parse_iso6709(coord);
        return Some(ZoneMeta {
            lat,
            lon,
            commentary,
            codes,
        });
    }
    None
}

/// Looks up the country name for an ISO 3166-1 alpha-2 `code`.
fn iso_name(code: &str) -> &'static str {
    let data = match db::file("iso3166.tab") {
        Some(d) => d,
        None => return "",
    };
    let text = core::str::from_utf8(data).unwrap_or("");
    for line in text.split('\n') {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let c = parts.next().unwrap_or("");
        if c == code {
            return parts.next().unwrap_or("");
        }
    }
    ""
}

/// Parses coordinates in ISO 6709 format `±DDMM±DDDMM` or `±DDMMSS±DDDMMSS`.
pub fn parse_iso6709(s: &str) -> (f64, f64) {
    let b = s.as_bytes();
    // The latitude starts at index 0; the longitude starts at the second sign.
    let mut lon_start = None;
    for (i, &c) in b.iter().enumerate().skip(1) {
        if c == b'+' || c == b'-' {
            lon_start = Some(i);
            break;
        }
    }
    let Some(lon_start) = lon_start else {
        return (0.0, 0.0);
    };
    let lat = parse_dms(&s[..lon_start], 2);
    let lon = parse_dms(&s[lon_start..], 3);
    (lat, lon)
}

/// Parses a `±DD[D]MM[SS]` string into decimal degrees, rounded to 4 places.
/// `deg_digits` is 2 for latitude, 3 for longitude.
fn parse_dms(s: &str, deg_digits: usize) -> f64 {
    let b = s.as_bytes();
    if b.len() < 1 + deg_digits + 2 {
        return 0.0;
    }
    let neg = b[0] == b'-';
    let mut i = 1; // skip sign

    let deg = atoi(&b[i..i + deg_digits]);
    i += deg_digits;
    let min = atoi(&b[i..i + 2]);
    i += 2;
    let sec = if b.len() >= i + 2 {
        atoi(&b[i..i + 2])
    } else {
        0
    };

    // Round to 4 decimal places using integer arithmetic (no std float methods).
    let total_seconds = deg * 3600 + min * 60 + sec;
    let val_e4 = (total_seconds * 10000 + 1800) / 3600;
    let v = val_e4 as f64 / 10000.0;
    if neg {
        -v
    } else {
        v
    }
}

fn atoi(b: &[u8]) -> i64 {
    let mut n = 0i64;
    for &c in b {
        if c.is_ascii_digit() {
            n = n * 10 + (c - b'0') as i64;
        }
    }
    n
}
