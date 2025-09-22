use std::{
    ffi::{CStr, FromBytesUntilNulError},
    io::Read as _,
};

use byteorder::{LittleEndian as LE, ReadBytesExt as _};
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
    /// The game's ID on ShieldBattery (a UUID as a u128).
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
    let game_id = data.read_u128::<LE>()?;
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
