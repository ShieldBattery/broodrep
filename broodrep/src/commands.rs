use std::io::{Cursor, Read as _};

use byteorder::{LittleEndian as LE, ReadBytesExt as _};
use thiserror::Error;

use crate::TextEncoding;

#[derive(Error, Debug)]
pub enum CommandParseError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("command data truncated at frame {frame}, offset {offset}")]
    Truncated { frame: u32, offset: usize },
}

/// A single player command recorded at a specific game frame.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayCommand {
    /// The game frame this command was issued on.
    pub frame: u32,
    /// The player who issued this command (0-7 for players, higher for observers).
    pub player_id: u8,
    /// The command data.
    pub command: Command,
}

/// The data for a specific command type. Common gameplay commands are parsed into typed variants;
/// recognized but less common commands are stored as `Known` with their type ID and raw bytes;
/// completely unrecognized commands are stored as `Unknown`.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    // -- Selection --
    /// Select units (replaces current selection).
    Select { unit_tags: Vec<u16> },
    /// Add units to current selection.
    SelectAdd { unit_tags: Vec<u16> },
    /// Remove units from current selection.
    SelectRemove { unit_tags: Vec<u16> },

    // -- Selection (1.21+ with extended unit tags) --
    /// Select units in 1.21+ format (replaces current selection). The wire format uses 4 bytes
    /// per entry: a u16 unit tag (same format as classic tags) followed by a u16 of unknown
    /// purpose (observed as 0 in all known replays).
    Select121 { unit_tags: Vec<u16> },
    /// Add units to current selection in 1.21+ format.
    SelectAdd121 { unit_tags: Vec<u16> },
    /// Remove units from current selection in 1.21+ format.
    SelectRemove121 { unit_tags: Vec<u16> },

    // -- Orders --
    /// Right-click (default order: move, attack, gather, etc.).
    RightClick {
        x: u16,
        y: u16,
        target_unit_tag: u16,
        target_unit_type: u16,
        queued: bool,
    },
    /// Targeted order (specific order type on a target).
    TargetedOrder {
        x: u16,
        y: u16,
        target_unit_tag: u16,
        target_unit_type: u16,
        order: u8,
        queued: bool,
    },
    /// Right-click in 1.21+ format (extended unit tags).
    RightClick121 {
        x: u16,
        y: u16,
        target_unit_tag: u16,
        target_unit_extra: u16,
        target_unit_type: u16,
        queued: bool,
    },
    /// Targeted order in 1.21+ format (extended unit tags).
    TargetedOrder121 {
        x: u16,
        y: u16,
        target_unit_tag: u16,
        target_unit_extra: u16,
        target_unit_type: u16,
        order: u8,
        queued: bool,
    },

    // -- Building --
    /// Place a building.
    Build {
        order: u8,
        x: u16,
        y: u16,
        unit_type: u16,
    },
    /// Cancel building construction.
    CancelBuild,
    /// Cancel add-on construction.
    CancelAddon,
    /// Lift off a building.
    LiftOff { x: u16, y: u16 },

    // -- Training / Morphing --
    /// Train a unit.
    Train { unit_type: u16 },
    /// Cancel a unit from the training queue.
    CancelTrain { unit_tag: u16 },
    /// Morph a unit (e.g. Lurker, Guardian).
    UnitMorph { unit_type: u16 },
    /// Morph a building (e.g. Lair, Hive).
    BuildingMorph { unit_type: u16 },
    /// Cancel a morph in progress.
    CancelMorph,
    /// Build an interceptor or scarab.
    TrainFighter,

    // -- Unit abilities --
    /// Stop current action.
    Stop { queued: bool },
    /// Hold position.
    HoldPosition { queued: bool },
    /// Burrow.
    Burrow { queued: bool },
    /// Unburrow.
    Unburrow { queued: bool },
    /// Cloak.
    Cloak { queued: bool },
    /// Decloak.
    Decloak { queued: bool },
    /// Enter siege mode.
    Siege { queued: bool },
    /// Leave siege mode.
    Unsiege { queued: bool },
    /// Return cargo (minerals/gas).
    ReturnCargo { queued: bool },
    /// Unload all units from a transport.
    UnloadAll { queued: bool },
    /// Unload a specific unit.
    Unload { unit_tag: u16 },
    /// Unload a specific unit in 1.21+ format.
    Unload121 { unit_tag: u16, extra: u16 },
    /// Merge two Templar into an Archon.
    MergeArchon,
    /// Merge two Dark Templar into a Dark Archon.
    MergeDarkArchon,
    /// Cancel a nuke.
    CancelNuke,
    /// Use Stim Pack.
    Stim,
    /// Stop Carrier interceptors.
    CarrierStop,
    /// Stop Reaver scarabs.
    ReaverStop,
    /// Order nothing (used internally).
    OrderNothing,

    // -- Research / Upgrades --
    /// Research a technology.
    Tech { tech_id: u8 },
    /// Cancel technology research.
    CancelTech,
    /// Start an upgrade.
    Upgrade { upgrade_id: u8 },
    /// Cancel an upgrade.
    CancelUpgrade,

    // -- Hotkeys --
    /// Hotkey action (assign or recall).
    Hotkey { hotkey_type: u8, group: u8 },

    // -- Alliance / Vision --
    /// Set shared vision flags.
    Vision { flags: u16 },
    /// Set alliance flags.
    Alliance { flags: u32 },

    // -- Game control --
    /// Change game speed.
    GameSpeed { speed: u8 },
    /// Pause the game.
    Pause,
    /// Resume the game.
    Resume,
    /// Cheat code entered.
    Cheat { flags: u32 },

    // -- Chat --
    /// In-game replay chat message.
    Chat { sender_slot: u8, message: String },

    // -- Multiplayer --
    /// Keep-alive packet.
    KeepAlive,
    /// Player left the game.
    LeaveGame { reason: u8 },
    /// Minimap ping.
    MinimapPing { x: u16, y: u16 },
    /// Network latency setting.
    Latency { latency: u8 },

    /// A command with a known type ID but whose data we don't parse into specific fields.
    /// The `data` contains the raw bytes after the type ID byte.
    Known { type_id: u8, data: Vec<u8> },

    /// An unrecognized command type. Contains the type ID and remaining bytes from the frame block
    /// that could not be parsed further.
    Unknown { type_id: u8, data: Vec<u8> },
}

/// Returns the data size (in bytes after the type ID byte) for a command type, or `None` if the
/// command is variable-length or unknown.
fn command_data_size(type_id: u8) -> Option<usize> {
    match type_id {
        // 0 bytes
        0x05 | 0x08 | 0x10 | 0x11 | 0x18 | 0x19 | 0x1b | 0x1c | 0x1d | 0x27 | 0x2a | 0x2e
        | 0x31 | 0x33 | 0x34 | 0x36 | 0x38 | 0x39 | 0x3c | 0x54 | 0x5a | 0x5b => Some(0),

        // 1 byte
        0x0f | 0x1a | 0x1e | 0x21 | 0x22 | 0x25 | 0x26 | 0x28 | 0x2b | 0x2c | 0x2d | 0x30
        | 0x32 | 0x3a | 0x3b | 0x3d | 0x42 | 0x43 | 0x55 | 0x57 => Some(1),

        // 2 bytes
        0x0d | 0x13 | 0x1f | 0x20 | 0x23 | 0x29 | 0x35 | 0x41 | 0x44 | 0x45 => Some(2),

        // 4 bytes
        0x0e | 0x12 | 0x2f | 0x58 | 0x62 => Some(4),

        // 5 bytes
        0x3e => Some(5),

        // 6 bytes
        0x37 => Some(6),

        // 7 bytes
        0x0c | 0x3f => Some(7),

        // 9 bytes
        0x14 | 0x56 => Some(9),

        // 10 bytes
        0x15 => Some(10),

        // 11 bytes
        0x60 => Some(11),

        // 12 bytes
        0x48 | 0x61 => Some(12),

        // 17 bytes
        0x40 => Some(17),

        // 81 bytes
        0x5c => Some(81),

        // Variable-length (select commands, save/load game)
        0x09 | 0x0a | 0x0b | 0x06 | 0x07 | 0x63 | 0x64 | 0x65 => None,

        // Unknown
        _ => None,
    }
}

/// Returns true if this command type ID is recognized (has a known data layout).
fn is_known_command(type_id: u8) -> bool {
    command_data_size(type_id).is_some()
        || matches!(
            type_id,
            0x09 | 0x0a | 0x0b | 0x06 | 0x07 | 0x63 | 0x64 | 0x65
        )
}

/// Parse a select command (0x09, 0x0a, 0x0b) with 16-bit unit tags.
fn parse_select_data(data: &[u8]) -> Result<Vec<u16>, CommandParseError> {
    if data.is_empty() {
        return Ok(Vec::new());
    }
    let count = data[0] as usize;
    let expected = 1 + count * 2;
    if data.len() < expected {
        return Err(CommandParseError::Truncated {
            frame: 0,
            offset: 0,
        });
    }
    let mut cursor = Cursor::new(&data[1..]);
    let mut tags = Vec::with_capacity(count);
    for _ in 0..count {
        tags.push(cursor.read_u16::<LE>()?);
    }
    Ok(tags)
}

/// Parse a select command (0x63, 0x64, 0x65) with 1.21+ unit tags. Each tag is 4 bytes on the
/// wire: a u16 unit tag followed by 2 bytes of padding (always 0). The tag itself uses the
/// same format as classic u16 tags (11-bit one-based index + recycle counter).
fn parse_select121_data(data: &[u8]) -> Result<Vec<u16>, CommandParseError> {
    if data.is_empty() {
        return Ok(Vec::new());
    }
    let count = data[0] as usize;
    let expected = 1 + count * 4;
    if data.len() < expected {
        return Err(CommandParseError::Truncated {
            frame: 0,
            offset: 0,
        });
    }
    let mut cursor = Cursor::new(&data[1..]);
    let mut tags = Vec::with_capacity(count);
    for _ in 0..count {
        tags.push(cursor.read_u16::<LE>()?);
        let _padding = cursor.read_u16::<LE>()?;
    }
    Ok(tags)
}

/// Parse a single command from its type ID and data bytes.
fn parse_command(type_id: u8, data: &[u8], encoding: TextEncoding) -> Command {
    // Helper to read from data as a cursor. For fixed-size commands, the caller has already
    // verified the data length.
    let mut c = Cursor::new(data);

    match type_id {
        // Selection
        0x09 => match parse_select_data(data) {
            Ok(tags) => Command::Select { unit_tags: tags },
            Err(_) => Command::Unknown {
                type_id,
                data: data.to_vec(),
            },
        },
        0x0a => match parse_select_data(data) {
            Ok(tags) => Command::SelectAdd { unit_tags: tags },
            Err(_) => Command::Unknown {
                type_id,
                data: data.to_vec(),
            },
        },
        0x0b => match parse_select_data(data) {
            Ok(tags) => Command::SelectRemove { unit_tags: tags },
            Err(_) => Command::Unknown {
                type_id,
                data: data.to_vec(),
            },
        },
        0x63 => match parse_select121_data(data) {
            Ok(tags) => Command::Select121 { unit_tags: tags },
            Err(_) => Command::Unknown {
                type_id,
                data: data.to_vec(),
            },
        },
        0x64 => match parse_select121_data(data) {
            Ok(tags) => Command::SelectAdd121 { unit_tags: tags },
            Err(_) => Command::Unknown {
                type_id,
                data: data.to_vec(),
            },
        },
        0x65 => match parse_select121_data(data) {
            Ok(tags) => Command::SelectRemove121 { unit_tags: tags },
            Err(_) => Command::Unknown {
                type_id,
                data: data.to_vec(),
            },
        },

        // Orders
        0x14 => Command::RightClick {
            x: c.read_u16::<LE>().unwrap(),
            y: c.read_u16::<LE>().unwrap(),
            target_unit_tag: c.read_u16::<LE>().unwrap(),
            target_unit_type: c.read_u16::<LE>().unwrap(),
            queued: c.read_u8().unwrap() != 0,
        },
        0x15 => Command::TargetedOrder {
            x: c.read_u16::<LE>().unwrap(),
            y: c.read_u16::<LE>().unwrap(),
            target_unit_tag: c.read_u16::<LE>().unwrap(),
            target_unit_type: c.read_u16::<LE>().unwrap(),
            order: c.read_u8().unwrap(),
            queued: c.read_u8().unwrap() != 0,
        },
        0x60 => Command::RightClick121 {
            x: c.read_u16::<LE>().unwrap(),
            y: c.read_u16::<LE>().unwrap(),
            target_unit_tag: c.read_u16::<LE>().unwrap(),
            target_unit_extra: c.read_u16::<LE>().unwrap(),
            target_unit_type: c.read_u16::<LE>().unwrap(),
            queued: c.read_u8().unwrap() != 0,
        },
        0x61 => Command::TargetedOrder121 {
            x: c.read_u16::<LE>().unwrap(),
            y: c.read_u16::<LE>().unwrap(),
            target_unit_tag: c.read_u16::<LE>().unwrap(),
            target_unit_extra: c.read_u16::<LE>().unwrap(),
            target_unit_type: c.read_u16::<LE>().unwrap(),
            order: c.read_u8().unwrap(),
            queued: c.read_u8().unwrap() != 0,
        },

        // Building
        0x0c => Command::Build {
            order: c.read_u8().unwrap(),
            x: c.read_u16::<LE>().unwrap(),
            y: c.read_u16::<LE>().unwrap(),
            unit_type: c.read_u16::<LE>().unwrap(),
        },
        0x18 => Command::CancelBuild,
        0x34 => Command::CancelAddon,
        0x2f => Command::LiftOff {
            x: c.read_u16::<LE>().unwrap(),
            y: c.read_u16::<LE>().unwrap(),
        },

        // Training / Morphing
        0x1f => Command::Train {
            unit_type: c.read_u16::<LE>().unwrap(),
        },
        0x20 => Command::CancelTrain {
            unit_tag: c.read_u16::<LE>().unwrap(),
        },
        0x23 => Command::UnitMorph {
            unit_type: c.read_u16::<LE>().unwrap(),
        },
        0x35 => Command::BuildingMorph {
            unit_type: c.read_u16::<LE>().unwrap(),
        },
        0x19 => Command::CancelMorph,
        0x27 => Command::TrainFighter,

        // Unit abilities
        0x1a => Command::Stop {
            queued: c.read_u8().unwrap() != 0,
        },
        0x2b => Command::HoldPosition {
            queued: c.read_u8().unwrap() != 0,
        },
        0x2c => Command::Burrow {
            queued: c.read_u8().unwrap() != 0,
        },
        0x2d => Command::Unburrow {
            queued: c.read_u8().unwrap() != 0,
        },
        0x21 => Command::Cloak {
            queued: c.read_u8().unwrap() != 0,
        },
        0x22 => Command::Decloak {
            queued: c.read_u8().unwrap() != 0,
        },
        0x26 => Command::Siege {
            queued: c.read_u8().unwrap() != 0,
        },
        0x25 => Command::Unsiege {
            queued: c.read_u8().unwrap() != 0,
        },
        0x1e => Command::ReturnCargo {
            queued: c.read_u8().unwrap() != 0,
        },
        0x28 => Command::UnloadAll {
            queued: c.read_u8().unwrap() != 0,
        },
        0x29 => Command::Unload {
            unit_tag: c.read_u16::<LE>().unwrap(),
        },
        0x62 => Command::Unload121 {
            unit_tag: c.read_u16::<LE>().unwrap(),
            extra: c.read_u16::<LE>().unwrap(),
        },
        0x2a => Command::MergeArchon,
        0x5a => Command::MergeDarkArchon,
        0x2e => Command::CancelNuke,
        0x36 => Command::Stim,
        0x1b => Command::CarrierStop,
        0x1c => Command::ReaverStop,
        0x1d => Command::OrderNothing,

        // Research / Upgrades
        0x30 => Command::Tech {
            tech_id: c.read_u8().unwrap(),
        },
        0x31 => Command::CancelTech,
        0x32 => Command::Upgrade {
            upgrade_id: c.read_u8().unwrap(),
        },
        0x33 => Command::CancelUpgrade,

        // Hotkeys
        0x13 => Command::Hotkey {
            hotkey_type: c.read_u8().unwrap(),
            group: c.read_u8().unwrap(),
        },

        // Alliance / Vision
        0x0d => Command::Vision {
            flags: c.read_u16::<LE>().unwrap(),
        },
        0x0e => Command::Alliance {
            flags: c.read_u32::<LE>().unwrap(),
        },

        // Game control
        0x0f => Command::GameSpeed {
            speed: c.read_u8().unwrap(),
        },
        0x10 => Command::Pause,
        0x11 => Command::Resume,
        0x12 => Command::Cheat {
            flags: c.read_u32::<LE>().unwrap(),
        },

        // Chat
        0x5c => {
            let sender_slot = c.read_u8().unwrap();
            let mut msg_bytes = [0u8; 80];
            let _ = c.read(&mut msg_bytes);
            let message = encoding.decode(&msg_bytes);
            Command::Chat {
                sender_slot,
                message,
            }
        }

        // Multiplayer
        0x05 => Command::KeepAlive,
        0x57 => Command::LeaveGame {
            reason: c.read_u8().unwrap(),
        },
        0x58 => Command::MinimapPing {
            x: c.read_u16::<LE>().unwrap(),
            y: c.read_u16::<LE>().unwrap(),
        },
        0x55 => Command::Latency {
            latency: c.read_u8().unwrap(),
        },

        // Known commands we store as raw bytes
        0x06 | 0x07 | 0x08 | 0x37 | 0x38 | 0x39 | 0x3a | 0x3b | 0x3c | 0x3d | 0x3e | 0x3f
        | 0x40 | 0x41 | 0x42 | 0x43 | 0x44 | 0x45 | 0x48 | 0x54 | 0x56 | 0x5b => Command::Known {
            type_id,
            data: data.to_vec(),
        },

        // Unknown
        _ => Command::Unknown {
            type_id,
            data: data.to_vec(),
        },
    }
}

/// Parse the raw command section bytes into a list of replay commands.
///
/// The command section format consists of frame blocks:
/// - `i32 LE`: frame number
/// - `u8`: number of bytes of action data following
/// - action data bytes (variable length)
///
/// Within each frame's action data, individual commands are:
/// - `u8`: player ID
/// - `u8`: command type ID
/// - type-specific data bytes
pub fn parse_commands(
    data: &[u8],
    encoding: TextEncoding,
) -> Result<Vec<ReplayCommand>, CommandParseError> {
    let mut commands = Vec::new();
    let mut cursor = Cursor::new(data);
    let total_len = data.len() as u64;

    while cursor.position() < total_len {
        // Need at least 5 bytes for a frame block header (4 frame + 1 size)
        if total_len - cursor.position() < 5 {
            break;
        }

        let frame = cursor.read_u32::<LE>()?;
        let block_size = cursor.read_u8()? as usize;
        let block_start = cursor.position() as usize;
        let block_end = block_start + block_size;

        if block_end > data.len() {
            return Err(CommandParseError::Truncated {
                frame,
                offset: block_start,
            });
        }

        let mut pos = block_start;
        while pos < block_end {
            // Need at least 2 bytes for player ID + command type
            if block_end - pos < 2 {
                break;
            }

            let player_id = data[pos];
            let type_id = data[pos + 1];
            let cmd_data_start = pos + 2;

            let cmd_data_len = if let Some(fixed_size) = command_data_size(type_id) {
                if cmd_data_start + fixed_size > block_end {
                    // Not enough data — treat as unknown and consume rest of block
                    let remaining = &data[cmd_data_start..block_end];
                    commands.push(ReplayCommand {
                        frame,
                        player_id,
                        command: Command::Unknown {
                            type_id,
                            data: remaining.to_vec(),
                        },
                    });
                    pos = block_end;
                    continue;
                }
                fixed_size
            } else if matches!(type_id, 0x09..=0x0b) {
                // Variable-length select: count byte + count * 2 bytes
                if cmd_data_start >= block_end {
                    pos = block_end;
                    continue;
                }
                let count = data[cmd_data_start] as usize;
                1 + count * 2
            } else if matches!(type_id, 0x63..=0x65) {
                // Variable-length select 1.21+: count byte + count * 4 bytes
                if cmd_data_start >= block_end {
                    pos = block_end;
                    continue;
                }
                let count = data[cmd_data_start] as usize;
                1 + count * 4
            } else {
                // Unknown or variable-length command (SaveGame, LoadGame, etc.)
                // Consume rest of block
                let remaining = &data[cmd_data_start..block_end];
                commands.push(ReplayCommand {
                    frame,
                    player_id,
                    command: if is_known_command(type_id) {
                        Command::Known {
                            type_id,
                            data: remaining.to_vec(),
                        }
                    } else {
                        Command::Unknown {
                            type_id,
                            data: remaining.to_vec(),
                        }
                    },
                });
                pos = block_end;
                continue;
            };

            let actual_end = cmd_data_start + cmd_data_len;
            if actual_end > block_end {
                // Truncated variable-length command
                let remaining = &data[cmd_data_start..block_end];
                commands.push(ReplayCommand {
                    frame,
                    player_id,
                    command: Command::Unknown {
                        type_id,
                        data: remaining.to_vec(),
                    },
                });
                pos = block_end;
                continue;
            }

            let cmd_data = &data[cmd_data_start..actual_end];
            commands.push(ReplayCommand {
                frame,
                player_id,
                command: parse_command(type_id, cmd_data, encoding),
            });

            pos = actual_end;
        }

        cursor.set_position(block_end as u64);
    }

    Ok(commands)
}

/// Returns the command type ID for a command.
impl Command {
    pub fn type_id(&self) -> u8 {
        match self {
            Command::Select { .. } => 0x09,
            Command::SelectAdd { .. } => 0x0a,
            Command::SelectRemove { .. } => 0x0b,
            Command::Select121 { .. } => 0x63,
            Command::SelectAdd121 { .. } => 0x64,
            Command::SelectRemove121 { .. } => 0x65,
            Command::RightClick { .. } => 0x14,
            Command::TargetedOrder { .. } => 0x15,
            Command::RightClick121 { .. } => 0x60,
            Command::TargetedOrder121 { .. } => 0x61,
            Command::Build { .. } => 0x0c,
            Command::CancelBuild => 0x18,
            Command::CancelAddon => 0x34,
            Command::LiftOff { .. } => 0x2f,
            Command::Train { .. } => 0x1f,
            Command::CancelTrain { .. } => 0x20,
            Command::UnitMorph { .. } => 0x23,
            Command::BuildingMorph { .. } => 0x35,
            Command::CancelMorph => 0x19,
            Command::TrainFighter => 0x27,
            Command::Stop { .. } => 0x1a,
            Command::HoldPosition { .. } => 0x2b,
            Command::Burrow { .. } => 0x2c,
            Command::Unburrow { .. } => 0x2d,
            Command::Cloak { .. } => 0x21,
            Command::Decloak { .. } => 0x22,
            Command::Siege { .. } => 0x26,
            Command::Unsiege { .. } => 0x25,
            Command::ReturnCargo { .. } => 0x1e,
            Command::UnloadAll { .. } => 0x28,
            Command::Unload { .. } => 0x29,
            Command::Unload121 { .. } => 0x62,
            Command::MergeArchon => 0x2a,
            Command::MergeDarkArchon => 0x5a,
            Command::CancelNuke => 0x2e,
            Command::Stim => 0x36,
            Command::CarrierStop => 0x1b,
            Command::ReaverStop => 0x1c,
            Command::OrderNothing => 0x1d,
            Command::Tech { .. } => 0x30,
            Command::CancelTech => 0x31,
            Command::Upgrade { .. } => 0x32,
            Command::CancelUpgrade => 0x33,
            Command::Hotkey { .. } => 0x13,
            Command::Vision { .. } => 0x0d,
            Command::Alliance { .. } => 0x0e,
            Command::GameSpeed { .. } => 0x0f,
            Command::Pause => 0x10,
            Command::Resume => 0x11,
            Command::Cheat { .. } => 0x12,
            Command::Chat { .. } => 0x5c,
            Command::KeepAlive => 0x05,
            Command::LeaveGame { .. } => 0x57,
            Command::MinimapPing { .. } => 0x58,
            Command::Latency { .. } => 0x55,
            Command::Known { type_id, .. } | Command::Unknown { type_id, .. } => *type_id,
        }
    }
}
