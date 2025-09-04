use std::{
    ffi::CStr,
    io::{Cursor, Read, Seek, SeekFrom},
};

use byteorder::{LittleEndian as LE, ReadBytesExt as _};
use chrono::{DateTime, NaiveDateTime};
use explode::ExplodeReader;
use flate2::bufread::ZlibDecoder;
use thiserror::Error;

use crate::compression::SafeDecompressor;
pub use crate::compression::{DecompressionConfig, DecompressionError};

mod compression;

#[derive(Error, Debug)]
pub enum BroodrepError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error("malformed header: {0}")]
    MalformedHeader(&'static str),
    #[error("problem decompressing data: {0}")]
    Decompression(#[from] DecompressionError),
}

pub struct Replay<R: Read + Seek> {
    inner: R,
    format: ReplayFormat,
    pub header: ReplayHeader,
}

impl<R: Read + Seek> Replay<R> {
    /// Creates a new Replay by parsing data from a [Read] implementation with default settings for
    /// reading.
    pub fn new(reader: R) -> Result<Self, BroodrepError> {
        Self::new_with_decompression_config(reader, DecompressionConfig::default())
    }

    // TODO(tec27): Would probably be nice to be able to specify limits for the file as a whole as
    // well
    /// Creates a new Replay by parsing data from a [Read] implementation with specified settings
    /// for reading. Note that the limits specified will apply to each chunk individually, rather
    /// than to the entire replay collectively.
    pub fn new_with_decompression_config(
        mut reader: R,
        config: DecompressionConfig,
    ) -> Result<Self, BroodrepError> {
        let format = Self::detect_format(&mut reader)?;

        reader.seek(SeekFrom::Start(0))?;
        // First section is just the magic bytes, we just sanity check it and then skip it since
        // we already know the format from detect_format()
        let header = Self::read_section_header(&mut reader)?;
        if header.num_chunks != 1 {
            return Err(BroodrepError::MalformedHeader("invalid magic chunk"));
        }
        let size = reader.read_u32::<LE>()?;
        if size != 4 {
            return Err(BroodrepError::MalformedHeader("invalid magic chunk size"));
        }
        // Magic bytes (note we've already checked this when detecting the format, so we don't need
        // to repeat that step here)
        reader.read_u32::<LE>()?;
        if format == ReplayFormat::Modern121 {
            // This is the offset of the first section after the "legacy" sections, I guess as a
            // way to be able to skip them easily? In older formats, this offset is not present
            // (even though the Modern non-1.21 version does have other sections)
            reader.read_u32::<LE>()?;
        }

        // Replay header section
        let replay_header = Self::read_legacy_section(&mut reader, format, config)?;
        let replay_header = Self::parse_replay_header(&replay_header)?;

        Ok(Replay {
            inner: reader,
            format,
            header: replay_header,
        })
    }

    pub fn into_inner(self) -> R {
        self.inner
    }

    pub fn format(&self) -> ReplayFormat {
        self.format
    }

    pub fn engine(&self) -> Engine {
        self.header.engine
    }

    pub fn frames(&self) -> u32 {
        self.header.frames
    }

    /// Returns the time the game started at, as dictated by the game host. Note that this is
    /// technically the game seed and not a timestamp (it just happens to use a timestamp), so this
    /// isn't *guaranteed* to be an accurate time (but in practice it is).
    pub fn start_time(&self) -> Option<NaiveDateTime> {
        Some(DateTime::from_timestamp(self.header.start_time as i64, 0)?.naive_utc())
    }

    pub fn game_title(&self) -> &str {
        &self.header.title
    }

    pub fn map_name(&self) -> &str {
        &self.header.map_name
    }

    /// Returns the (width, height) of the map (in tiles).
    pub fn map_dimensions(&self) -> (u16, u16) {
        (self.header.map_width, self.header.map_height)
    }

    pub fn game_speed(&self) -> GameSpeed {
        self.header.speed
    }

    pub fn game_type(&self) -> GameType {
        self.header.game_type
    }

    /// For Top Vs Bottom, specifies the number of players on the first team. For Team game types,
    /// specifies the number of slots on each team (truncating the last team if necessary).
    pub fn game_sub_type(&self) -> u16 {
        self.header.game_sub_type
    }

    pub fn host_name(&self) -> &str {
        &self.header.host_name
    }

    pub fn host_player(&self) -> Option<&Player> {
        self.header
            .slots
            .iter()
            .find(|p| p.name == self.header.host_name)
    }

    pub fn players(&self) -> impl Iterator<Item = &Player> {
        self.header.players()
    }

    pub fn observers(&self) -> impl Iterator<Item = &Player> {
        self.header.observers()
    }

    pub fn slots(&self) -> &[Player] {
        &self.header.slots
    }

    fn detect_format(reader: &mut R) -> Result<ReplayFormat, BroodrepError> {
        // 1.21+ has `seRS`, before that it's `reRS`
        reader.seek(SeekFrom::Start(12))?;
        let mut magic = [0; 4];
        reader.read_exact(&mut magic)?;

        if magic == *b"seRS" {
            return Ok(ReplayFormat::Modern121);
        }
        if magic != *b"reRS" {
            return Err(BroodrepError::MalformedHeader("invalid magic bytes"));
        }

        // Check compression type, newer compression type indicates 1.18+
        reader.seek(SeekFrom::Current(12))?; // offset 28
        let byte = reader.read_u8()?;
        if byte == 0x78 {
            Ok(ReplayFormat::Modern)
        } else {
            // TODO(tec27): Make sure it's within the valid range
            Ok(ReplayFormat::Legacy)
        }
    }

    fn read_section_header(reader: &mut R) -> Result<SectionHeader, BroodrepError> {
        let checksum = reader.read_u32::<LE>()?;
        let num_chunks = reader.read_u32::<LE>()?;
        Ok(SectionHeader {
            checksum,
            num_chunks,
        })
    }

    fn read_legacy_section(
        reader: &mut R,
        format: ReplayFormat,
        config: DecompressionConfig,
    ) -> Result<Vec<u8>, BroodrepError> {
        let header = Self::read_section_header(reader)?;
        // TODO(tec27): Pass a size hint for known sections to avoid reallocations?
        let mut data = Vec::new();
        for _ in 0..header.num_chunks {
            let size = reader.read_u32::<LE>()?;
            data.reserve(size as usize);
            // TODO(tec27): Keep a working buffer around to avoid needing to reallocate buffers
            // frequently? Peek the first byte and seek back to avoid needing this allocation at
            // all?
            let mut compressed = vec![0; size as usize];
            reader.read_exact(&mut compressed)?;

            match format {
                ReplayFormat::Legacy => {
                    let mut decoder = SafeDecompressor::new(
                        ExplodeReader::new(&compressed[..]),
                        config,
                        Some(size as u64),
                    );
                    decoder.read_to_end(&mut data)?;
                }
                ReplayFormat::Modern | ReplayFormat::Modern121 => {
                    if size <= 4 || compressed[0] != 0x78 {
                        // Not compressed, we can return it directly
                        data.extend(compressed);
                    } else {
                        let mut decoder = SafeDecompressor::new(
                            ZlibDecoder::new(&compressed[..]),
                            config,
                            Some(size as u64),
                        );
                        decoder.read_to_end(&mut data)?;
                    }
                }
            }
        }

        Ok(data)
    }

    fn parse_replay_header(data: &[u8]) -> Result<ReplayHeader, BroodrepError> {
        let mut cursor = Cursor::new(data);
        let engine = cursor.read_u8()?.into();
        let frames = cursor.read_u32::<LE>()?;

        cursor.seek(SeekFrom::Current(3))?; // replay_campaign_mission + 0x48 (lobby init command)

        let start_time = cursor.read_u32::<LE>()?;

        cursor.seek(SeekFrom::Current(12))?; // player bytes

        // TODO(tec27): Handle non-UTF-8 string formats
        let mut title = vec![0u8; 29];
        cursor.read_exact(&mut title[..28])?;
        let title = CStr::from_bytes_until_nul(&title)
            // This should never happen (we left an extra byte to ensure the null) but just in case
            .map_err(|_e| BroodrepError::MalformedHeader("invalid title"))?
            .to_string_lossy()
            .into_owned();

        let map_width = cursor.read_u16::<LE>()?;
        let map_height = cursor.read_u16::<LE>()?;
        cursor.seek(SeekFrom::Current(1))?; // unused/padding?
        let available_slots = cursor.read_u8()?;
        let speed = cursor.read_u8()?.try_into()?;
        cursor.seek(SeekFrom::Current(1))?; // unused/padding?
        let game_type = cursor.read_u16::<LE>()?.into();
        let game_sub_type = cursor.read_u16::<LE>()?;

        cursor.seek(SeekFrom::Current(8))?; // unknown

        let mut host_name = vec![0u8; 25];
        cursor.read_exact(&mut host_name[..24])?;
        let host_name = CStr::from_bytes_until_nul(&host_name)
            // This should never happen (we left an extra byte to ensure the null) but just in case
            .map_err(|_e| BroodrepError::MalformedHeader("invalid host name"))?
            .to_string_lossy()
            .into_owned();

        cursor.seek(SeekFrom::Current(1))?; // unknown

        let mut map_name = vec![0u8; 27];
        cursor.read_exact(&mut map_name[..26])?;
        let map_name = CStr::from_bytes_until_nul(&map_name)
            // This should never happen (we left an extra byte to ensure the null) but just in case
            .map_err(|_e| BroodrepError::MalformedHeader("invalid map name"))?
            .to_string_lossy()
            .into_owned();

        cursor.seek(SeekFrom::Current(38))?; // unknown

        let players = (0..12)
            .map(|_i| {
                let slot_id = cursor.read_u16::<LE>()?;
                cursor.seek(SeekFrom::Current(2))?; // unknown
                let network_id = cursor.read_u8()?;
                cursor.seek(SeekFrom::Current(3))?; // unknown
                let player_type: PlayerType = cursor.read_u8()?.try_into()?;
                let race: Race = cursor.read_u8()?.try_into()?;
                let team = cursor.read_u8()?;
                let mut name = vec![0u8; 26];
                cursor.read_exact(&mut name[..25])?;
                let name = CStr::from_bytes_until_nul(&name)
                    // This should never happen (we left an extra byte to ensure the null) but just
                    // in case
                    .map_err(|_e| BroodrepError::MalformedHeader("invalid player name"))?
                    .to_string_lossy()
                    .into_owned();

                Ok::<Player, BroodrepError>(Player {
                    slot_id,
                    network_id,
                    player_type,
                    race,
                    team,
                    name,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ReplayHeader {
            engine,
            frames,
            start_time,
            title,
            map_width,
            map_height,
            available_slots,
            speed,
            game_type,
            game_sub_type,
            host_name,
            map_name,
            slots: players,
        })
    }
}

/// The format version of a replay.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ReplayFormat {
    /// The replay was created with a version before 1.18.
    Legacy,
    /// The replay was created with a version between 1.18 and 1.21.
    Modern,
    /// The replay was created with version 1.21 or later.
    Modern121,
}

#[derive(Debug, Copy, Clone)]
struct SectionHeader {
    #[expect(dead_code)]
    checksum: u32,
    num_chunks: u32,
}

/// The engine the game was played under.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Engine {
    StarCraft,
    BroodWar,
    Unknown(u8),
}

impl From<u8> for Engine {
    fn from(value: u8) -> Self {
        match value {
            0 => Engine::StarCraft,
            1 => Engine::BroodWar,
            other => Engine::Unknown(other),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum GameSpeed {
    Slowest = 0,
    Slower = 1,
    Slow = 2,
    Normal = 3,
    Fast = 4,
    Faster = 5,
    Fastest = 6,
}

impl TryFrom<u8> for GameSpeed {
    type Error = BroodrepError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(GameSpeed::Slowest),
            1 => Ok(GameSpeed::Slower),
            2 => Ok(GameSpeed::Slow),
            3 => Ok(GameSpeed::Normal),
            4 => Ok(GameSpeed::Fast),
            5 => Ok(GameSpeed::Faster),
            6 => Ok(GameSpeed::Fastest),
            _ => Err(BroodrepError::MalformedHeader("invalid game speed")),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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
    Unknown(u16),
}

impl From<u16> for GameType {
    fn from(value: u16) -> Self {
        match value {
            0 => GameType::None,
            // 1 => Custom in WC3
            2 => GameType::Melee,
            3 => GameType::FreeForAll,
            4 => GameType::OneOnOne,
            5 => GameType::CaptureTheFlag,
            6 => GameType::Greed,
            7 => GameType::Slaughter,
            8 => GameType::SuddenDeath,
            9 => GameType::Ladder,
            10 => GameType::UseMapSettings,
            11 => GameType::TeamMelee,
            12 => GameType::TeamFreeForAll,
            13 => GameType::TeamCaptureTheFlag,
            // 14 => Unknown
            15 => GameType::TopVsBottom,
            // 16 => Iron Man Ladder in WC3
            other => GameType::Unknown(other),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReplayHeader {
    pub engine: Engine,
    /// How many game frames this replays contains actions for.
    pub frames: u32,
    /// The time the game started at. This is actually the game's random seed, but since the game
    /// always uses the current unix timestamp as a seed, it also represents the local time the game
    /// began (local to the host).
    pub start_time: u32,
    pub title: String,
    /// Map width in tiles
    pub map_width: u16,
    /// Map height in tiles
    pub map_height: u16,
    pub available_slots: u8,
    pub speed: GameSpeed,
    pub game_type: GameType,
    /// For Top Vs Bottom, specifies the number of players on the first team. For Team game types,
    /// specifies the number of slots on each team (truncating the last team if necessary).
    pub game_sub_type: u16,
    pub host_name: String,
    pub map_name: String,
    /// All of the slots in the game, including empty slots.
    pub slots: Vec<Player>,
}

impl ReplayHeader {
    /// Returns an iterator over all of the filled slots in the game (not including observers).
    pub fn players(&self) -> impl Iterator<Item = &Player> {
        self.slots
            .iter()
            .filter(|p| !p.is_empty() && !p.is_observer())
    }

    /// Returns an iterator over all of the filled observer slots in the game.
    pub fn observers(&self) -> impl Iterator<Item = &Player> {
        self.slots
            .iter()
            .filter(|p| !p.is_empty() && p.is_observer())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Player {
    /// ID of the map slot the player was placed in (post-randomization, if applicable).
    pub slot_id: u16,
    /// Network ID of the player. Computer players will be 255. Observers will be 128-131.
    pub network_id: u8,
    pub player_type: PlayerType,
    pub race: Race,
    pub team: u8,
    pub name: String,
    // TODO(tec27): implement colors
}

impl Player {
    /// Returns true if this [Player] represents an empty slot.
    pub fn is_empty(&self) -> bool {
        self.name.is_empty()
    }

    /// Returns true if this [Player] is an observer.
    pub fn is_observer(&self) -> bool {
        (128..=131).contains(&self.network_id)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PlayerType {
    Inactive = 0,
    Computer = 1,
    Human = 2,
    RescuePassive = 3,
    Unused = 4,
    ComputerControlled = 5,
    Open = 6,
    Neutral = 7,
    Closed = 8,
}

impl TryFrom<u8> for PlayerType {
    type Error = BroodrepError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(PlayerType::Inactive),
            1 => Ok(PlayerType::Computer),
            2 => Ok(PlayerType::Human),
            3 => Ok(PlayerType::RescuePassive),
            4 => Ok(PlayerType::Unused),
            5 => Ok(PlayerType::ComputerControlled),
            6 => Ok(PlayerType::Open),
            7 => Ok(PlayerType::Neutral),
            8 => Ok(PlayerType::Closed),
            _ => Err(BroodrepError::MalformedHeader("invalid player type")),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Race {
    Zerg = 0,
    Terran = 1,
    Protoss = 2,
    // NOTE(tec27): Generally this shouldn't be present for occupied slots in a replay (as it will
    // have been resolved by the replay write time), but for empty slots it may be
    Random = 6,
}

impl TryFrom<u8> for Race {
    type Error = BroodrepError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Race::Zerg),
            1 => Ok(Race::Terran),
            2 => Ok(Race::Protoss),
            6 => Ok(Race::Random),
            _ => Err(BroodrepError::MalformedHeader("invalid assigned race")),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    const NOT_A_REPLAY: &[u8] = include_bytes!("../testdata/not_a_replay.rep");
    const LEGACY: &[u8] = include_bytes!("../testdata/things.rep");
    const LEGACY_EMPTY: &[u8] = include_bytes!("../testdata/empty.rep");
    const SCR_OLD: &[u8] = include_bytes!("../testdata/scr_old.rep");
    const SCR_121: &[u8] = include_bytes!("../testdata/scr_replay.rep");

    #[test]
    fn test_replay_format_invalid() {
        let mut cursor = Cursor::new(NOT_A_REPLAY);
        assert!(matches!(
            Replay::new(&mut cursor),
            Err(BroodrepError::MalformedHeader(_))
        ));
    }

    #[test]
    fn test_replay_format_legacy() {
        let mut cursor = Cursor::new(LEGACY);
        let replay = Replay::new(&mut cursor).unwrap();
        assert_eq!(replay.format, ReplayFormat::Legacy);
    }

    #[test]
    fn test_replay_format_legacy_empty() {
        let mut cursor = Cursor::new(LEGACY_EMPTY);
        let replay = Replay::new(&mut cursor).unwrap();
        assert_eq!(replay.format, ReplayFormat::Legacy);
    }

    #[test]
    fn test_replay_format_scr_old() {
        let mut cursor = Cursor::new(SCR_OLD);
        let replay = Replay::new(&mut cursor).unwrap();
        assert_eq!(replay.format, ReplayFormat::Modern);
    }

    #[test]
    fn test_replay_format_scr_121() {
        let mut cursor = Cursor::new(SCR_121);
        let replay = Replay::new(&mut cursor).unwrap();
        assert_eq!(replay.format, ReplayFormat::Modern121);
    }

    #[test]
    fn replay_header_legacy() {
        let mut cursor = Cursor::new(LEGACY);
        let replay = Replay::new(&mut cursor).unwrap();

        assert_eq!(replay.header.engine, Engine::BroodWar);
        assert_eq!(replay.header.frames, 894);
        assert_eq!(replay.header.start_time, 1477230422);
        assert_eq!(replay.header.title, "neiv");
        assert_eq!(replay.header.map_width, 128);
        assert_eq!(replay.header.map_height, 128);
        assert_eq!(replay.header.available_slots, 4);
        assert_eq!(replay.header.speed, GameSpeed::Fastest);
        assert_eq!(replay.header.game_type, GameType::TopVsBottom);
        assert_eq!(replay.header.game_sub_type, 2);
        assert_eq!(replay.header.host_name, "neiv");
        assert_eq!(replay.header.map_name, "Shadowlands");
    }

    #[test]
    fn replay_header_scr_121() {
        let mut cursor = Cursor::new(SCR_121);
        let replay = Replay::new(&mut cursor).unwrap();

        assert_eq!(replay.header.engine, Engine::BroodWar);
        assert_eq!(replay.header.frames, 715);
        assert_eq!(replay.header.start_time, 1578881288);
        assert_eq!(replay.header.title, "u");
        assert_eq!(replay.header.map_width, 128);
        assert_eq!(replay.header.map_height, 112);
        assert_eq!(replay.header.available_slots, 2);
        assert_eq!(replay.header.speed, GameSpeed::Fastest);
        assert_eq!(replay.header.game_type, GameType::Melee);
        assert_eq!(replay.header.game_sub_type, 1);
        assert_eq!(replay.header.host_name, "");
        assert_eq!(
            replay.header.map_name,
            "\u{0007}제3세계(Third World) \u{0005}"
        );

        assert_eq!(replay.header.slots.len(), 12);
        assert_eq!(
            replay.header.slots[0],
            Player {
                slot_id: 0,
                network_id: 0,
                player_type: PlayerType::Human,
                name: "u".into(),
                race: Race::Terran,
                team: 1,
            }
        );
        assert!(!replay.header.slots[0].is_observer());
        assert_eq!(
            replay.header.slots[1],
            Player {
                slot_id: 1,
                network_id: 255,
                player_type: PlayerType::Computer,
                name: "Sargas Tribe".into(),
                race: Race::Protoss,
                team: 1,
            }
        );
        assert!(replay.header.slots[2].is_empty());

        let occupied = replay.header.players().collect::<Vec<_>>();
        assert_eq!(occupied.len(), 2);
        let observers = replay.header.observers().collect::<Vec<_>>();
        assert_eq!(observers.len(), 0);
    }
}
