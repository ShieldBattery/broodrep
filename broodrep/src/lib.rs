use std::{
    ffi::CStr,
    io::{Cursor, Read, Seek, SeekFrom},
};

use byteorder::{LittleEndian as LE, ReadBytesExt as _};
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
            // This is the length of the "legacy" sections that don't have tags (so the `seRS`
            // magic is effectively its own tagged section). In older formats, the `reRS` is just
            // magic and no length is provided
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

        cursor.seek(SeekFrom::Current(3))?; // unknown/padding?

        let start_time = cursor.read_u32::<LE>()?;

        cursor.seek(SeekFrom::Current(12))?; // player bytes?

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
    // TODO(tec27): Player data
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
    }
}
