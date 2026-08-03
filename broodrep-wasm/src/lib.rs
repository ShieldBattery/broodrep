use js_sys::Uint8Array;
use serde::{Deserialize, Serialize};
use std::{io::Cursor, ops::Range};
use tsify::Tsify;
use uuid::Uuid;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_APPEND_CONTENT: &'static str = r#"
export type Uuid = string;
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "CommandQuery")]
    pub type CommandQueryInput;

    #[wasm_bindgen(typescript_type = "CommandParseConfig")]
    pub type CommandParseConfigInput;
}

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

/// Command parsing limits. Omitted fields use broodrep's safe defaults: 250,000 commands and
/// 16 MiB of dynamically owned command data.
///
/// Use these limits when loading commands or building a command summary from untrusted replay
/// data. The limits apply to parsing in WASM before any command data is exposed to JavaScript.
#[derive(Clone, Debug, Default, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tsify(from_wasm_abi)]
pub struct CommandParseConfig {
    /// Maximum commands to parse (default: 250,000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_commands: Option<u32>,

    /// Maximum dynamically owned command-data bytes to retain (default: 16 MiB).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_owned_data_bytes: Option<u32>,
}

impl From<CommandParseConfig> for broodrep::CommandParseConfig {
    fn from(options: CommandParseConfig) -> Self {
        let defaults = broodrep::CommandParseConfig::default();
        broodrep::CommandParseConfig {
            max_commands: options
                .max_commands
                .map_or(defaults.max_commands, |value| value as usize),
            max_owned_data_bytes: options
                .max_owned_data_bytes
                .map_or(defaults.max_owned_data_bytes, |value| value as usize),
        }
    }
}

/// A normalized command kind. Legacy and 1.21+ encodings of the same logical command share a
/// kind, so queries do not need to account for replay-format-specific type IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum CommandKind {
    Select,
    SelectAdd,
    SelectRemove,
    RightClick,
    TargetedOrder,
    Build,
    CancelBuild,
    CancelAddon,
    LiftOff,
    Train,
    CancelTrain,
    UnitMorph,
    BuildingMorph,
    CancelMorph,
    TrainFighter,
    Stop,
    HoldPosition,
    Burrow,
    Unburrow,
    Cloak,
    Decloak,
    Siege,
    Unsiege,
    ReturnCargo,
    UnloadAll,
    Unload,
    MergeArchon,
    MergeDarkArchon,
    CancelNuke,
    Stim,
    CarrierStop,
    ReaverStop,
    OrderNothing,
    Tech,
    CancelTech,
    Upgrade,
    CancelUpgrade,
    Hotkey,
    Vision,
    Alliance,
    GameSpeed,
    Pause,
    Resume,
    Cheat,
    Chat,
    KeepAlive,
    LeaveGame,
    MinimapPing,
    Latency,
    Known,
    Unknown,
}

/// A broad, mutually exclusive command category used by command queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub enum CommandCategory {
    Selection,
    Order,
    Production,
    Ability,
    Research,
    Hotkey,
    Diplomacy,
    GameControl,
    Communication,
    Network,
    PlayerStatus,
    Unknown,
}

/// Declarative filters for selecting commands.
///
/// Player and frame constraints are intersected. If any inclusion list is present, a command must
/// match at least one included kind or category. Exclusions are then applied and always win.
#[derive(Clone, Debug, Default, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tsify(from_wasm_abi)]
pub struct CommandQuery {
    /// Network player IDs to include, matching [`Player::network_id`] rather than slot IDs. A
    /// present but empty list matches no commands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player_ids: Option<Vec<u8>>,

    /// First included game frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_frame: Option<u32>,

    /// First excluded game frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_frame: Option<u32>,

    /// Exact normalized kinds to include.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_kinds: Option<Vec<CommandKind>>,

    /// Broad categories to include. Kind and category inclusion lists are unioned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_categories: Option<Vec<CommandCategory>>,

    /// Raw wire-format type IDs to include. This is primarily useful for `known` and `unknown`
    /// commands, or when intentionally distinguishing legacy and 1.21+ encodings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_type_ids: Option<Vec<u8>>,

    /// Exact normalized kinds to exclude.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude_kinds: Option<Vec<CommandKind>>,

    /// Broad categories to exclude. Exclusion always wins over inclusion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude_categories: Option<Vec<CommandCategory>>,

    /// Raw wire-format type IDs to exclude.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude_type_ids: Option<Vec<u8>>,
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
            shieldbattery_version: data.shieldbattery_version,
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
    /// Network player ID matching [`Player::network_id`], not [`Player::slot_id`].
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

/// A command list retained in WASM memory.
///
/// Obtain this with [`Replay::load_commands`]. It avoids immediately creating a JavaScript object
/// for every command; [`ParsedCommands::get_range`] copies and serializes only the requested page.
#[wasm_bindgen]
pub struct ParsedCommands {
    commands: Vec<broodrep::ReplayCommand>,
    frames: u32,
    duration_minutes: f64,
}

#[wasm_bindgen]
impl ParsedCommands {
    /// Number of commands retained by this owner.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> usize {
        self.commands.len()
    }

    /// Whether this owner contains no commands.
    #[wasm_bindgen(js_name = isEmpty)]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Returns one page of commands as JavaScript objects.
    ///
    /// `start` and `count` must describe a range entirely within this owner. The returned commands
    /// are copied from WASM memory and converted to JavaScript; the other retained commands are not
    /// converted or copied. An invalid range throws a `RangeError`.
    #[wasm_bindgen(js_name = getRange, unchecked_return_type = "ReplayCommand[]")]
    pub fn get_range(&self, start: f64, count: f64) -> Result<JsValue, JsValue> {
        let range = checked_range(start, count, self.commands.len()).map_err(range_error)?;
        commands_to_js(self.commands[range].iter().cloned())
    }

    /// Returns commands matching a declarative query as JavaScript objects.
    ///
    /// This reuses the commands retained in WASM memory and serializes only matches. Results retain
    /// replay order. Invalid frame ranges throw a `RangeError`.
    #[wasm_bindgen(unchecked_return_type = "ReplayCommand[]")]
    pub fn query(&self, query: CommandQueryInput) -> Result<JsValue, JsValue> {
        let query = command_query_from_js(query)?;
        let query = CompiledCommandQuery::new(query).map_err(range_error)?;
        commands_to_js(
            self.commands
                .iter()
                .filter(|command| query.matches(command))
                .cloned(),
        )
    }

    /// Calculates raw actions-per-minute from the retained commands without creating JavaScript
    /// command objects.
    #[wasm_bindgen(js_name = getPlayerApm)]
    pub fn get_player_apm(&self) -> PlayerApmSummary {
        player_apm_summary(
            self.frames,
            self.duration_minutes,
            count_apm_actions(self.commands.iter()),
        )
    }
}

/// Raw section bytes retained in WASM memory.
///
/// Obtain this with [`Replay::load_raw_section`] or [`Replay::load_raw_custom_section`]. Calling
/// [`RawSection::copy_range`] creates a new JavaScript `Uint8Array` only for the requested range.
#[wasm_bindgen]
pub struct RawSection {
    bytes: Vec<u8>,
}

#[wasm_bindgen]
impl RawSection {
    /// Number of bytes retained by this owner.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> usize {
        self.bytes.len()
    }

    /// Whether this owner contains no bytes.
    #[wasm_bindgen(js_name = isEmpty)]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Copies a byte range into a new JavaScript `Uint8Array`.
    ///
    /// `start` and `count` must describe a range entirely within this owner. An invalid range
    /// throws a `RangeError`.
    #[wasm_bindgen(js_name = copyRange)]
    pub fn copy_range(&self, start: f64, count: f64) -> Result<Uint8Array, JsValue> {
        let range = checked_range(start, count, self.bytes.len()).map_err(range_error)?;
        Ok(Uint8Array::from(&self.bytes[range]))
    }
}

/// The count for one raw command type ID.
#[derive(Clone, Debug, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi)]
pub struct CommandTypeCount {
    /// Raw command type ID from the replay stream.
    pub type_id: u8,
    /// Number of commands with this type ID.
    pub count: u32,
}

/// A compact command-count summary.
///
/// `counts` is ordered by ascending numeric `typeId` and omits command types that do not occur.
#[derive(Clone, Debug, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi)]
pub struct CommandSummary {
    /// Total number of commands in the command section.
    pub total: u32,
    /// Per-type counts in ascending numeric type-ID order.
    pub counts: Vec<CommandTypeCount>,
}

/// Raw actions-per-minute for one replay player ID.
#[derive(Clone, Debug, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi)]
pub struct PlayerApm {
    /// Network player ID matching [`Player::network_id`], not [`Player::slot_id`].
    pub player_id: u8,
    pub actions: u32,
    pub apm: f64,
}

/// Per-player raw APM calculated over the replay's complete duration.
#[derive(Clone, Debug, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi)]
pub struct PlayerApmSummary {
    pub frames: u32,
    pub duration_minutes: f64,
    pub players: Vec<PlayerApm>,
}

/// An eagerly copied replay snapshot that owns no replay bytes.
///
/// [`parse_replay_metadata`] returns this when an application only needs basic replay information
/// and should release the original replay byte buffer immediately.
#[derive(Clone, Debug, Tsify, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[tsify(into_wasm_abi)]
pub struct ReplayMetadata {
    pub format: ReplayFormat,
    pub header: ReplayHeader,
    pub slots: Vec<Player>,
}

const COMMAND_KIND_COUNT: usize = CommandKind::Unknown as usize + 1;
const COMMAND_CATEGORY_COUNT: usize = CommandCategory::Unknown as usize + 1;

impl From<broodrep::CommandKind> for CommandKind {
    fn from(kind: broodrep::CommandKind) -> Self {
        match kind {
            broodrep::CommandKind::Select => Self::Select,
            broodrep::CommandKind::SelectAdd => Self::SelectAdd,
            broodrep::CommandKind::SelectRemove => Self::SelectRemove,
            broodrep::CommandKind::RightClick => Self::RightClick,
            broodrep::CommandKind::TargetedOrder => Self::TargetedOrder,
            broodrep::CommandKind::Build => Self::Build,
            broodrep::CommandKind::CancelBuild => Self::CancelBuild,
            broodrep::CommandKind::CancelAddon => Self::CancelAddon,
            broodrep::CommandKind::LiftOff => Self::LiftOff,
            broodrep::CommandKind::Train => Self::Train,
            broodrep::CommandKind::CancelTrain => Self::CancelTrain,
            broodrep::CommandKind::UnitMorph => Self::UnitMorph,
            broodrep::CommandKind::BuildingMorph => Self::BuildingMorph,
            broodrep::CommandKind::CancelMorph => Self::CancelMorph,
            broodrep::CommandKind::TrainFighter => Self::TrainFighter,
            broodrep::CommandKind::Stop => Self::Stop,
            broodrep::CommandKind::HoldPosition => Self::HoldPosition,
            broodrep::CommandKind::Burrow => Self::Burrow,
            broodrep::CommandKind::Unburrow => Self::Unburrow,
            broodrep::CommandKind::Cloak => Self::Cloak,
            broodrep::CommandKind::Decloak => Self::Decloak,
            broodrep::CommandKind::Siege => Self::Siege,
            broodrep::CommandKind::Unsiege => Self::Unsiege,
            broodrep::CommandKind::ReturnCargo => Self::ReturnCargo,
            broodrep::CommandKind::UnloadAll => Self::UnloadAll,
            broodrep::CommandKind::Unload => Self::Unload,
            broodrep::CommandKind::MergeArchon => Self::MergeArchon,
            broodrep::CommandKind::MergeDarkArchon => Self::MergeDarkArchon,
            broodrep::CommandKind::CancelNuke => Self::CancelNuke,
            broodrep::CommandKind::Stim => Self::Stim,
            broodrep::CommandKind::CarrierStop => Self::CarrierStop,
            broodrep::CommandKind::ReaverStop => Self::ReaverStop,
            broodrep::CommandKind::OrderNothing => Self::OrderNothing,
            broodrep::CommandKind::Tech => Self::Tech,
            broodrep::CommandKind::CancelTech => Self::CancelTech,
            broodrep::CommandKind::Upgrade => Self::Upgrade,
            broodrep::CommandKind::CancelUpgrade => Self::CancelUpgrade,
            broodrep::CommandKind::Hotkey => Self::Hotkey,
            broodrep::CommandKind::Vision => Self::Vision,
            broodrep::CommandKind::Alliance => Self::Alliance,
            broodrep::CommandKind::GameSpeed => Self::GameSpeed,
            broodrep::CommandKind::Pause => Self::Pause,
            broodrep::CommandKind::Resume => Self::Resume,
            broodrep::CommandKind::Cheat => Self::Cheat,
            broodrep::CommandKind::Chat => Self::Chat,
            broodrep::CommandKind::KeepAlive => Self::KeepAlive,
            broodrep::CommandKind::LeaveGame => Self::LeaveGame,
            broodrep::CommandKind::MinimapPing => Self::MinimapPing,
            broodrep::CommandKind::Latency => Self::Latency,
            broodrep::CommandKind::Known => Self::Known,
            broodrep::CommandKind::Unknown => Self::Unknown,
        }
    }
}

impl From<broodrep::CommandCategory> for CommandCategory {
    fn from(category: broodrep::CommandCategory) -> Self {
        match category {
            broodrep::CommandCategory::Selection => Self::Selection,
            broodrep::CommandCategory::Order => Self::Order,
            broodrep::CommandCategory::Production => Self::Production,
            broodrep::CommandCategory::Ability => Self::Ability,
            broodrep::CommandCategory::Research => Self::Research,
            broodrep::CommandCategory::Hotkey => Self::Hotkey,
            broodrep::CommandCategory::Diplomacy => Self::Diplomacy,
            broodrep::CommandCategory::GameControl => Self::GameControl,
            broodrep::CommandCategory::Communication => Self::Communication,
            broodrep::CommandCategory::Network => Self::Network,
            broodrep::CommandCategory::PlayerStatus => Self::PlayerStatus,
            broodrep::CommandCategory::Unknown => Self::Unknown,
        }
    }
}

struct CompiledCommandQuery {
    players: Option<[bool; 256]>,
    start_frame: Option<u32>,
    end_frame: Option<u32>,
    has_inclusions: bool,
    included_kinds: [bool; COMMAND_KIND_COUNT],
    included_categories: [bool; COMMAND_CATEGORY_COUNT],
    included_type_ids: [bool; 256],
    excluded_kinds: [bool; COMMAND_KIND_COUNT],
    excluded_categories: [bool; COMMAND_CATEGORY_COUNT],
    excluded_type_ids: [bool; 256],
}

impl CompiledCommandQuery {
    fn new(query: CommandQuery) -> Result<Self, &'static str> {
        if query
            .start_frame
            .zip(query.end_frame)
            .is_some_and(|(start, end)| start > end)
        {
            return Err("startFrame must not exceed endFrame");
        }
        if query
            .player_ids
            .as_ref()
            .is_some_and(|values| values.len() > 256)
        {
            return Err("playerIds must contain at most 256 entries");
        }
        if query
            .include_kinds
            .as_ref()
            .is_some_and(|values| values.len() > COMMAND_KIND_COUNT)
            || query
                .exclude_kinds
                .as_ref()
                .is_some_and(|values| values.len() > COMMAND_KIND_COUNT)
        {
            return Err("command kind filters contain too many entries");
        }
        if query
            .include_categories
            .as_ref()
            .is_some_and(|values| values.len() > COMMAND_CATEGORY_COUNT)
            || query
                .exclude_categories
                .as_ref()
                .is_some_and(|values| values.len() > COMMAND_CATEGORY_COUNT)
        {
            return Err("command category filters contain too many entries");
        }
        if query
            .include_type_ids
            .as_ref()
            .is_some_and(|values| values.len() > 256)
            || query
                .exclude_type_ids
                .as_ref()
                .is_some_and(|values| values.len() > 256)
        {
            return Err("command type ID filters contain too many entries");
        }

        let players = query.player_ids.map(|player_ids| {
            let mut selected = [false; 256];
            for player_id in player_ids {
                selected[usize::from(player_id)] = true;
            }
            selected
        });
        let has_inclusions = query.include_kinds.is_some()
            || query.include_categories.is_some()
            || query.include_type_ids.is_some();
        let mut included_kinds = [false; COMMAND_KIND_COUNT];
        let mut included_categories = [false; COMMAND_CATEGORY_COUNT];
        let mut included_type_ids = [false; 256];
        let mut excluded_kinds = [false; COMMAND_KIND_COUNT];
        let mut excluded_categories = [false; COMMAND_CATEGORY_COUNT];
        let mut excluded_type_ids = [false; 256];

        for kind in query.include_kinds.into_iter().flatten() {
            included_kinds[kind as usize] = true;
        }
        for category in query.include_categories.into_iter().flatten() {
            included_categories[category as usize] = true;
        }
        for type_id in query.include_type_ids.into_iter().flatten() {
            included_type_ids[usize::from(type_id)] = true;
        }
        for kind in query.exclude_kinds.into_iter().flatten() {
            excluded_kinds[kind as usize] = true;
        }
        for category in query.exclude_categories.into_iter().flatten() {
            excluded_categories[category as usize] = true;
        }
        for type_id in query.exclude_type_ids.into_iter().flatten() {
            excluded_type_ids[usize::from(type_id)] = true;
        }

        Ok(Self {
            players,
            start_frame: query.start_frame,
            end_frame: query.end_frame,
            has_inclusions,
            included_kinds,
            included_categories,
            included_type_ids,
            excluded_kinds,
            excluded_categories,
            excluded_type_ids,
        })
    }

    fn matches(&self, command: &broodrep::ReplayCommand) -> bool {
        if self
            .players
            .as_ref()
            .is_some_and(|players| !players[usize::from(command.player_id)])
            || self.start_frame.is_some_and(|start| command.frame < start)
            || self.end_frame.is_some_and(|end| command.frame >= end)
        {
            return false;
        }

        let core_kind = command.command.kind();
        let type_id = usize::from(command.command.type_id());
        let kind = CommandKind::from(core_kind) as usize;
        let category = CommandCategory::from(core_kind.category()) as usize;
        let included = !self.has_inclusions
            || self.included_kinds[kind]
            || self.included_categories[category]
            || self.included_type_ids[type_id];
        included
            && !self.excluded_kinds[kind]
            && !self.excluded_categories[category]
            && !self.excluded_type_ids[type_id]
    }
}

fn checked_range(start: f64, count: f64, len: usize) -> Result<Range<usize>, &'static str> {
    let start = js_index(start)?;
    let count = js_index(count)?;
    let end = start.checked_add(count).ok_or("range end overflows")?;

    if start > len || end > len {
        return Err("range is outside the retained data");
    }

    Ok(start..end)
}

fn js_index(value: f64) -> Result<usize, &'static str> {
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 {
        return Err("range offsets and counts must be finite, non-negative integers");
    }
    if value > f64::from(u32::MAX) {
        return Err("range offsets and counts must not exceed 4,294,967,295");
    }

    Ok(value as u32 as usize)
}

fn range_error(message: &'static str) -> JsValue {
    js_sys::RangeError::new(message).into()
}

fn error(message: &'static str) -> JsValue {
    JsValue::from_str(message)
}

fn command_query_from_js(value: CommandQueryInput) -> Result<CommandQuery, JsValue> {
    serde_wasm_bindgen::from_value(value.into())
        .map_err(|error| JsValue::from_str(&format!("invalid command query: {error}")))
}

fn command_parse_config_from_js(
    value: Option<CommandParseConfigInput>,
) -> Result<broodrep::CommandParseConfig, JsValue> {
    let Some(value) = value.map(JsValue::from) else {
        return Ok(broodrep::CommandParseConfig::default());
    };
    serde_wasm_bindgen::from_value::<CommandParseConfig>(value)
        .map(Into::into)
        .map_err(|error| JsValue::from_str(&format!("invalid command parse config: {error}")))
}

fn commands_to_wasm(
    commands: impl IntoIterator<Item = broodrep::ReplayCommand>,
) -> Vec<ReplayCommand> {
    commands.into_iter().map(Into::into).collect()
}

fn commands_to_js(
    commands: impl IntoIterator<Item = broodrep::ReplayCommand>,
) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(&commands_to_wasm(commands))
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

fn command_summary_from_counts(
    total: usize,
    counts: [usize; 256],
) -> Result<CommandSummary, &'static str> {
    let total = u32::try_from(total).map_err(|_| "command total exceeds JavaScript range")?;
    let counts = counts
        .into_iter()
        .enumerate()
        .filter(|(_, count)| *count != 0)
        .map(|(type_id, count)| {
            Ok(CommandTypeCount {
                type_id: type_id as u8,
                count: u32::try_from(count)
                    .map_err(|_| "command type count exceeds JavaScript range")?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(CommandSummary { total, counts })
}

fn count_apm_actions<'a>(
    commands: impl IntoIterator<Item = &'a broodrep::ReplayCommand>,
) -> [u32; 256] {
    let mut counts = [0u32; 256];
    for command in commands {
        if command.command.kind().counts_as_apm_action() {
            counts[usize::from(command.player_id)] += 1;
        }
    }
    counts
}

fn player_apm_summary(frames: u32, duration_minutes: f64, counts: [u32; 256]) -> PlayerApmSummary {
    let players = counts
        .into_iter()
        .enumerate()
        .filter(|(_, actions)| *actions != 0)
        .map(|(player_id, actions)| PlayerApm {
            player_id: player_id as u8,
            actions,
            apm: if duration_minutes == 0.0 {
                0.0
            } else {
                f64::from(actions) / duration_minutes
            },
        })
        .collect();
    PlayerApmSummary {
        frames,
        duration_minutes,
        players,
    }
}

fn duration_minutes(replay: &broodrep::Replay<Cursor<Vec<u8>>>) -> f64 {
    (replay.game_speed().time_per_step() * replay.frames()).as_secs_f64() / 60.0
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

    /// Loads and retains parsed commands in WASM memory, or returns `undefined` if the replay has
    /// no commands section.
    ///
    /// This is an explicit caller-owned cache: the library does not retain the commands on
    /// `Replay` itself. Use [`ParsedCommands::get_range`] to transfer only the pages needed by
    /// JavaScript.
    #[wasm_bindgen(js_name = loadCommands)]
    pub fn load_commands(
        &mut self,
        options: Option<CommandParseConfigInput>,
    ) -> Result<Option<ParsedCommands>, JsValue> {
        let frames = self.replay.frames();
        let duration_minutes = duration_minutes(&self.replay);
        let config = command_parse_config_from_js(options)?;
        self.replay
            .read_commands_with_config(config)
            .map(|commands| {
                commands.map(|commands| ParsedCommands {
                    commands,
                    frames,
                    duration_minutes,
                })
            })
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }

    /// Returns commands matching a declarative query, or `undefined` if the replay has no command
    /// section.
    ///
    /// Commands are scanned in replay order, but only matches are retained and converted to
    /// JavaScript objects. Player and frame constraints are intersected; kind and category
    /// inclusions are unioned; exclusions are applied last and always win.
    #[wasm_bindgen(
        js_name = queryCommands,
        unchecked_return_type = "ReplayCommand[] | undefined"
    )]
    pub fn query_commands(
        &mut self,
        query: CommandQueryInput,
        options: Option<CommandParseConfigInput>,
    ) -> Result<JsValue, JsValue> {
        let query = command_query_from_js(query)?;
        let query = CompiledCommandQuery::new(query).map_err(range_error)?;
        let config = command_parse_config_from_js(options)?;
        let mut commands = Vec::new();
        let outcome = self
            .replay
            .visit_commands_with_config(config, |command| {
                if query.matches(&command) {
                    commands.push(command);
                }
                std::ops::ControlFlow::Continue(())
            })
            .map_err(|error| JsValue::from_str(&error.to_string()))?;

        match outcome {
            Some(_) => commands_to_js(commands),
            None => Ok(JsValue::UNDEFINED),
        }
    }

    /// Returns compact counts for command type IDs without retaining or serializing a complete
    /// command list, or `undefined` if the replay has no commands section.
    ///
    /// The command section is still decompressed for this call. Use `loadCommands` when commands
    /// themselves are needed more than once.
    #[wasm_bindgen(js_name = getCommandSummary)]
    pub fn get_command_summary(
        &mut self,
        options: Option<CommandParseConfigInput>,
    ) -> Result<Option<CommandSummary>, JsValue> {
        let mut total = 0usize;
        let mut counts = [0usize; 256];
        let config = command_parse_config_from_js(options)?;
        let outcome = self
            .replay
            .visit_commands_with_config(config, |command| {
                total += 1;
                counts[usize::from(command.command.type_id())] += 1;
                std::ops::ControlFlow::Continue(())
            })
            .map_err(|error| JsValue::from_str(&error.to_string()))?;

        outcome
            .map(|_| command_summary_from_counts(total, counts).map_err(error))
            .transpose()
    }

    /// Calculates per-player raw actions-per-minute without retaining or serializing individual
    /// command objects, or returns `undefined` if the replay has no command section.
    ///
    /// Raw APM counts selections, orders, production, abilities, research, hotkeys, diplomacy,
    /// and minimap pings over the replay's complete duration. It excludes game-control commands,
    /// chat, network traffic, player-status records, and untyped commands. It does not apply the
    /// redundancy filtering associated with effective-APM metrics.
    #[wasm_bindgen(js_name = getPlayerApm)]
    pub fn get_player_apm(
        &mut self,
        options: Option<CommandParseConfigInput>,
    ) -> Result<Option<PlayerApmSummary>, JsValue> {
        let frames = self.replay.frames();
        let duration_minutes = duration_minutes(&self.replay);
        let mut counts = [0u32; 256];
        let config = command_parse_config_from_js(options)?;
        let outcome = self
            .replay
            .visit_commands_with_config(config, |command| {
                if command.command.kind().counts_as_apm_action() {
                    counts[usize::from(command.player_id)] += 1;
                }
                std::ops::ControlFlow::Continue(())
            })
            .map_err(|error| JsValue::from_str(&error.to_string()))?;

        Ok(outcome.map(|_| player_apm_summary(frames, duration_minutes, counts)))
    }

    /// Loads a decompressed replay section into a `RawSection` owner, or returns `undefined` if
    /// it is not present. This keeps the full section in WASM memory without copying it into a
    /// JavaScript `Uint8Array`; use [`RawSection::copy_range`] to copy only requested bytes.
    #[wasm_bindgen(js_name = loadRawSection)]
    pub fn load_raw_section(
        &mut self,
        section: ReplaySection,
    ) -> Result<Option<RawSection>, JsValue> {
        self.replay
            .read_raw_section(section.into())
            .map(|bytes| bytes.map(|bytes| RawSection { bytes }))
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }

    /// Loads a raw section identified by a little-endian 32-bit ID into a `RawSection` owner, or
    /// returns `undefined` if it is not present. The bytes remain in WASM memory until the owner is
    /// released.
    #[wasm_bindgen(js_name = loadRawCustomSection)]
    pub fn load_raw_custom_section(
        &mut self,
        section_id: u32,
    ) -> Result<Option<RawSection>, JsValue> {
        self.replay
            .read_raw_section(section_id.to_le_bytes().into())
            .map(|bytes| bytes.map(|bytes| RawSection { bytes }))
            .map_err(|error| JsValue::from_str(&error.to_string()))
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
    #[wasm_bindgen(
        js_name = getCommands,
        unchecked_return_type = "ReplayCommand[] | undefined"
    )]
    pub fn get_commands(&mut self) -> Result<JsValue, JsValue> {
        let commands = self
            .replay
            .read_commands()
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        match commands {
            Some(commands) => commands_to_js(commands),
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
    parse_replay_from_data(data, options).map(Replay::new)
}

/// Parse only metadata from a StarCraft replay and release the replay bytes before returning.
///
/// Unlike [`parse_replay`], this does not return a lazy `Replay` owner. The returned snapshot owns
/// its format, header, and slot data but does not retain the input `Uint8Array` in WASM memory.
#[wasm_bindgen(js_name = parseReplayMetadata)]
pub fn parse_replay_metadata(
    data: Uint8Array,
    options: Option<DecompressionConfig>,
) -> Result<ReplayMetadata, JsValue> {
    let replay = parse_replay_from_data(data, options)?;
    Ok(ReplayMetadata {
        format: replay.format.into(),
        header: replay.header.clone().into(),
        slots: replay.slots().iter().cloned().map(Into::into).collect(),
    })
}

fn parse_replay_from_data(
    data: Uint8Array,
    options: Option<DecompressionConfig>,
) -> Result<broodrep::Replay<Cursor<Vec<u8>>>, JsValue> {
    let bytes: Vec<u8> = data.to_vec();
    let cursor = Cursor::new(bytes);

    let config = options.unwrap_or_default().into();
    broodrep::Replay::new_with_decompression_config(cursor, config)
        .map_err(|error| JsValue::from_str(&format!("Failed to parse replay: {error}")))
}

/// Get version information about the broodrep library.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Initializes the WASM module. With the `console_error_panic_hook` feature enabled, this installs
/// richer panic reporting for JavaScript; without that feature it is a no-op.
#[wasm_bindgen(start)]
pub fn init() {
    // Set up better panic messages for debugging
    #[cfg(feature = "console_error_panic_hook")]
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

    #[wasm_bindgen(inline_js = r#"
        export function malformedReplayQueryDoesNotPoison(replay) {
            let queryThrew = false;
            let configThrew = false;
            try {
                replay.queryCommands({ includeKinds: ['bogus'] });
            } catch (_) {
                queryThrew = true;
            }
            try {
                replay.getPlayerApm({ maxCommands: 'bogus' });
            } catch (_) {
                configThrew = true;
            }
            const valid = replay.queryCommands({ includeKinds: ['select'] });
            replay.free();
            return queryThrew && configThrew && Array.isArray(valid);
        }

        export function malformedRetainedQueryDoesNotPoison(commands) {
            let threw = false;
            try {
                commands.query({ includeKinds: ['bogus'] });
            } catch (_) {
                threw = true;
            }
            const valid = commands.query({ includeKinds: ['select'] });
            commands.free();
            return threw && Array.isArray(valid);
        }
    "#)]
    extern "C" {
        #[wasm_bindgen(js_name = malformedReplayQueryDoesNotPoison)]
        fn malformed_replay_query_does_not_poison(replay: JsValue) -> bool;

        #[wasm_bindgen(js_name = malformedRetainedQueryDoesNotPoison)]
        fn malformed_retained_query_does_not_poison(commands: JsValue) -> bool;
    }

    const LEGACY_REPLAY: &[u8] = include_bytes!("../../broodrep/testdata/things.rep");
    const SCR_121_REPLAY: &[u8] = include_bytes!("../../broodrep/testdata/scr_replay.rep");
    const SB_DATA_REPLAY: &[u8] = include_bytes!("../../broodrep/testdata/sb_data.rep");
    const SCR_EMPTY_COMMANDS_REPLAY: &[u8] =
        include_bytes!("../../broodrep/testdata/scr_empty_commands.rep");
    const LONG_HUNTERS_REPLAY: &[u8] = include_bytes!("../../broodrep/testdata/long_hunters.rep");

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

    #[wasm_bindgen_test]
    fn test_retained_owner_ranges() {
        let data = Uint8Array::from(LEGACY_REPLAY);
        let mut replay = parse_replay(data, None).unwrap();

        let commands = replay.load_commands(None).unwrap().unwrap();
        let first_page = commands.get_range(0.0, 1.0).unwrap();
        assert!(js_sys::Array::is_array(&first_page));
        assert_eq!(js_sys::Array::from(&first_page).length(), 1);
        assert!(commands.get_range(commands.length() as f64, 0.0).is_ok());
        let range_error = commands
            .get_range(commands.length() as f64, 1.0)
            .unwrap_err();
        assert!(range_error.is_instance_of::<js_sys::RangeError>());

        let raw = replay
            .load_raw_section(ReplaySection::Commands)
            .unwrap()
            .unwrap();
        let full = replay
            .get_raw_section(ReplaySection::Commands)
            .unwrap()
            .unwrap();
        let copied = raw.copy_range(0.0, 4.0).unwrap();
        assert_eq!(copied.to_vec(), full[..4]);
        assert!(raw.copy_range(raw.length() as f64, 0.0).is_ok());
        let range_error = raw.copy_range(raw.length() as f64, 1.0).unwrap_err();
        assert!(range_error.is_instance_of::<js_sys::RangeError>());
    }

    #[wasm_bindgen_test]
    fn test_filtered_queries_and_apm() {
        let data = Uint8Array::from(LEGACY_REPLAY);
        let mut replay = parse_replay(data, None).unwrap();
        let query = CommandQuery {
            include_categories: Some(vec![CommandCategory::Selection]),
            exclude_kinds: Some(vec![CommandKind::SelectRemove]),
            ..CommandQuery::default()
        };

        let query_js = serde_wasm_bindgen::to_value(&query).unwrap();
        let filtered = replay
            .query_commands(query_js.unchecked_into(), None)
            .unwrap();
        assert!(js_sys::Array::is_array(&filtered));
        assert!(js_sys::Array::from(&filtered).length() > 0);

        let retained = replay.load_commands(None).unwrap().unwrap();
        let retained_query_js = serde_wasm_bindgen::to_value(&query).unwrap();
        let retained_filtered = retained.query(retained_query_js.unchecked_into()).unwrap();
        assert_eq!(
            js_sys::Array::from(&filtered).length(),
            js_sys::Array::from(&retained_filtered).length()
        );

        let streamed_apm = replay.get_player_apm(None).unwrap().unwrap();
        let retained_apm = retained.get_player_apm();
        assert_eq!(streamed_apm.frames, retained_apm.frames);
        assert_eq!(streamed_apm.players.len(), retained_apm.players.len());
        assert!(streamed_apm.players.iter().all(|player| player.apm > 0.0));
    }

    #[wasm_bindgen_test]
    fn test_malformed_queries_do_not_poison_owner_borrows() {
        let data = Uint8Array::from(LEGACY_REPLAY);
        let replay = parse_replay(data, None).unwrap();
        assert!(malformed_replay_query_does_not_poison(replay.into()));

        let data = Uint8Array::from(LEGACY_REPLAY);
        let mut replay = parse_replay(data, None).unwrap();
        let commands = replay.load_commands(None).unwrap().unwrap();
        assert!(malformed_retained_query_does_not_poison(commands.into()));
    }

    #[test]
    fn command_parse_options_use_core_defaults() {
        let config: broodrep::CommandParseConfig = CommandParseConfig::default().into();
        let defaults = broodrep::CommandParseConfig::default();
        assert_eq!(config.max_commands, defaults.max_commands);
        assert_eq!(config.max_owned_data_bytes, defaults.max_owned_data_bytes);

        let custom: broodrep::CommandParseConfig = CommandParseConfig {
            max_commands: Some(42),
            max_owned_data_bytes: Some(512),
        }
        .into();
        assert_eq!(custom.max_commands, 42);
        assert_eq!(custom.max_owned_data_bytes, 512);
    }

    #[test]
    fn ranges_must_be_in_bounds_without_overflow() {
        assert_eq!(checked_range(2.0, 3.0, 8), Ok(2..5));
        assert_eq!(checked_range(8.0, 0.0, 8), Ok(8..8));
        assert!(checked_range(8.0, 1.0, 8).is_err());
        assert!(checked_range(f64::from(u32::MAX), 1.0, 8).is_err());
        assert!(checked_range(-1.0, 0.0, 8).is_err());
        assert!(checked_range(1.5, 0.0, 8).is_err());
        assert!(checked_range(f64::NAN, 0.0, 8).is_err());
    }

    fn core_command(
        frame: u32,
        player_id: u8,
        command: broodrep::Command,
    ) -> broodrep::ReplayCommand {
        broodrep::ReplayCommand {
            frame,
            player_id,
            command,
        }
    }

    #[test]
    fn command_query_intersects_structure_unions_inclusions_and_prefers_exclusion() {
        let query = CompiledCommandQuery::new(CommandQuery {
            player_ids: Some(vec![1]),
            start_frame: Some(10),
            end_frame: Some(20),
            include_kinds: Some(vec![CommandKind::Chat]),
            include_categories: Some(vec![CommandCategory::Selection]),
            include_type_ids: None,
            exclude_kinds: Some(vec![CommandKind::SelectRemove]),
            exclude_categories: None,
            exclude_type_ids: None,
        })
        .unwrap();

        assert!(query.matches(&core_command(
            10,
            1,
            broodrep::Command::Select { unit_tags: vec![] }
        )));
        assert!(query.matches(&core_command(
            19,
            1,
            broodrep::Command::Chat {
                sender_slot: 1,
                message: String::new(),
            }
        )));
        assert!(!query.matches(&core_command(
            10,
            1,
            broodrep::Command::SelectRemove { unit_tags: vec![] }
        )));
        assert!(!query.matches(&core_command(
            10,
            0,
            broodrep::Command::Select { unit_tags: vec![] }
        )));
        assert!(!query.matches(&core_command(
            20,
            1,
            broodrep::Command::Select { unit_tags: vec![] }
        )));
    }

    #[test]
    fn empty_inclusion_and_invalid_frame_range_are_unambiguous() {
        let query = CompiledCommandQuery::new(CommandQuery {
            include_kinds: Some(vec![]),
            ..CommandQuery::default()
        })
        .unwrap();
        assert!(!query.matches(&core_command(0, 0, broodrep::Command::KeepAlive)));

        assert!(
            CompiledCommandQuery::new(CommandQuery {
                start_frame: Some(2),
                end_frame: Some(1),
                ..CommandQuery::default()
            })
            .is_err()
        );

        assert!(
            CompiledCommandQuery::new(CommandQuery {
                player_ids: Some(vec![0; 257]),
                ..CommandQuery::default()
            })
            .is_err()
        );
    }

    #[test]
    fn raw_type_id_filters_can_distinguish_untyped_commands() {
        let query = CompiledCommandQuery::new(CommandQuery {
            include_type_ids: Some(vec![0xfe]),
            exclude_kinds: Some(vec![CommandKind::Known]),
            ..CommandQuery::default()
        })
        .unwrap();

        assert!(query.matches(&core_command(
            0,
            0,
            broodrep::Command::Unknown {
                type_id: 0xfe,
                data: vec![],
            }
        )));
        assert!(!query.matches(&core_command(
            0,
            0,
            broodrep::Command::Known {
                type_id: 0xfe,
                data: vec![],
            }
        )));
    }

    #[test]
    fn raw_apm_counts_only_documented_actions() {
        let commands = [
            core_command(0, 0, broodrep::Command::Select { unit_tags: vec![] }),
            core_command(0, 0, broodrep::Command::MinimapPing { x: 0, y: 0 }),
            core_command(0, 0, broodrep::Command::KeepAlive),
            core_command(
                0,
                0,
                broodrep::Command::Chat {
                    sender_slot: 0,
                    message: String::new(),
                },
            ),
            core_command(0, 1, broodrep::Command::Train { unit_type: 0 }),
        ];
        let summary = player_apm_summary(100, 2.0, count_apm_actions(commands.iter()));

        assert_eq!(summary.players.len(), 2);
        assert_eq!(summary.players[0].actions, 2);
        assert_eq!(summary.players[0].apm, 1.0);
        assert_eq!(summary.players[1].actions, 1);
        assert_eq!(summary.players[1].apm, 0.5);
    }

    #[test]
    fn long_fixture_streamed_and_retained_apm_match_exact_action_counts() {
        let core_replay = broodrep::Replay::new(Cursor::new(LONG_HUNTERS_REPLAY.to_vec())).unwrap();
        let mut replay = Replay::new(core_replay);
        let streamed = replay.get_player_apm(None).unwrap().unwrap();
        let retained = replay
            .load_commands(None)
            .unwrap()
            .unwrap()
            .get_player_apm();
        let expected = [4_059, 2_165, 3_056, 9_949, 8_486, 8_365];

        assert_eq!(streamed.frames, 56_209);
        assert_eq!(streamed.players.len(), expected.len());
        assert_eq!(
            streamed
                .players
                .iter()
                .map(|player| player.actions)
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            retained
                .players
                .iter()
                .map(|player| player.actions)
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn command_summary_is_sorted_by_numeric_type_id() {
        let mut counts = [0usize; 256];
        counts[0x05] = 2;
        counts[0xfe] = 3;

        let summary = command_summary_from_counts(5, counts).unwrap();
        assert_eq!(summary.total, 5);
        assert_eq!(summary.counts.len(), 2);
        assert_eq!(summary.counts[0].type_id, 0x05);
        assert_eq!(summary.counts[0].count, 2);
        assert_eq!(summary.counts[1].type_id, 0xfe);
        assert_eq!(summary.counts[1].count, 3);
    }

    #[test]
    fn command_summary_streams_fixture_without_a_command_owner() {
        let core_replay = broodrep::Replay::new(Cursor::new(LEGACY_REPLAY.to_vec())).unwrap();
        let mut replay = Replay::new(core_replay);

        let summary = replay.get_command_summary(None).unwrap().unwrap();
        assert!(summary.total > 0);
        assert_eq!(
            summary.total,
            summary.counts.iter().map(|count| count.count).sum()
        );
        assert!(
            summary
                .counts
                .windows(2)
                .all(|counts| counts[0].type_id < counts[1].type_id)
        );
    }

    #[test]
    fn loading_fixture_sections_creates_wasm_owners() {
        let core_replay = broodrep::Replay::new(Cursor::new(LEGACY_REPLAY.to_vec())).unwrap();
        let mut replay = Replay::new(core_replay);

        let commands = replay.load_commands(None).unwrap().unwrap();
        assert!(!commands.is_empty());
        assert!(commands.length() > 0);

        let section = replay
            .load_raw_section(ReplaySection::Commands)
            .unwrap()
            .unwrap();
        assert!(!section.is_empty());
        assert!(section.length() > 0);
    }

    #[test]
    fn command_conversion_retains_command_metadata() {
        let converted = commands_to_wasm([broodrep::ReplayCommand {
            frame: 123,
            player_id: 4,
            command: broodrep::Command::KeepAlive,
        }]);

        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].frame, 123);
        assert_eq!(converted[0].player_id, 4);
        assert!(matches!(converted[0].command, CommandData::KeepAlive));
    }
}
