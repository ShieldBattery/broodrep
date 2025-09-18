use std::{
    ffi::{CStr, FromBytesUntilNulError},
    io::Read as _,
};

use byteorder::{LittleEndian as LE, ReadBytesExt as _};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ShieldBatteryDataError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error("invalid string: {0}")]
    InvalidString(#[from] FromBytesUntilNulError),
}

#[derive(Debug, Clone)]
pub enum ShieldBatteryData {
    Version0(ShieldBatteryDataV0),
    Version1(ShieldBatteryDataV0, ShieldBatteryDataV1),
}

#[derive(Debug, Clone)]
pub struct ShieldBatteryDataV0 {
    pub starcraft_exe_build: u32,
    pub shieldbattery_version: String,
    pub team_game_main_players: [u8; 4],
    pub starting_races: [u8; 12],
    pub game_id: u128,
    pub user_ids: [u32; 8],
}

#[derive(Debug, Clone)]
pub struct ShieldBatteryDataV1 {
    game_logic_version: u16,
}

impl ShieldBatteryData {
    /// The build number of the StarCraft executable used to play the game.
    pub fn starcraft_exe_build(&self) -> u32 {
        match self {
            ShieldBatteryData::Version0(data) => data.starcraft_exe_build,
            ShieldBatteryData::Version1(data, _) => data.starcraft_exe_build,
        }
    }

    /// The version string of the ShieldBattery client used to play the game.
    pub fn shieldbattery_version(&self) -> &str {
        match self {
            ShieldBatteryData::Version0(data) => &data.shieldbattery_version,
            ShieldBatteryData::Version1(data, _) => &data.shieldbattery_version,
        }
    }

    /// Which players were the "main" players in a team game (e.g. Team Melee).
    pub fn team_game_main_players(&self) -> &[u8; 4] {
        match self {
            ShieldBatteryData::Version0(data) => &data.team_game_main_players,
            ShieldBatteryData::Version1(data, _) => &data.team_game_main_players,
        }
    }

    /// The starting race for each player in the game.
    pub fn starting_races(&self) -> &[u8; 12] {
        match self {
            ShieldBatteryData::Version0(data) => &data.starting_races,
            ShieldBatteryData::Version1(data, _) => &data.starting_races,
        }
    }

    /// The game's ID on ShieldBattery (a UUID as a u128).
    pub fn game_id(&self) -> u128 {
        match self {
            ShieldBatteryData::Version0(data) => data.game_id,
            ShieldBatteryData::Version1(data, _) => data.game_id,
        }
    }

    /// The ShieldBattery user IDs of the players ingame, in the same order as the players in the
    /// replay header.
    pub fn user_ids(&self) -> &[u32; 8] {
        match self {
            ShieldBatteryData::Version0(data) => &data.user_ids,
            ShieldBatteryData::Version1(data, _) => &data.user_ids,
        }
    }

    /// The version of ShieldBattery game logic modifications used to play the game.
    pub fn game_logic_version(&self) -> Option<u16> {
        match self {
            ShieldBatteryData::Version0(_) => None,
            ShieldBatteryData::Version1(_, data) => Some(data.game_logic_version),
        }
    }
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
    let game_id = data.read_u128::<LE>()?;
    let mut user_ids = [0u32; 8];
    data.read_u32_into::<LE>(&mut user_ids)?;

    let v0 = ShieldBatteryDataV0 {
        starcraft_exe_build,
        shieldbattery_version,
        team_game_main_players,
        starting_races,
        game_id,
        user_ids,
    };
    if version == 0 {
        Ok(ShieldBatteryData::Version0(v0))
    } else
    /* if version >= 1 */
    {
        let game_logic_version = data.read_u16::<LE>()?;
        let v1 = ShieldBatteryDataV1 { game_logic_version };
        Ok(ShieldBatteryData::Version1(v0, v1))
    }
}
