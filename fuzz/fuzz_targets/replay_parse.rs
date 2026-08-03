#![no_main]

use std::io::Cursor;

use broodrep::{DecompressionConfig, Replay, ReplaySection};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // The time limit is disabled so the target stays a pure function of the input bytes
    // (wall-clock cutoffs make crashes non-reproducible); the size and ratio limits are what
    // actually bound the work, and are tightened well below the defaults since no fuzz input
    // needs 100MB of decompressed output. Note that these limits apply per chunk.
    let config = DecompressionConfig {
        max_decompressed_size: 4 * 1024 * 1024,
        max_compression_ratio: 1000.0,
        max_decompression_time: None,
    };
    let Ok(mut replay) = Replay::new_with_decompression_config(Cursor::new(data), config) else {
        return;
    };

    exercise_replay(&mut replay);
});

/// Touches every public accessor and lazily-read section, discarding results. Only the header is
/// parsed eagerly, so a successful [Replay::new_with_decompression_config] proves nothing about
/// the other sections -- they are only exercised by actually requesting them.
fn exercise_replay(replay: &mut Replay<Cursor<&[u8]>>) {
    let _ = replay.format();
    let _ = replay.engine();
    let _ = replay.frames();
    let _ = replay.start_time();
    let _ = replay.game_title();
    let _ = replay.map_name();
    let _ = replay.map_dimensions();
    let _ = replay.game_speed();
    let _ = replay.game_type();
    let _ = replay.game_sub_type();
    let _ = replay.host_name();
    let _ = replay.host_player();
    let _ = replay.players().count();
    let _ = replay.observers().count();
    for player in replay.slots() {
        let _ = player.is_empty();
        let _ = player.is_observer();
        let _ = player.is_active();
    }

    // Sections with dedicated parsers.
    let _ = replay.get_limits();
    let _ = replay.get_shieldbattery_section();
    let _ = replay.get_commands();

    // Sections without dedicated parsers still exercise the section reading/decompression paths
    // (chunked + compressed reads for the legacy sections, raw reads for the modern ones).
    for section in [
        ReplaySection::Header,
        ReplaySection::MapData,
        ReplaySection::PlayerNames,
        ReplaySection::Skins,
        ReplaySection::Bfix,
        ReplaySection::CustomColors,
        ReplaySection::Gcfg,
    ] {
        let _ = replay.get_raw_section(section);
    }
}
