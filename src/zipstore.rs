//! Minimal, zero-allocation reader for the embedded `zoneinfo.zip`.
//!
//! The archive is produced with no compression (every entry uses the STORE
//! method), so we never need an inflate implementation: locating an entry is a
//! matter of walking the central directory and returning a sub-slice of the
//! backing bytes. Nothing is copied and nothing is allocated.

use crate::error::Error;

const EOCD_SIG: u32 = 0x0605_4b50;
const CEN_SIG: u32 = 0x0201_4b50;

const EOCD_MIN_LEN: usize = 22;
const CEN_FIXED_LEN: usize = 46;
const LOC_FIXED_LEN: usize = 30;

fn u16_at(data: &[u8], off: usize) -> Option<u16> {
    data.get(off..off + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(data: &[u8], off: usize) -> Option<u32> {
    data.get(off..off + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// A single central-directory record, decoded lazily.
struct CentralEntry<'a> {
    name: &'a str,
    method: u16,
    uncompressed_size: u32,
    local_offset: u32,
}

/// Iterator over the central-directory entries of a STORE-only zip archive.
struct CentralDir<'a> {
    zip: &'a [u8],
    pos: usize,
    remaining: usize,
}

impl<'a> CentralDir<'a> {
    fn open(zip: &'a [u8]) -> Result<Self, Error> {
        if zip.len() < EOCD_MIN_LEN {
            return Err(Error::BadZip("archive too short"));
        }
        // Scan backwards for the End Of Central Directory signature. The trailing
        // comment is empty in our archive, but tolerate up to 64 KiB just in case.
        let max_back = core::cmp::min(zip.len(), EOCD_MIN_LEN + 0xffff);
        let start = zip.len() - max_back;
        let mut eocd = None;
        let mut i = zip.len() - EOCD_MIN_LEN;
        loop {
            if u32_at(zip, i) == Some(EOCD_SIG) {
                eocd = Some(i);
                break;
            }
            if i == start {
                break;
            }
            i -= 1;
        }
        let eocd = eocd.ok_or(Error::BadZip("no end-of-central-directory record"))?;

        let total = u16_at(zip, eocd + 10).ok_or(Error::BadZip("truncated EOCD"))? as usize;
        let cd_offset = u32_at(zip, eocd + 16).ok_or(Error::BadZip("truncated EOCD"))? as usize;
        if cd_offset > zip.len() {
            return Err(Error::BadZip("central directory offset out of range"));
        }
        Ok(CentralDir {
            zip,
            pos: cd_offset,
            remaining: total,
        })
    }
}

impl<'a> Iterator for CentralDir<'a> {
    type Item = CentralEntry<'a>;

    fn next(&mut self) -> Option<CentralEntry<'a>> {
        if self.remaining == 0 {
            return None;
        }
        let zip = self.zip;
        let p = self.pos;
        if u32_at(zip, p)? != CEN_SIG {
            self.remaining = 0;
            return None;
        }
        let method = u16_at(zip, p + 10)?;
        let uncompressed_size = u32_at(zip, p + 24)?;
        let name_len = u16_at(zip, p + 28)? as usize;
        let extra_len = u16_at(zip, p + 30)? as usize;
        let comment_len = u16_at(zip, p + 32)? as usize;
        let local_offset = u32_at(zip, p + 42)?;

        let name_start = p + CEN_FIXED_LEN;
        let name_bytes = zip.get(name_start..name_start + name_len)?;
        let name = core::str::from_utf8(name_bytes).ok()?;

        self.pos = name_start + name_len + extra_len + comment_len;
        self.remaining -= 1;

        Some(CentralEntry {
            name,
            method,
            uncompressed_size,
            local_offset,
        })
    }
}

/// Returns the raw (stored) bytes of `name` within `zip`, or an error.
pub fn find<'a>(zip: &'a [u8], name: &str) -> Result<&'a [u8], Error> {
    find_named(zip, name).map(|(_, data)| data)
}

/// Like [`find`], but also returns the archive's canonical name slice (useful
/// for obtaining a `&'static str` name from a borrowed query).
pub fn find_named<'a>(zip: &'a [u8], name: &str) -> Result<(&'a str, &'a [u8]), Error> {
    for entry in CentralDir::open(zip)? {
        if entry.name == name {
            if entry.method != 0 {
                return Err(Error::BadZip("entry is compressed"));
            }
            let data = read_stored(zip, entry.local_offset, entry.uncompressed_size)?;
            return Ok((entry.name, data));
        }
    }
    Err(Error::NotFound)
}

/// Returns an iterator over every entry name in `zip`.
pub fn names(zip: &'static [u8]) -> impl Iterator<Item = &'static str> {
    CentralDir::open(zip).into_iter().flatten().map(|e| e.name)
}

fn read_stored(zip: &[u8], local_offset: u32, size: u32) -> Result<&[u8], Error> {
    let lo = local_offset as usize;
    if u32_at(zip, lo) != Some(0x0403_4b50) {
        return Err(Error::BadZip("bad local file header"));
    }
    let name_len = u16_at(zip, lo + 26).ok_or(Error::BadZip("truncated local header"))? as usize;
    let extra_len = u16_at(zip, lo + 28).ok_or(Error::BadZip("truncated local header"))? as usize;
    let data_start = lo + LOC_FIXED_LEN + name_len + extra_len;
    zip.get(data_start..data_start + size as usize)
        .ok_or(Error::BadZip("entry data out of range"))
}
