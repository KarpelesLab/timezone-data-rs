//! Data generator for the `timezone-data` crate.
//!
//! Pre-parses every TZif file in `zoneinfo.zip` into static Rust objects and
//! writes `src/generated.rs` (the embedded `(name, Zone)` table), plus the
//! `src/zone1970.tab` / `src/iso3166.tab` metadata tables.
//!
//! Run from the repository (`cargo run -p xtask`) whenever `zoneinfo.zip` is
//! updated; the generated output is committed, so building the library itself
//! never has to parse anything.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use timezone_data::{parse_posix_tz, PosixTz, RuleKind, TransitionRule};

// ----------------------------------------------------------------------------
// Zip reading (STORE only)
// ----------------------------------------------------------------------------

fn u16_at(d: &[u8], o: usize) -> usize {
    u16::from_le_bytes([d[o], d[o + 1]]) as usize
}

fn u32_at(d: &[u8], o: usize) -> usize {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]) as usize
}

fn read_zip(zip: &[u8]) -> Vec<(String, Vec<u8>)> {
    const EOCD_SIG: usize = 0x0605_4b50;
    const CEN_SIG: usize = 0x0201_4b50;
    const LOC_SIG: usize = 0x0403_4b50;

    let min = 22;
    let floor = zip.len().saturating_sub(min + 0xffff);
    let mut i = zip.len() - min;
    let eocd = loop {
        if u32_at(zip, i) == EOCD_SIG {
            break i;
        }
        assert!(i != floor, "no end-of-central-directory record");
        i -= 1;
    };

    let total = u16_at(zip, eocd + 10);
    let mut pos = u32_at(zip, eocd + 16);
    let mut out = Vec::with_capacity(total);

    for _ in 0..total {
        assert_eq!(u32_at(zip, pos), CEN_SIG, "bad central directory header");
        let method = u16_at(zip, pos + 10);
        let uncompressed = u32_at(zip, pos + 24);
        let name_len = u16_at(zip, pos + 28);
        let extra_len = u16_at(zip, pos + 30);
        let comment_len = u16_at(zip, pos + 32);
        let local_off = u32_at(zip, pos + 42);
        let name = String::from_utf8(zip[pos + 46..pos + 46 + name_len].to_vec())
            .expect("entry name is not UTF-8");

        if !name.ends_with('/') {
            assert_eq!(method, 0, "entry {name} is compressed (expected STORE)");
            assert_eq!(u32_at(zip, local_off), LOC_SIG, "bad local file header");
            let lname = u16_at(zip, local_off + 26);
            let lextra = u16_at(zip, local_off + 28);
            let data = local_off + 30 + lname + lextra;
            out.push((name, zip[data..data + uncompressed].to_vec()));
        }

        pos += 46 + name_len + extra_len + comment_len;
    }
    out
}

// ----------------------------------------------------------------------------
// TZif decoding
// ----------------------------------------------------------------------------

struct DType {
    offset: i32,
    is_dst: bool,
    abbrev: String,
}

struct DTrans {
    when: i64,
    type_idx: usize,
    is_std: bool,
    is_ut: bool,
}

struct DZone {
    name: String,
    version: u8,
    types: Vec<DType>,
    transitions: Vec<DTrans>,
    extend_raw: String,
}

fn be_i32(b: &[u8]) -> i32 {
    i32::from_be_bytes([b[0], b[1], b[2], b[3]])
}
fn be_i64(b: &[u8]) -> i64 {
    i64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}
fn be_u32(b: &[u8]) -> usize {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize
}

fn byte_string(block: &[u8], idx: usize) -> String {
    let s = &block[idx..];
    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    String::from_utf8_lossy(&s[..end]).into_owned()
}

/// Decodes one TZif file. Assumes well-formed input (it is our own data).
fn decode(name: &str, data: &[u8]) -> DZone {
    assert!(
        data.len() >= 44 && &data[..4] == b"TZif",
        "{name}: not TZif"
    );
    let version = match data[4] {
        0 => 1u8,
        b'2' => 2,
        b'3' => 3,
        b'4' => 4,
        v => panic!("{name}: unknown version byte {v:#x}"),
    };

    let counts = |base: usize| {
        [
            be_u32(&data[base..]),      // isutcnt
            be_u32(&data[base + 4..]),  // isstdcnt
            be_u32(&data[base + 8..]),  // leapcnt
            be_u32(&data[base + 12..]), // timecnt
            be_u32(&data[base + 16..]), // typecnt
            be_u32(&data[base + 20..]), // charcnt
        ]
    };

    let [mut isutcnt, mut isstdcnt, mut leapcnt, mut timecnt, mut typecnt, mut charcnt] =
        counts(20);

    let v1_data = timecnt * 4 + timecnt + typecnt * 6 + charcnt + leapcnt * 8 + isstdcnt + isutcnt;

    let (time_size, leap_size, mut p) = if version >= 2 {
        let v2 = 44 + v1_data;
        assert_eq!(&data[v2..v2 + 4], b"TZif", "{name}: v2 magic");
        [isutcnt, isstdcnt, leapcnt, timecnt, typecnt, charcnt] = counts(v2 + 20);
        (8usize, 12usize, v2 + 44)
    } else {
        (4usize, 8usize, 44usize)
    };

    // Transition times.
    let mut transitions: Vec<DTrans> = Vec::with_capacity(timecnt);
    for _ in 0..timecnt {
        let when = if time_size == 8 {
            be_i64(&data[p..])
        } else {
            be_i32(&data[p..]) as i64
        };
        transitions.push(DTrans {
            when,
            type_idx: 0,
            is_std: false,
            is_ut: false,
        });
        p += time_size;
    }
    // Transition type indices.
    for t in &mut transitions {
        t.type_idx = data[p] as usize;
        p += 1;
    }
    // ttinfo records.
    let mut types = Vec::with_capacity(typecnt);
    let mut abbr_idx = Vec::with_capacity(typecnt);
    for _ in 0..typecnt {
        types.push(DType {
            offset: be_i32(&data[p..]),
            is_dst: data[p + 4] != 0,
            abbrev: String::new(),
        });
        abbr_idx.push(data[p + 5] as usize);
        p += 6;
    }
    // Abbreviation block.
    let abbrev_block = &data[p..p + charcnt];
    for (ty, &idx) in types.iter_mut().zip(&abbr_idx) {
        ty.abbrev = byte_string(abbrev_block, idx);
    }
    p += charcnt;
    // Leap seconds (skipped — our data has none).
    p += leapcnt * leap_size;
    // std/wall + UT/local indicators.
    for i in 0..isstdcnt {
        if i < transitions.len() {
            transitions[i].is_std = data[p] != 0;
        }
        p += 1;
    }
    for i in 0..isutcnt {
        if i < transitions.len() {
            transitions[i].is_ut = data[p] != 0;
        }
        p += 1;
    }

    // POSIX TZ footer (v2+): "\n<rule>\n".
    let mut extend_raw = String::new();
    if version >= 2 {
        let footer = &data[p..];
        if footer.len() > 1 && footer[0] == b'\n' {
            let rest = &footer[1..];
            if let Some(nl) = rest.iter().position(|&b| b == b'\n') {
                extend_raw = String::from_utf8_lossy(&rest[..nl]).into_owned();
            }
        }
    }

    DZone {
        name: name.to_string(),
        version,
        types,
        transitions,
        extend_raw,
    }
}

// ----------------------------------------------------------------------------
// Code generation
// ----------------------------------------------------------------------------

fn rule_kind(k: RuleKind) -> &'static str {
    match k {
        RuleKind::Julian => "Julian",
        RuleKind::DayOfYear => "DayOfYear",
        RuleKind::MonthWeekDay => "MonthWeekDay",
    }
}

fn emit_rule(r: TransitionRule) -> String {
    format!(
        "TransitionRule {{ kind: RuleKind::{}, day: {}, week: {}, mon: {}, time: {} }}",
        rule_kind(r.kind),
        r.day,
        r.week,
        r.mon,
        r.time
    )
}

fn emit_extend(p: &Option<PosixTz<'_>>) -> String {
    match p {
        Some(p) => format!(
            "Some(PosixTz {{ std_abbrev: {:?}, std_offset: {}, dst_abbrev: {:?}, \
             dst_offset: {}, start: {}, end: {} }})",
            p.std_abbrev,
            p.std_offset,
            p.dst_abbrev,
            p.dst_offset,
            emit_rule(p.start),
            emit_rule(p.end),
        ),
        None => "None".to_string(),
    }
}

fn generate(zones: &[DZone]) -> String {
    let mut out = String::new();
    out.push_str(
        "// @generated by `cargo run -p xtask` from zoneinfo.zip — do not edit.\n\
         use crate::{PosixTz, RuleKind, Transition, TransitionRule, Zone, ZoneType};\n\n",
    );

    // Per-zone type and transition arrays.
    for (i, z) in zones.iter().enumerate() {
        if !z.types.is_empty() {
            write!(out, "static TY_{i}: [ZoneType; {}] = [", z.types.len()).unwrap();
            for t in &z.types {
                write!(
                    out,
                    "ZoneType {{ abbrev: {:?}, offset: {}, is_dst: {} }},",
                    t.abbrev, t.offset, t.is_dst
                )
                .unwrap();
            }
            out.push_str("];\n");
        }
        if !z.transitions.is_empty() {
            write!(
                out,
                "static TR_{i}: [Transition; {}] = [",
                z.transitions.len()
            )
            .unwrap();
            for t in &z.transitions {
                write!(
                    out,
                    "Transition {{ when: {}, type_idx: {}, is_std: {}, is_ut: {} }},",
                    t.when, t.type_idx, t.is_std, t.is_ut
                )
                .unwrap();
            }
            out.push_str("];\n");
        }
    }

    // The sorted lookup table.
    out.push_str("\npub static ENTRIES: &[(&str, Zone)] = &[\n");
    for (i, z) in zones.iter().enumerate() {
        let types_ref = if z.types.is_empty() {
            "&[]".to_string()
        } else {
            format!("&TY_{i}")
        };
        let tr_ref = if z.transitions.is_empty() {
            "&[]".to_string()
        } else {
            format!("&TR_{i}")
        };
        let extend = parse_posix_tz(&z.extend_raw).ok();
        write!(
            out,
            "    ({:?}, Zone {{ name: {:?}, version: {}, types: {}, transitions: {}, \
             leap_seconds: &[], extend: {}, extend_raw: {:?} }}),\n",
            z.name,
            z.name,
            z.version,
            types_ref,
            tr_ref,
            emit_extend(&extend),
            z.extend_raw,
        )
        .unwrap();
    }
    out.push_str("];\n");
    out
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a parent directory")
        .to_path_buf();
    let src = root.join("src");

    let zip = fs::read(root.join("zoneinfo.zip")).expect("read zoneinfo.zip");
    let entries = read_zip(&zip);

    let mut zones: Vec<DZone> = Vec::new();
    for (name, bytes) in &entries {
        match name.as_str() {
            "zone1970.tab" | "iso3166.tab" => {
                fs::write(src.join(name), bytes).unwrap();
            }
            _ => zones.push(decode(name, bytes)),
        }
    }
    zones.sort_by(|a, b| a.name.cmp(&b.name));

    fs::write(src.join("generated.rs"), generate(&zones)).unwrap();

    // The raw per-file directory is no longer embedded.
    let _ = fs::remove_dir_all(src.join("zoneinfo"));

    println!("generated {} zones into src/generated.rs", zones.len());
}
