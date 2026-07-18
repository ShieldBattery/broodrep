use js_sys::Uint8Array;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use tsify::Tsify;
use uuid::Uuid;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_APPEND_CONTENT: &'static str = r#"
export type Uuid = string;
"#;

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
#[tsify(into_wasm_abi)]
pub enum Race {
    #[serde(rename = "z")]
    Zerg,
    #[serde(rename = "t")]
    Terran,
    #[serde(rename = "p")]
    Protoss,
    #[serde(rename = "r")]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi)]
pub struct ShieldBatteryData {
    pub starcraft_exe_build: u32,
    pub shieldbattery_version: String,
    pub team_game_main_players: [u8; 4],
    pub starting_races: [Race; 12],
    pub game_id: Uuid,
    pub user_ids: [u32; 8],
    pub game_logic_version: Option<u16>,
}

impl From<broodrep::ShieldBatteryData> for ShieldBatteryData {
    fn from(data: broodrep::ShieldBatteryData) -> Self {
        ShieldBatteryData {
            starcraft_exe_build: data.starcraft_exe_build,
            shieldbattery_version: data.shieldbattery_version.to_string(),
            team_game_main_players: data.team_game_main_players,
            starting_races: data.starting_races.map(Into::into),
            game_id: Uuid::from_u128(data.game_id),
            user_ids: data.user_ids,
            game_logic_version: data.game_logic_version,
        }
    }
}

/// A replay command with its frame number, player, and command data.
#[derive(Clone, Debug, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi)]
pub struct ReplayCommand {
    pub frame: u32,
    pub player_id: u8,
    pub command: CommandData,
}

/// The data for a specific command type.
#[derive(Clone, Debug, Tsify, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
#[tsify(into_wasm_abi)]
pub enum CommandData {
    #[serde(rename_all = "camelCase")]
    Select {
        unit_tags: Vec<u32>,
    },
    #[serde(rename_all = "camelCase")]
    SelectAdd {
        unit_tags: Vec<u32>,
    },
    #[serde(rename_all = "camelCase")]
    SelectRemove {
        unit_tags: Vec<u32>,
    },
    #[serde(rename_all = "camelCase")]
    RightClick {
        x: u16,
        y: u16,
        target_unit_tag: u32,
        target_unit_type: u16,
        queued: bool,
    },
    #[serde(rename_all = "camelCase")]
    TargetedOrder {
        x: u16,
        y: u16,
        target_unit_tag: u32,
        target_unit_type: u16,
        order: u8,
        queued: bool,
    },
    #[serde(rename_all = "camelCase")]
    Build {
        order: u8,
        x: u16,
        y: u16,
        unit_type: u16,
    },
    #[serde(rename_all = "camelCase")]
    Train {
        unit_type: u16,
    },
    #[serde(rename_all = "camelCase")]
    UnitMorph {
        unit_type: u16,
    },
    #[serde(rename_all = "camelCase")]
    BuildingMorph {
        unit_type: u16,
    },
    #[serde(rename_all = "camelCase")]
    CancelTrain {
        unit_tag: u16,
    },
    Stop {
        queued: bool,
    },
    HoldPosition {
        queued: bool,
    },
    Burrow {
        queued: bool,
    },
    Unburrow {
        queued: bool,
    },
    Cloak {
        queued: bool,
    },
    Decloak {
        queued: bool,
    },
    Siege {
        queued: bool,
    },
    Unsiege {
        queued: bool,
    },
    ReturnCargo {
        queued: bool,
    },
    UnloadAll {
        queued: bool,
    },
    #[serde(rename_all = "camelCase")]
    Unload {
        unit_tag: u32,
    },
    #[serde(rename_all = "camelCase")]
    Hotkey {
        hotkey_type: u8,
        group: u8,
    },
    #[serde(rename_all = "camelCase")]
    Tech {
        tech_id: u8,
    },
    #[serde(rename_all = "camelCase")]
    Upgrade {
        upgrade_id: u8,
    },
    #[serde(rename_all = "camelCase")]
    LiftOff {
        x: u16,
        y: u16,
    },
    #[serde(rename_all = "camelCase")]
    MinimapPing {
        x: u16,
        y: u16,
    },
    #[serde(rename_all = "camelCase")]
    Chat {
        sender_slot: u8,
        message: String,
    },
    #[serde(rename_all = "camelCase")]
    LeaveGame {
        reason: u8,
    },
    #[serde(rename_all = "camelCase")]
    GameSpeed {
        speed: u8,
    },
    #[serde(rename_all = "camelCase")]
    Cheat {
        flags: u32,
    },
    #[serde(rename_all = "camelCase")]
    Vision {
        flags: u16,
    },
    #[serde(rename_all = "camelCase")]
    Alliance {
        flags: u32,
    },
    #[serde(rename_all = "camelCase")]
    Latency {
        latency: u8,
    },
    TrainFighter,
    MergeArchon,
    MergeDarkArchon,
    CancelBuild,
    CancelMorph,
    CancelAddon,
    CancelTech,
    CancelUpgrade,
    CancelNuke,
    Stim,
    CarrierStop,
    ReaverStop,
    OrderNothing,
    Pause,
    Resume,
    KeepAlive,
    /// A command with a known type ID but stored as raw data.
    #[serde(rename_all = "camelCase")]
    Known {
        type_id: u8,
        data: Vec<u8>,
    },
    /// An unrecognized command type.
    #[serde(rename_all = "camelCase")]
    Unknown {
        type_id: u8,
        data: Vec<u8>,
    },
}

impl From<broodrep::ReplayCommand> for ReplayCommand {
    fn from(cmd: broodrep::ReplayCommand) -> Self {
        ReplayCommand {
            frame: cmd.frame,
            player_id: cmd.player_id,
            command: cmd.command.into(),
        }
    }
}

impl From<broodrep::Command> for CommandData {
    fn from(cmd: broodrep::Command) -> Self {
        match cmd {
            // Merge regular and 121 select variants (widening u16 to u32)
            broodrep::Command::Select { unit_tags } => CommandData::Select {
                unit_tags: unit_tags.into_iter().map(u32::from).collect(),
            },
            broodrep::Command::Select121 { unit_tags } => CommandData::Select {
                unit_tags: unit_tags.into_iter().map(u32::from).collect(),
            },
            broodrep::Command::SelectAdd { unit_tags } => CommandData::SelectAdd {
                unit_tags: unit_tags.into_iter().map(u32::from).collect(),
            },
            broodrep::Command::SelectAdd121 { unit_tags } => CommandData::SelectAdd {
                unit_tags: unit_tags.into_iter().map(u32::from).collect(),
            },
            broodrep::Command::SelectRemove { unit_tags } => CommandData::SelectRemove {
                unit_tags: unit_tags.into_iter().map(u32::from).collect(),
            },
            broodrep::Command::SelectRemove121 { unit_tags } => CommandData::SelectRemove {
                unit_tags: unit_tags.into_iter().map(u32::from).collect(),
            },
            // Merge regular and 121 order variants
            broodrep::Command::RightClick {
                x,
                y,
                target_unit_tag,
                target_unit_type,
                queued,
            } => CommandData::RightClick {
                x,
                y,
                target_unit_tag: target_unit_tag as u32,
                target_unit_type,
                queued,
            },
            broodrep::Command::RightClick121 {
                x,
                y,
                target_unit_tag,
                target_unit_type,
                queued,
                ..
            } => CommandData::RightClick {
                x,
                y,
                target_unit_tag: target_unit_tag as u32,
                target_unit_type,
                queued,
            },
            broodrep::Command::TargetedOrder {
                x,
                y,
                target_unit_tag,
                target_unit_type,
                order,
                queued,
            } => CommandData::TargetedOrder {
                x,
                y,
                target_unit_tag: target_unit_tag as u32,
                target_unit_type,
                order,
                queued,
            },
            broodrep::Command::TargetedOrder121 {
                x,
                y,
                target_unit_tag,
                target_unit_type,
                order,
                queued,
                ..
            } => CommandData::TargetedOrder {
                x,
                y,
                target_unit_tag: target_unit_tag as u32,
                target_unit_type,
                order,
                queued,
            },
            broodrep::Command::Build {
                order,
                x,
                y,
                unit_type,
            } => CommandData::Build {
                order,
                x,
                y,
                unit_type,
            },
            broodrep::Command::Train { unit_type } => CommandData::Train { unit_type },
            broodrep::Command::UnitMorph { unit_type } => CommandData::UnitMorph { unit_type },
            broodrep::Command::BuildingMorph { unit_type } => {
                CommandData::BuildingMorph { unit_type }
            }
            broodrep::Command::CancelTrain { unit_tag } => CommandData::CancelTrain { unit_tag },
            broodrep::Command::Stop { queued } => CommandData::Stop { queued },
            broodrep::Command::HoldPosition { queued } => CommandData::HoldPosition { queued },
            broodrep::Command::Burrow { queued } => CommandData::Burrow { queued },
            broodrep::Command::Unburrow { queued } => CommandData::Unburrow { queued },
            broodrep::Command::Cloak { queued } => CommandData::Cloak { queued },
            broodrep::Command::Decloak { queued } => CommandData::Decloak { queued },
            broodrep::Command::Siege { queued } => CommandData::Siege { queued },
            broodrep::Command::Unsiege { queued } => CommandData::Unsiege { queued },
            broodrep::Command::ReturnCargo { queued } => CommandData::ReturnCargo { queued },
            broodrep::Command::UnloadAll { queued } => CommandData::UnloadAll { queued },
            broodrep::Command::Unload { unit_tag } => CommandData::Unload {
                unit_tag: unit_tag as u32,
            },
            broodrep::Command::Unload121 { unit_tag, .. } => CommandData::Unload {
                unit_tag: unit_tag as u32,
            },
            broodrep::Command::Hotkey { hotkey_type, group } => {
                CommandData::Hotkey { hotkey_type, group }
            }
            broodrep::Command::Tech { tech_id } => CommandData::Tech { tech_id },
            broodrep::Command::Upgrade { upgrade_id } => CommandData::Upgrade { upgrade_id },
            broodrep::Command::LiftOff { x, y } => CommandData::LiftOff { x, y },
            broodrep::Command::MinimapPing { x, y } => CommandData::MinimapPing { x, y },
            broodrep::Command::Chat {
                sender_slot,
                message,
            } => CommandData::Chat {
                sender_slot,
                message,
            },
            broodrep::Command::LeaveGame { reason } => CommandData::LeaveGame { reason },
            broodrep::Command::GameSpeed { speed } => CommandData::GameSpeed { speed },
            broodrep::Command::Cheat { flags } => CommandData::Cheat { flags },
            broodrep::Command::Vision { flags } => CommandData::Vision { flags },
            broodrep::Command::Alliance { flags } => CommandData::Alliance { flags },
            broodrep::Command::Latency { latency } => CommandData::Latency { latency },
            broodrep::Command::TrainFighter => CommandData::TrainFighter,
            broodrep::Command::MergeArchon => CommandData::MergeArchon,
            broodrep::Command::MergeDarkArchon => CommandData::MergeDarkArchon,
            broodrep::Command::CancelBuild => CommandData::CancelBuild,
            broodrep::Command::CancelMorph => CommandData::CancelMorph,
            broodrep::Command::CancelAddon => CommandData::CancelAddon,
            broodrep::Command::CancelTech => CommandData::CancelTech,
            broodrep::Command::CancelUpgrade => CommandData::CancelUpgrade,
            broodrep::Command::CancelNuke => CommandData::CancelNuke,
            broodrep::Command::Stim => CommandData::Stim,
            broodrep::Command::CarrierStop => CommandData::CarrierStop,
            broodrep::Command::ReaverStop => CommandData::ReaverStop,
            broodrep::Command::OrderNothing => CommandData::OrderNothing,
            broodrep::Command::Pause => CommandData::Pause,
            broodrep::Command::Resume => CommandData::Resume,
            broodrep::Command::KeepAlive => CommandData::KeepAlive,
            broodrep::Command::Known { type_id, data } => CommandData::Known { type_id, data },
            broodrep::Command::Unknown { type_id, data } => CommandData::Unknown { type_id, data },
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

    /// Returns the parsed ShieldBattery section, or `undefined` if not present in the replay.
    #[wasm_bindgen(js_name = getShieldBatterySection)]
    pub fn get_shieldbattery_section(&mut self) -> Result<Option<ShieldBatteryData>, JsValue> {
        Ok(self
            .replay
            .get_shieldbattery_section()
            .map_err(|e| JsValue::from_str(&e.to_string()))?
            .map(Into::into))
    }

    /// Returns the parsed commands from the replay, or `undefined` if the commands section is not
    /// present.
    #[wasm_bindgen(js_name = getCommands)]
    pub fn get_commands(&mut self) -> Result<JsValue, JsValue> {
        let commands = self
            .replay
            .get_commands()
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        match commands {
            Some(cmds) => {
                let wasm_cmds: Vec<ReplayCommand> = cmds.into_iter().map(Into::into).collect();
                serde_wasm_bindgen::to_value(&wasm_cmds)
                    .map_err(|e| JsValue::from_str(&e.to_string()))
            }
            None => Ok(JsValue::UNDEFINED),
        }
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
    const SB_DATA_REPLAY: &[u8] = include_bytes!("../../broodrep/testdata/sb_data.rep");
    const SCR_EMPTY_COMMANDS_REPLAY: &[u8] =
        include_bytes!("../../broodrep/testdata/scr_empty_commands.rep");

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
    fn test_shieldbattery_game_id() {
        let data = Uint8Array::from(SB_DATA_REPLAY);
        let mut replay = parse_replay(data, None).unwrap();

        let sb = replay.get_shieldbattery_section().unwrap().unwrap();
        // The UUID string must be in RFC-4122 order, exactly as ShieldBattery wrote it
        assert_eq!(
            sb.game_id.to_string(),
            "019878ca-6a88-7ebb-9b93-20d6e6bd892a"
        );
    }

    #[wasm_bindgen_test]
    fn test_scr_empty_commands_replay() {
        // Replay with no commands section data and the Sbat section at the end of the file; used
        // to lose all modern sections and panic on getRawSection(playerNames)
        let data = Uint8Array::from(SCR_EMPTY_COMMANDS_REPLAY);
        let mut replay = parse_replay(data, None).unwrap();

        let sb = replay.get_shieldbattery_section().unwrap();
        assert!(sb.is_some());

        let names = replay.get_raw_section(ReplaySection::PlayerNames).unwrap();
        assert!(names.is_some_and(|n| !n.is_empty()));

        let commands = replay.get_raw_section(ReplaySection::Commands).unwrap();
        assert_eq!(commands, Some(vec![]));
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
