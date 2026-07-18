//! Scans a directory of replay files and reports parsing statistics, verifying the section walk
//! against an independent byte-level walk of each file. Useful for checking parser changes
//! against a large real-world corpus.
//!
//! Usage: cargo run --release -p broodrep --example corpus_scan -- <directory>

use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use broodrep::{Replay, ReplayFormat, ReplaySection};

const CHECK_SECTIONS: &[ReplaySection] = &[
    ReplaySection::Header,
    ReplaySection::Commands,
    ReplaySection::MapData,
    ReplaySection::PlayerNames,
    ReplaySection::Skins,
    ReplaySection::Limits,
    ReplaySection::Bfix,
    ReplaySection::CustomColors,
    ReplaySection::Gcfg,
    ReplaySection::ShieldBattery,
];

fn collect_reps(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_reps(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("rep"))
        {
            out.push(path);
        }
    }
}

/// Returns the modern-section tags found by walking the chain from the seRS offset field, if the
/// chain ends exactly at EOF (i.e. the file is externally verifiable).
fn manual_modern_sections(data: &[u8]) -> Option<Vec<[u8; 4]>> {
    if data.len() < 20 || &data[12..16] != b"seRS" {
        return None;
    }
    let start = u32::from_le_bytes(data[16..20].try_into().unwrap()) as usize;
    if start < 20 || start > data.len() {
        return None;
    }
    let mut tags = vec![];
    let mut pos = start;
    while pos + 8 <= data.len() {
        let tag: [u8; 4] = data[pos..pos + 4].try_into().unwrap();
        let size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;
        pos = pos.checked_add(8)?.checked_add(size)?;
        if pos > data.len() {
            return None;
        }
        tags.push(tag);
    }
    (pos == data.len()).then_some(tags)
}

fn main() {
    let dir = std::env::args().nth(1).expect("usage: corpus_scan <dir>");
    let mut files = vec![];
    collect_reps(Path::new(&dir), &mut files);
    println!("scanning {} replay files...", files.len());

    // Silence panic output; we record panics ourselves
    std::panic::set_hook(Box::new(|_| {}));

    let mut parsed = 0usize;
    let mut parse_errors: Vec<(PathBuf, String)> = vec![];
    let mut panics: Vec<(PathBuf, String)> = vec![];
    let mut section_errors: Vec<(PathBuf, String)> = vec![];
    let mut walk_mismatches: Vec<(PathBuf, String)> = vec![];
    let mut format_counts = std::collections::HashMap::new();
    let mut sbat_found = 0usize;
    let mut nonascii_names: Vec<(PathBuf, String)> = vec![];

    for path in &files {
        let Ok(data) = std::fs::read(path) else {
            continue;
        };

        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut replay = match Replay::new(Cursor::new(&data[..])) {
                Ok(r) => r,
                Err(e) => return Err(format!("parse: {e}")),
            };
            *format_counts.entry(replay.format()).or_insert(0usize) += 1;

            let _ = replay.players().count();
            let _ = replay.observers().count();
            if !replay.map_name().is_ascii() {
                nonascii_names.push((path.clone(), replay.map_name().to_string()));
            }

            let mut found = vec![];
            for &section in CHECK_SECTIONS {
                match replay.get_raw_section(section) {
                    Ok(Some(_)) => found.push(section),
                    Ok(None) => {}
                    Err(e) => section_errors.push((path.clone(), format!("{section:?}: {e}"))),
                }
            }
            if found.contains(&ReplaySection::ShieldBattery) {
                sbat_found += 1;
            }
            let _ = replay.get_limits();
            let _ = replay.get_shieldbattery_section();
            if let Err(e) = replay.get_commands() {
                section_errors.push((path.clone(), format!("commands parse: {e}")));
            }

            // Cross-check the library's section walk against an independent walk of the file
            if replay.format() == ReplayFormat::Modern121
                && let Some(tags) = manual_modern_sections(&data)
            {
                for tag in tags {
                    let section: ReplaySection = tag.into();
                    let known = CHECK_SECTIONS.contains(&section);
                    if known && !found.contains(&section) {
                        walk_mismatches.push((
                            path.clone(),
                            format!("{} present in file but not found by library", {
                                String::from_utf8_lossy(&tag)
                            }),
                        ));
                    }
                }
            }
            Ok(())
        }));

        match result {
            Ok(Ok(())) => parsed += 1,
            Ok(Err(e)) => parse_errors.push((path.clone(), e)),
            Err(payload) => {
                let msg = payload
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unknown panic".to_string());
                panics.push((path.clone(), msg));
            }
        }
    }

    println!("\nparsed OK:      {parsed}/{}", files.len());
    println!("formats:        {format_counts:?}");
    println!("Sbat found:     {sbat_found}");
    println!("parse errors:   {}", parse_errors.len());
    for (p, e) in parse_errors.iter().take(10) {
        println!("  {} - {e}", p.display());
    }
    println!("PANICS:         {}", panics.len());
    for (p, e) in panics.iter().take(10) {
        println!("  {} - {e}", p.display());
    }
    println!("section errors: {}", section_errors.len());
    for (p, e) in section_errors.iter().take(10) {
        println!("  {} - {e}", p.display());
    }
    println!("walk mismatches: {}", walk_mismatches.len());
    for (p, e) in walk_mismatches.iter().take(10) {
        println!("  {} - {e}", p.display());
    }
    println!("non-ASCII map names: {} (sample)", nonascii_names.len());
    for (p, name) in nonascii_names.iter().take(30) {
        println!("  {name:?} - {}", p.display());
    }
}
