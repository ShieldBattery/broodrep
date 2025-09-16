use broodrep::{DecompressionConfig, Replay};
use js_sys::Uint8Array;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use wasm_bindgen::prelude::*;

/// JavaScript-friendly decompression configuration options.
/// These settings help prevent zip bomb attacks and excessive resource usage.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct DecompressionOptions {
    /// Maximum bytes to decompress in total (default: 100MB)
    #[serde(default)]
    pub max_decompressed_size: Option<u64>,

    /// Maximum compression ratio allowed (default: 500:1)
    #[serde(default)]
    pub max_compression_ratio: Option<f64>,
}

impl From<DecompressionOptions> for DecompressionConfig {
    fn from(options: DecompressionOptions) -> Self {
        DecompressionConfig {
            max_decompressed_size: options.max_decompressed_size.unwrap_or(100 * 1024 * 1024),
            max_compression_ratio: options.max_compression_ratio.unwrap_or(500.0),
            // WASM doesn't have support for Instant::now() so we disable this timing check
            max_decompression_time: None,
        }
    }
}

/// A JavaScript-friendly representation of a parsed StarCraft replay.
/// Note: This struct is designed for serialization only - wasm-bindgen cannot
/// directly expose structs with non-Copy fields like String and Vec.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ReplayInfo {
    /// The format version of the replay
    pub format: String,
    /// The engine the game was played under
    pub engine: String,
    /// Number of game frames this replay contains
    pub frames: u32,
    /// Unix timestamp of when the game started (or null if invalid)
    pub start_time: Option<f64>,
    /// Game title
    pub game_title: String,
    /// Map name
    pub map_name: String,
    /// Map width in tiles
    pub map_width: u16,
    /// Map height in tiles
    pub map_height: u16,
    /// Game speed setting
    pub game_speed: String,
    /// Game type
    pub game_type: String,
    /// Game sub-type value
    pub game_sub_type: u16,
    /// Name of the game host
    pub host_name: String,
    /// List of all players (including empty slots)
    pub players: Vec<PlayerInfo>,
    /// List of active players only (excluding empty slots and observers)
    pub active_players: Vec<PlayerInfo>,
    /// List of observers only
    pub observers: Vec<PlayerInfo>,
}

/// A JavaScript-friendly representation of a player.
/// Note: This struct is designed for serialization only.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlayerInfo {
    /// ID of the map slot the player was placed in
    pub slot_id: u16,
    /// Network ID of the player
    pub network_id: u8,
    /// Type of player (Human, Computer, etc.)
    pub player_type: String,
    /// Race of the player
    pub race: String,
    /// Team number
    pub team: u8,
    /// Player name
    pub name: String,
    /// Whether this is an empty slot
    pub is_empty: bool,
    /// Whether this is an observer
    pub is_observer: bool,
}

impl From<&broodrep::Player> for PlayerInfo {
    fn from(player: &broodrep::Player) -> Self {
        PlayerInfo {
            slot_id: player.slot_id,
            network_id: player.network_id,
            player_type: player.player_type.to_string(),
            race: player.race.to_string(),
            team: player.team,
            name: player.name.clone(),
            is_empty: player.is_empty(),
            is_observer: player.is_observer(),
        }
    }
}

/// Parse a StarCraft replay from a Uint8Array (synchronous version).
///
/// # Arguments
/// * `data` - The replay file data as a JavaScript Uint8Array
/// * `options` - Optional decompression configuration to customize security limits
///
/// # Returns
/// A ReplayInfo object containing all parsed replay data as a serialized JsValue,
/// or throws an error if parsing fails.
#[wasm_bindgen(js_name = parseReplay)]
pub fn parse_replay(data: Uint8Array, options: JsValue) -> Result<JsValue, JsValue> {
    let bytes: Vec<u8> = data.to_vec();
    let cursor = Cursor::new(bytes);

    let config = if options.is_null() || options.is_undefined() {
        DecompressionConfig {
            // Instant::now() isn't available in WASM, so we disable timing checks
            // TODO(tec27): We could replace this with web_sys instead, would need to modify the
            // decompression code to make that possible though
            max_decompression_time: None,
            ..Default::default()
        }
    } else {
        let opts: DecompressionOptions = serde_wasm_bindgen::from_value(options)
            .map_err(|e| JsValue::from_str(&format!("Invalid decompression options: {}", e)))?;
        opts.into()
    };
    let replay = Replay::new_with_decompression_config(cursor, config)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse replay: {}", e)))?;

    // TODO(tec27): Would be better to expose some kind of enum here instead of English strings
    let format_str = format!("{}", replay.format());
    let engine_str = format!("{}", replay.engine());

    let start_time = replay
        .start_time()
        .map(|dt| dt.and_utc().timestamp() as f64);

    let (map_width, map_height) = replay.map_dimensions();

    let all_players: Vec<PlayerInfo> = replay.slots().iter().map(PlayerInfo::from).collect();
    let active_players: Vec<PlayerInfo> = replay.players().map(PlayerInfo::from).collect();
    let observers: Vec<PlayerInfo> = replay.observers().map(PlayerInfo::from).collect();

    let replay_info = ReplayInfo {
        format: format_str,
        engine: engine_str,
        frames: replay.frames(),
        start_time,
        game_title: replay.game_title().to_string(),
        map_name: replay.map_name().to_string(),
        map_width,
        map_height,
        game_speed: replay.game_speed().to_string(),
        game_type: replay.game_type().to_string(),
        game_sub_type: replay.game_sub_type(),
        host_name: replay.host_name().to_string(),
        players: all_players,
        active_players,
        observers,
    };

    serde_wasm_bindgen::to_value(&replay_info)
        .map_err(|e| JsValue::from_str(&format!("Serialization failed: {}", e)))
}

/// Get version information about the broodrep library.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Initialize the WASM module. This sets up panic hooks for better error reporting in JavaScript.
#[wasm_bindgen(start)]
pub fn init() {
    // Set up better panic messages for debugging
    console_error_panic_hook::set_once();
}

// For better error reporting in development
#[cfg(feature = "console_error_panic_hook")]
extern crate console_error_panic_hook;

#[cfg(test)]
#[allow(dead_code)]
mod tests {
    use super::*;
    use js_sys::Uint8Array;
    use wasm_bindgen_test::*;

    const LEGACY_REPLAY: &[u8] = include_bytes!("../../broodrep/testdata/things.rep");
    const SCR_121_REPLAY: &[u8] = include_bytes!("../../broodrep/testdata/scr_replay.rep");

    #[wasm_bindgen_test]
    fn test_parse_legacy_replay() {
        let data = Uint8Array::from(LEGACY_REPLAY);
        let result = parse_replay(data, JsValue::undefined());
        assert!(result.is_ok());

        let js_val = result.unwrap();
        let replay_info: ReplayInfo = serde_wasm_bindgen::from_value(js_val).unwrap();

        assert_eq!(replay_info.engine, "Brood War");
        assert_eq!(replay_info.frames, 894);
        assert_eq!(replay_info.game_title, "neiv");
        assert_eq!(replay_info.map_name, "Shadowlands");
    }

    #[wasm_bindgen_test]
    fn test_parse_modern_replay() {
        let data = Uint8Array::from(SCR_121_REPLAY);
        let result = parse_replay(data, JsValue::undefined());
        assert!(result.is_ok());

        let js_val = result.unwrap();
        let replay_info: ReplayInfo = serde_wasm_bindgen::from_value(js_val).unwrap();

        assert_eq!(replay_info.engine, "Brood War");
        assert_eq!(replay_info.frames, 715);
        assert_eq!(replay_info.game_title, "u");
    }

    #[wasm_bindgen_test]
    fn test_version() {
        let v = version();
        assert!(!v.is_empty());
    }

    #[wasm_bindgen_test]
    fn test_invalid_replay() {
        let invalid_data = Uint8Array::from(&[0u8; 100][..]);
        let result = parse_replay(invalid_data, JsValue::undefined());
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    fn test_parse_with_custom_options() {
        let data = Uint8Array::from(LEGACY_REPLAY);

        let options = DecompressionOptions {
            max_decompressed_size: Some(200 * 1024 * 1024), // 200MB
            max_compression_ratio: Some(1000.0),            // Allow higher compression ratios
        };
        let options_js = serde_wasm_bindgen::to_value(&options).unwrap();

        let result = parse_replay(data, options_js);
        assert!(result.is_ok());

        let js_val = result.unwrap();
        let replay_info: ReplayInfo = serde_wasm_bindgen::from_value(js_val).unwrap();

        assert_eq!(replay_info.engine, "Brood War");
        assert_eq!(replay_info.frames, 894);
    }
}
