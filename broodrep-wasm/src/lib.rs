use js_sys::Uint8Array;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use tsify::Tsify;
use wasm_bindgen::prelude::*;

/// Decompression configuration options. These settings help prevent zip bomb attacks and excessive
/// resource usage.
#[derive(Clone, Debug, Default, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tsify(from_wasm_abi)]
pub struct DecompressionConfig {
    /// Maximum bytes to decompress in total (default: 100MB)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_decompressed_size: Option<u64>,

    /// Maximum compression ratio allowed (default: 500:1)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_compression_ratio: Option<f64>,
}

impl From<DecompressionConfig> for broodrep::DecompressionConfig {
    fn from(options: DecompressionConfig) -> Self {
        broodrep::DecompressionConfig {
            max_decompressed_size: options.max_decompressed_size.unwrap_or(100 * 1024 * 1024),
            max_compression_ratio: options.max_compression_ratio.unwrap_or(500.0),
            // WASM doesn't have support for Instant::now() so we disable this timing check
            max_decompression_time: None,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[tsify(into_wasm_abi)]
pub enum ReplayFormat {
    Legacy,
    Modern,
    Modern121,
}

impl From<broodrep::ReplayFormat> for ReplayFormat {
    fn from(format: broodrep::ReplayFormat) -> Self {
        match format {
            broodrep::ReplayFormat::Legacy => ReplayFormat::Legacy,
            broodrep::ReplayFormat::Modern => ReplayFormat::Modern,
            broodrep::ReplayFormat::Modern121 => ReplayFormat::Modern121,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi)]
pub enum Engine {
    StarCraft,
    BroodWar,
    Unknown,
}

impl From<broodrep::Engine> for Engine {
    fn from(engine: broodrep::Engine) -> Self {
        match engine {
            broodrep::Engine::StarCraft => Engine::StarCraft,
            broodrep::Engine::BroodWar => Engine::BroodWar,
            broodrep::Engine::Unknown(_) => Engine::Unknown,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi)]
pub enum GameSpeed {
    Slowest,
    Slower,
    Slow,
    Normal,
    Fast,
    Faster,
    Fastest,
}

impl From<broodrep::GameSpeed> for GameSpeed {
    fn from(speed: broodrep::GameSpeed) -> Self {
        match speed {
            broodrep::GameSpeed::Slowest => GameSpeed::Slowest,
            broodrep::GameSpeed::Slower => GameSpeed::Slower,
            broodrep::GameSpeed::Slow => GameSpeed::Slow,
            broodrep::GameSpeed::Normal => GameSpeed::Normal,
            broodrep::GameSpeed::Fast => GameSpeed::Fast,
            broodrep::GameSpeed::Faster => GameSpeed::Faster,
            broodrep::GameSpeed::Fastest => GameSpeed::Fastest,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi)]
pub enum GameType {
    None,
    Melee,
    FreeForAll,
    OneOnOne,
    CaptureTheFlag,
    Greed,
    Slaughter,
    SuddenDeath,
    Ladder,
    UseMapSettings,
    TeamMelee,
    TeamFreeForAll,
    TeamCaptureTheFlag,
    TopVsBottom,
    Unknown,
}

impl From<broodrep::GameType> for GameType {
    fn from(game_type: broodrep::GameType) -> Self {
        match game_type {
            broodrep::GameType::None => GameType::None,
            broodrep::GameType::Melee => GameType::Melee,
            broodrep::GameType::FreeForAll => GameType::FreeForAll,
            broodrep::GameType::OneOnOne => GameType::OneOnOne,
            broodrep::GameType::CaptureTheFlag => GameType::CaptureTheFlag,
            broodrep::GameType::Greed => GameType::Greed,
            broodrep::GameType::Slaughter => GameType::Slaughter,
            broodrep::GameType::SuddenDeath => GameType::SuddenDeath,
            broodrep::GameType::Ladder => GameType::Ladder,
            broodrep::GameType::UseMapSettings => GameType::UseMapSettings,
            broodrep::GameType::TeamMelee => GameType::TeamMelee,
            broodrep::GameType::TeamFreeForAll => GameType::TeamFreeForAll,
            broodrep::GameType::TeamCaptureTheFlag => GameType::TeamCaptureTheFlag,
            broodrep::GameType::TopVsBottom => GameType::TopVsBottom,
            broodrep::GameType::Unknown(_) => GameType::Unknown,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi)]
pub enum PlayerType {
    Inactive,
    Computer,
    Human,
    RescuePassive,
    Unused,
    ComputerControlled,
    Open,
    Neutral,
    Closed,
}

impl From<broodrep::PlayerType> for PlayerType {
    fn from(player_type: broodrep::PlayerType) -> Self {
        match player_type {
            broodrep::PlayerType::Inactive => PlayerType::Inactive,
            broodrep::PlayerType::Computer => PlayerType::Computer,
            broodrep::PlayerType::Human => PlayerType::Human,
            broodrep::PlayerType::RescuePassive => PlayerType::RescuePassive,
            broodrep::PlayerType::Unused => PlayerType::Unused,
            broodrep::PlayerType::ComputerControlled => PlayerType::ComputerControlled,
            broodrep::PlayerType::Open => PlayerType::Open,
            broodrep::PlayerType::Neutral => PlayerType::Neutral,
            broodrep::PlayerType::Closed => PlayerType::Closed,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi)]
pub enum Race {
    Zerg,
    Terran,
    Protoss,
    Random,
}

impl From<broodrep::Race> for Race {
    fn from(value: broodrep::Race) -> Self {
        match value {
            broodrep::Race::Zerg => Race::Zerg,
            broodrep::Race::Terran => Race::Terran,
            broodrep::Race::Protoss => Race::Protoss,
            broodrep::Race::Random => Race::Random,
        }
    }
}

/// A player in the replay.
#[derive(Clone, Debug, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi)]
pub struct Player {
    /// ID of the map slot the player was placed in (post-randomization, if applicable).
    pub slot_id: u16,
    /// Network ID of the player. Computer players will be 255. Observers will be 128-131.
    pub network_id: u8,
    pub player_type: PlayerType,
    pub race: Race,
    pub team: u8,
    pub name: String,

    pub is_empty: bool,
    pub is_observer: bool,
}

impl From<broodrep::Player> for Player {
    fn from(player: broodrep::Player) -> Self {
        Player {
            is_empty: player.is_empty(),
            is_observer: player.is_observer(),

            slot_id: player.slot_id,
            network_id: player.network_id,
            player_type: player.player_type.into(),
            race: player.race.into(),
            team: player.team,
            name: player.name,
        }
    }
}

#[derive(Clone, Debug, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi)]
pub struct ReplayHeader {
    pub engine: Engine,
    pub frames: u32,
    pub start_time: u32,
    pub title: String,
    pub map_width: u16,
    pub map_height: u16,
    pub available_slots: u8,
    pub speed: GameSpeed,
    pub game_type: GameType,
    pub game_sub_type: u16,
    pub host_name: String,
    pub map_name: String,
}

impl From<broodrep::ReplayHeader> for ReplayHeader {
    fn from(header: broodrep::ReplayHeader) -> Self {
        ReplayHeader {
            engine: header.engine.into(),
            frames: header.frames,
            start_time: header.start_time,
            title: header.title,
            map_width: header.map_width,
            map_height: header.map_height,
            available_slots: header.available_slots,
            speed: header.speed.into(),
            game_type: header.game_type.into(),
            game_sub_type: header.game_sub_type,
            host_name: header.host_name,
            map_name: header.map_name,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tsify(from_wasm_abi, into_wasm_abi)]
pub enum ReplaySection {
    /// Header containing basic game information and player slots
    Header,
    /// Commands issued by players during the game
    Commands,
    /// CHK map data
    MapData,
    /// Longer strings for player names (that also seem to always be utf-8, so safer to decode)
    PlayerNames,
    /// Building/unit skin settings for players
    Skins,
    /// Unit/sprite limits for the game
    Limits,
    /// Bug fix(es)? TODO(tec27): Figure out what this actually is :)
    Bfix,
    /// Custom (extended) team color settings
    CustomColors,
    /// Game configuration? TODO(tec27): Figure out what this actually is :)
    Gcfg,

    // Non-official sections
    ShieldBattery,
}

impl From<ReplaySection> for broodrep::ReplaySection {
    fn from(section: ReplaySection) -> Self {
        match section {
            ReplaySection::Header => broodrep::ReplaySection::Header,
            ReplaySection::Commands => broodrep::ReplaySection::Commands,
            ReplaySection::MapData => broodrep::ReplaySection::MapData,
            ReplaySection::PlayerNames => broodrep::ReplaySection::PlayerNames,
            ReplaySection::Skins => broodrep::ReplaySection::Skins,
            ReplaySection::Limits => broodrep::ReplaySection::Limits,
            ReplaySection::Bfix => broodrep::ReplaySection::Bfix,
            ReplaySection::CustomColors => broodrep::ReplaySection::CustomColors,
            ReplaySection::Gcfg => broodrep::ReplaySection::Gcfg,
            ReplaySection::ShieldBattery => broodrep::ReplaySection::ShieldBattery,
        }
    }
}

/// A parsed StarCraft replay. Only the header will be parsed eagerly, other sections may be
/// processed on demand.
///
/// Retrieving individual fields may be unexpectedly expensive, so it's recommended to store/reuse
/// their values instead of repeatedly accessing them.
#[wasm_bindgen]
pub struct Replay {
    replay: broodrep::Replay<Cursor<Vec<u8>>>,
    #[wasm_bindgen(readonly)]
    pub format: ReplayFormat,
    #[wasm_bindgen(readonly, getter_with_clone)]
    pub header: ReplayHeader,
}

#[wasm_bindgen]
impl Replay {
    fn new(replay: broodrep::Replay<Cursor<Vec<u8>>>) -> Self {
        Replay {
            format: replay.format.into(),
            header: replay.header.clone().into(),

            replay,
        }
    }

    #[wasm_bindgen(js_name = hostPlayer)]
    pub fn host_player(&self) -> Option<Player> {
        self.replay.host_player().cloned().map(Into::into)
    }

    pub fn players(&self) -> Vec<Player> {
        self.replay.players().cloned().map(Into::into).collect()
    }

    pub fn observers(&self) -> Vec<Player> {
        self.replay.observers().cloned().map(Into::into).collect()
    }

    pub fn slots(&self) -> Vec<Player> {
        self.replay
            .slots()
            .iter()
            .cloned()
            .map(Into::into)
            .collect()
    }

    /// Returns the raw bytes of a given replay section, or `undefined` if not present in the replay
    /// file. The bytes will be decompressed if it is a section with known compression.
    #[wasm_bindgen(js_name = getRawSection)]
    pub fn get_raw_section(&mut self, section: ReplaySection) -> Result<Option<Vec<u8>>, JsValue> {
        self.replay
            .get_raw_section(section.into())
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Returns the raw bytes of a given replay section (specified by section ID as a 32-bit
    /// number in little-endian format), or `undefined` if not present in the replay. The bytes will
    /// be decompressed if it is a section with known compression.
    #[wasm_bindgen(js_name = getRawCustomSection)]
    pub fn get_raw_custom_section(&mut self, section_id: u32) -> Result<Option<Vec<u8>>, JsValue> {
        self.replay
            .get_raw_section(section_id.to_le_bytes().into())
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

/// Parse a StarCraft replay from a Uint8Array (synchronously).
///
/// # Arguments
/// * `data` - The replay file data as a JavaScript Uint8Array
/// * `options` - Optional decompression configuration to customize security limits
///
/// # Returns
/// A Replay object that allows retrieving information from the replay, or throws an error if
/// parsing fails.
#[wasm_bindgen(js_name = parseReplay)]
pub fn parse_replay(
    data: Uint8Array,
    options: Option<DecompressionConfig>,
) -> Result<Replay, JsValue> {
    let bytes: Vec<u8> = data.to_vec();
    let cursor = Cursor::new(bytes);

    let config = options.unwrap_or_default().into();
    let replay = broodrep::Replay::new_with_decompression_config(cursor, config)
        .map_err(|e| JsValue::from_str(&format!("Failed to parse replay: {}", e)))?;

    Ok(Replay::new(replay))
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
        let result = parse_replay(data, None);
        assert!(result.is_ok());

        let replay = result.unwrap();
        let header = replay.header;

        assert_eq!(header.engine, Engine::BroodWar);
        assert_eq!(header.frames, 894);
        assert_eq!(header.title, "neiv");
        assert_eq!(header.map_name, "Shadowlands");
    }

    #[wasm_bindgen_test]
    fn test_parse_modern_replay() {
        let data = Uint8Array::from(SCR_121_REPLAY);
        let result = parse_replay(data, None);
        assert!(result.is_ok());

        let replay = result.unwrap();
        let header = replay.header;

        assert_eq!(header.engine, Engine::BroodWar);
        assert_eq!(header.frames, 715);
        assert_eq!(header.title, "u");
    }

    #[wasm_bindgen_test]
    fn test_version() {
        let v = version();
        assert!(!v.is_empty());
    }

    #[wasm_bindgen_test]
    fn test_invalid_replay() {
        let invalid_data = Uint8Array::from(&[0u8; 100][..]);
        let result = parse_replay(invalid_data, None);
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    fn test_parse_with_custom_options() {
        let data = Uint8Array::from(LEGACY_REPLAY);

        let options = DecompressionConfig {
            max_decompressed_size: Some(200 * 1024 * 1024), // 200MB
            max_compression_ratio: Some(1000.0),            // Allow higher compression ratios
        };

        let result = parse_replay(data, Some(options));
        assert!(result.is_ok());

        let replay = result.unwrap();
        let header = replay.header;

        assert_eq!(header.engine, Engine::BroodWar);
        assert_eq!(header.frames, 894);
    }
}
