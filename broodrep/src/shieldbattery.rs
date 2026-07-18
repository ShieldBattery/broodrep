use std::{
    ffi::{CStr, FromBytesUntilNulError},
    io::Read as _,
};

use byteorder::{BigEndian as BE, LittleEndian as LE, ReadBytesExt as _};
use thiserror::Error;

use crate::Race;

#[derive(Error, Debug)]
pub enum ShieldBatteryDataError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error("invalid string: {0}")]
    InvalidString(#[from] FromBytesUntilNulError),
}

#[derive(Debug, Clone)]
pub struct ShieldBatteryData {
    /// The build number of the StarCraft executable used to play the game.
    pub starcraft_exe_build: u32,
    /// The version string of the ShieldBattery client used to play the game.
    pub shieldbattery_version: String,
    /// Which players were the "main" players in a team game (e.g. Team Melee).
    pub team_game_main_players: [u8; 4],
    /// The starting race for each player in the game.
    pub starting_races: [Race; 12],
    /// The game's ID on ShieldBattery (a UUID). The UUID's RFC-4122 bytes are stored big-endian
    /// in this value, so e.g. `uuid::Uuid::from_u128(game_id)` or
    /// `uuid::Uuid::from_bytes(game_id.to_be_bytes())` will reconstruct it directly.
    pub game_id: u128,
    /// The ShieldBattery user IDs of the players ingame, in the same order as the players in the
    /// replay header.
    pub user_ids: [u32; 8],
    /// The version of ShieldBattery game logic modifications used to play the game. May not be
    /// present on older replays.
    pub game_logic_version: Option<u16>,
}

pub fn parse_shieldbattery_section(
    mut data: &[u8],
) -> Result<ShieldBatteryData, ShieldBatteryDataError> {
    let version = data.read_u16::<LE>()?;

    let starcraft_exe_build = data.read_u32::<LE>()?;
    let mut shieldbattery_version = [0; 0x11];
    data.read_exact(&mut shieldbattery_version[..0x10])?;
    let shieldbattery_version = CStr::from_bytes_until_nul(&shieldbattery_version)?
        .to_string_lossy()
        .to_string();
    let mut team_game_main_players = [0u8; 4];
    data.read_exact(&mut team_game_main_players)?;
    let mut starting_races = [0u8; 12];
    data.read_exact(&mut starting_races)?;
    let starting_races = starting_races.map(Into::into);
    // ShieldBattery's writer stores the UUID's hex digits in canonical RFC-4122 string order
    // (i.e. big-endian), see game/src/replay.rs `write_uuid` in the ShieldBattery repo
    let game_id = data.read_u128::<BE>()?;
    let mut user_ids = [0u32; 8];
    data.read_u32_into::<LE>(&mut user_ids)?;

    let mut parsed = ShieldBatteryData {
        starcraft_exe_build,
        shieldbattery_version,
        team_game_main_players,
        starting_races,
        game_id,
        user_ids,
        game_logic_version: None,
    };
    if version >= 1 {
        parsed.game_logic_version = Some(data.read_u16::<LE>()?);
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_id_matches_shieldbattery_write_uuid() {
        // Payload layout from ShieldBattery's game/src/replay.rs `add_shieldbattery_data`, with
        // the game_id bytes taken from its `write_uuid` unit test vector:
        // "12345678-9abc-def0-1234-56789abcdef0" is written as bytes
        // 12 34 56 78 9a bc de f0 12 34 56 78 9a bc de f0
        let mut data = vec![0u8; 0x58];
        data[0x0..0x2].copy_from_slice(&1u16.to_le_bytes()); // format_version
        data[0x26..0x36].copy_from_slice(&[
            0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc,
            0xde, 0xf0,
        ]);

        let parsed = parse_shieldbattery_section(&data).unwrap();
        assert_eq!(parsed.game_id, 0x12345678_9abc_def0_1234_56789abcdef0);
    }
}
