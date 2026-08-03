//! Generates the committed seed corpus under `fuzz/seeds/<target>/`.
//!
//! Seeds are copied/derived from the real test replays in `../broodrep/testdata/` (these are
//! already committed fixtures used by broodrep's own test suite, so reusing them here doesn't
//! introduce any new licensing concerns). Anything over `MAX_SEED_SIZE` is skipped to keep the
//! corpus modest.
//!
//! Seed files get a `.seed` extension instead of `.rep` so they aren't captured by the
//! repository's `*.rep` git-lfs rule -- the corpus should stay plain git files so fuzzing (both
//! locally and in CI) doesn't depend on LFS.
//!
//! Run with `cargo run --bin seed_gen` from `fuzz/`.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use broodrep::{Replay, ReplaySection};

/// Seeds (and sections extracted from replay seeds) larger than this are skipped. Covers all
/// current testdata replays (the largest is ~292KB).
const MAX_SEED_SIZE: u64 = 512 * 1024;

fn main() {
    let testdata_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../broodrep/testdata");
    let seeds_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("seeds");

    let replay_dir = seeds_root.join("replay_parse");
    let commands_dir = seeds_root.join("commands_parse");
    let sbat_dir = seeds_root.join("shieldbattery_parse");

    fs::create_dir_all(&replay_dir).expect("create replay_parse seed dir");
    fs::create_dir_all(&commands_dir).expect("create commands_parse seed dir");
    fs::create_dir_all(&sbat_dir).expect("create shieldbattery_parse seed dir");

    let mut replay_paths: Vec<PathBuf> = fs::read_dir(&testdata_dir)
        .expect("read testdata dir")
        .map(|e| e.expect("read testdata entry").path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "rep"))
        .collect();
    replay_paths.sort();

    for path in &replay_paths {
        let name = path
            .file_stem()
            .expect("replay file stem")
            .to_string_lossy();
        let bytes = fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        if bytes.len() as u64 > MAX_SEED_SIZE {
            println!(
                "skipping {name} for replay_parse: {} bytes exceeds {} byte cap",
                bytes.len(),
                MAX_SEED_SIZE
            );
            continue;
        }

        write_seed(&replay_dir, &format!("{name}.seed"), &bytes);

        // Section seeds can only be derived from replays the library can actually read (e.g.
        // not_a_replay.rep is seeded as-is above, but has no sections to extract).
        let Ok(mut replay) = Replay::new(Cursor::new(&bytes[..])) else {
            println!("note: {name} does not parse as a replay, seeding raw bytes only");
            continue;
        };

        let extractions: [(_, &Path, _); 2] = [
            (ReplaySection::Commands, &commands_dir, "commands"),
            (ReplaySection::ShieldBattery, &sbat_dir, "sbat"),
        ];
        for (section, dir, suffix) in extractions {
            match replay.get_raw_section(section) {
                Ok(Some(data)) => {
                    if data.is_empty() {
                        // Present-but-empty sections aren't useful seeds.
                        continue;
                    }
                    if data.len() as u64 > MAX_SEED_SIZE {
                        println!(
                            "skipping {name} for {suffix}: section is {} bytes, exceeds {} byte cap",
                            data.len(),
                            MAX_SEED_SIZE
                        );
                        continue;
                    }
                    write_seed(dir, &format!("{name}.{suffix}.seed"), &data);
                }
                Ok(None) => {}
                Err(e) => println!("note: extracting {suffix} from {name} failed ({e})"),
            }
        }
    }

    println!("done");
}

fn write_seed(dir: &Path, name: &str, bytes: &[u8]) {
    let path = dir.join(name);
    fs::write(&path, bytes).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    println!(
        "wrote {} ({} bytes)",
        relative(&path).display(),
        bytes.len()
    );
}

fn relative(path: &Path) -> PathBuf {
    path.strip_prefix(env!("CARGO_MANIFEST_DIR"))
        .unwrap_or(path)
        .to_path_buf()
}
