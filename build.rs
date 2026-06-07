//! Build script: unpack the committed `zoneinfo.zip` into individual files and
//! generate a sorted `(name, &[u8])` table embedded via `include_bytes!`.
//!
//! Embedding each TZif file directly lets the library look zones up with a plain
//! binary search at runtime — there is no zip archive to parse. The zip is kept
//! in the repository only as the build-time data source.

use std::env;
use std::fs;
use std::path::PathBuf;

fn u16_at(d: &[u8], o: usize) -> usize {
    u16::from_le_bytes([d[o], d[o + 1]]) as usize
}

fn u32_at(d: &[u8], o: usize) -> usize {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]) as usize
}

/// Extracts `(name, bytes)` for every STORE-compressed entry of a zip archive.
fn read_zip(zip: &[u8]) -> Vec<(String, Vec<u8>)> {
    const EOCD_SIG: usize = 0x0605_4b50;
    const CEN_SIG: usize = 0x0201_4b50;
    const LOC_SIG: usize = 0x0403_4b50;

    // Locate the End Of Central Directory record (scan backwards over a possible
    // trailing comment).
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

/// Escapes a string for inclusion in a Rust double-quoted literal.
fn rust_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn main() {
    println!("cargo:rerun-if-changed=zoneinfo.zip");
    println!("cargo:rerun-if-changed=build.rs");

    let zip = fs::read("zoneinfo.zip").expect("read zoneinfo.zip");
    let mut entries = read_zip(&zip);
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let data_dir = out_dir.join("zoneinfo");

    let mut src = String::from("pub static ENTRIES: &[(&str, &[u8])] = &[\n");
    for (name, bytes) in &entries {
        let path = data_dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        src.push_str(&format!(
            "    (\"{}\", include_bytes!(\"{}\")),\n",
            rust_str(name),
            rust_str(path.to_str().unwrap()),
        ));
    }
    src.push_str("];\n");

    fs::write(out_dir.join("zones.rs"), src).unwrap();
}
