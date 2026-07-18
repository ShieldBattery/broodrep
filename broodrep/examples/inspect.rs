//! Diagnostic tool for analyzing replay file structure. Dumps header string bytes and manually
//! walks the block/section structure to find where parsing diverges from the file contents.
//!
//! Usage: cargo run -p broodrep --example inspect -- <replay files...>

use std::io::Cursor;

use broodrep::{Replay, ReplaySection};

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn u32le(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
}

/// Walks one legacy-style block ({checksum, num_chunks, {size, data[size]}...}) starting at
/// `offset`, returning the offset just past it.
fn walk_block(data: &[u8], offset: usize) -> Option<(usize, u32, Vec<u32>)> {
    let num_chunks = u32le(data, offset + 4)?;
    let mut pos = offset + 8;
    let mut sizes = vec![];
    for _ in 0..num_chunks {
        let size = u32le(data, pos)?;
        sizes.push(size);
        pos = pos.checked_add(4)?.checked_add(size as usize)?;
        if pos > data.len() {
            return None;
        }
    }
    Some((pos, num_chunks, sizes))
}

fn main() {
    for path in std::env::args().skip(1) {
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                println!("=== {path}: READ ERROR: {e}");
                continue;
            }
        };
        println!("=== {path} ({} bytes)", data.len());

        let replay = match Replay::new(Cursor::new(data.clone())) {
            Ok(r) => r,
            Err(e) => {
                println!("  PARSE ERROR: {e}");
                continue;
            }
        };
        let mut replay = replay;
        println!("  format: {}", replay.format());
        println!(
            "  title: {:?} | host: {:?} | map: {:?}",
            replay.game_title(),
            replay.host_name(),
            replay.map_name()
        );

        // Dump raw string bytes from the decompressed header
        if let Ok(Some(header)) = replay.get_raw_section(ReplaySection::Header) {
            println!("  title bytes  @0x18: {}", hex(&header[0x18..0x18 + 28]));
            println!("  host bytes   @0x48: {}", hex(&header[0x48..0x48 + 25]));
            println!("  map bytes    @0x61: {}", hex(&header[0x61..0x61 + 32]));
        }

        // Manually walk the top-level file structure
        let is_scr = &data[12..16] == b"seRS";
        let scr_offset = if is_scr { u32le(&data, 16) } else { None };
        let mut pos = if is_scr { 20 } else { 16 };
        println!("  scr_offset field: {scr_offset:?}");
        for name in [
            "header",
            "cmds-size",
            "cmds",
            "chk-size",
            "chk",
            "playernames",
        ] {
            match walk_block(&data, pos) {
                Some((end, chunks, sizes)) => {
                    println!(
                        "  block {name:12} @{pos:6} .. {end:6} ({chunks} chunks, sizes {sizes:?})"
                    );
                    pos = end;
                }
                None => {
                    println!("  block {name:12} @{pos:6} .. INVALID");
                    println!(
                        "    bytes at {pos}: {}",
                        hex(&data[pos.min(data.len())..(pos + 48).min(data.len())])
                    );
                    pos = usize::MAX;
                    break;
                }
            }
        }
        if pos != usize::MAX {
            println!("  after legacy blocks, pos = {pos}");
        }

        // Walk the modern section chain from scr_offset (if present) and from pos
        if let Some(start) = scr_offset {
            let mut spos = start as usize;
            println!("  modern chain from scr_offset {spos}:");
            while spos + 8 <= data.len() {
                let tag = &data[spos..spos + 4];
                let size = u32le(&data, spos + 4).unwrap();
                println!("    {:?} @{spos} size {size}", String::from_utf8_lossy(tag));
                spos += 8 + size as usize;
            }
            println!(
                "    chain end: {spos} (file len {}, {})",
                data.len(),
                if spos == data.len() {
                    "EXACT"
                } else {
                    "MISALIGNED"
                }
            );
        }
    }
}
