use std::{
    collections::HashMap,
    ffi::CStr,
    fmt,
    io::{Cursor, Read, Seek, SeekFrom},
};

use byteorder::{LittleEndian as LE, ReadBytesExt as _};
use chrono::{DateTime, NaiveDateTime};
use explode::ExplodeReader;
use flate2::bufread::ZlibDecoder;
use thiserror::Error;

use crate::compression::SafeDecompressor;
pub use crate::compression::{DecompressionConfig, DecompressionError};
pub use crate::shieldbattery::{ShieldBatteryData, ShieldBatteryDataError};

mod compression;
mod shieldbattery;

#[derive(Error, Debug)]
pub enum BroodrepError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error("malformed header: {0}")]
    MalformedHeader(&'static str),
    #[error("problem decompressing data: {0}")]
    Decompression(#[from] DecompressionError),
    #[error("duplicate section found: {0:?}")]
    DuplicateSection(ReplaySection),
    #[error("shieldbattery data error: {0}")]
    ShieldBatteryData(#[from] shieldbattery::ShieldBatteryDataError),
}

/// A StarCraft replay, parsed from a [Read] implementation. Only the header will be parsed eagerly,
/// all other sections are processed/parsed on demand.
pub struct Replay<R: Read + Seek> {
    inner: R,
    decompression_config: DecompressionConfig,
    /// Offsets from the beginning of the file to the header for a particular section. For modern
    /// sections, this will be the offset of the raw data size. For legacy sections, it's the offset
    /// of the section header.
    section_offsets: HashMap<ReplaySection, u64>,
    pub format: ReplayFormat,
    pub header: ReplayHeader,
}

const SIZE_HEADER: usize = 0x279;
const SIZE_PLAYER_NAMES: usize = 0x300;
const SIZE_SKINS: usize = 0x15e0;
const SIZE_LIMITS: usize = 0x1c;
const SIZE_BFIX: usize = 0x08;
const SIZE_CUSTOM_COLORS: usize = 0xc0;
const SIZE_GCFG: usize = 0x19;

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

        let mut section_offsets = HashMap::new();

        section_offsets.insert(ReplaySection::Header, reader.stream_position()?);
        let replay_header =
            Self::read_legacy_section(&mut reader, format, config, Some(SIZE_HEADER))?;
        let replay_header = Self::parse_replay_header(&replay_header)?;

        let r = || -> Result<(), BroodrepError> {
            // NOTE(tec27): Dynamically sized legacy sections (commands, map data) have a section
            // before them that specifies their total uncompressed size, so we need to effectively
            // skip 2 sections for those
            Self::skip_legacy_section(&mut reader)?;
            section_offsets.insert(ReplaySection::Commands, reader.stream_position()?);
            Self::skip_legacy_section(&mut reader)?;

            Self::skip_legacy_section(&mut reader)?;
            section_offsets.insert(ReplaySection::MapData, reader.stream_position()?);
            Self::skip_legacy_section(&mut reader)?;

            section_offsets.insert(ReplaySection::PlayerNames, reader.stream_position()?);
            // TODO(tec27): Probably we should read this here and update the header player names as
            // needed
            Self::skip_legacy_section(&mut reader)?;

            // Modern sections
            if format != ReplayFormat::Legacy {
                loop {
                    let mut section_id = [0u8; 4];
                    reader.read_exact(&mut section_id)?;

                    let section: ReplaySection = section_id.into();
                    if section_offsets.contains_key(&section) {
                        // TODO(tec27): Should we actually handle this instead? No SC:R replay should
                        // ever have this but other clients might
                        return Err(BroodrepError::DuplicateSection(section));
                    }
                    section_offsets.insert(section, reader.stream_position()?);
                    let size = reader.read_u32::<LE>()?;
                    reader.seek(SeekFrom::Current(size as i64))?;
                }
            }

            Ok(())
        }();

        match r {
            Ok(_) => {}
            // Eof after the header is "ok", other sections are non-essential
            Err(BroodrepError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {}
            Err(e) => return Err(e),
        }

        Ok(Replay {
            inner: reader,
            decompression_config: config,
            format,
            section_offsets,
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

    /// Returns the raw bytes of a given replay section, or [None] if not present in the replay
    /// file. The bytes will be decompressed if it is a section with known compression.
    pub fn get_raw_section(
        &mut self,
        section: ReplaySection,
    ) -> Result<Option<Vec<u8>>, BroodrepError> {
        let offset = match self.section_offsets.get(&section) {
            Some(o) => *o,
            None => return Ok(None),
        };

        if section.is_modern() {
            self.inner.seek(SeekFrom::Start(offset))?;
            let size = self.inner.read_u32::<LE>()?;
            let mut data = vec![0; size as usize];
            self.inner.read_exact(&mut data)?;
            Ok(Some(data))
        } else {
            self.inner.seek(SeekFrom::Start(offset))?;
            let bytes = Self::read_legacy_section(
                &mut self.inner,
                self.format,
                self.decompression_config,
                section.size_hint(),
            )?;
            Ok(Some(bytes))
        }
    }

    /// Returns the parsed ShieldBattery data section, if present.
    pub fn get_shieldbattery_section(
        &mut self,
    ) -> Result<Option<ShieldBatteryData>, BroodrepError> {
        let data = match self.get_raw_section(ReplaySection::ShieldBattery)? {
            Some(d) => d,
            None => return Ok(None),
        };
        Ok(Some(shieldbattery::parse_shieldbattery_section(&data)?))
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
        size_hint: Option<usize>,
    ) -> Result<Vec<u8>, BroodrepError> {
        let header = Self::read_section_header(reader)?;
        let mut data = Vec::with_capacity(size_hint.unwrap_or(0));
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

    /// Reads the header and then skips over a section without parsing it.
    fn skip_legacy_section(reader: &mut R) -> Result<(), BroodrepError> {
        let header = Self::read_section_header(reader)?;
        for _ in 0..header.num_chunks {
            let size = reader.read_u32::<LE>()?;
            reader.seek(SeekFrom::Current(size as i64))?;
        }
        Ok(())
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
                let race: Race = cursor.read_u8()?.into();
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

impl fmt::Display for ReplayFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReplayFormat::Legacy => write!(f, "Legacy (pre-1.18)"),
            ReplayFormat::Modern => write!(f, "Modern (1.18-1.21)"),
            ReplayFormat::Modern121 => write!(f, "Modern (1.21+)"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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

    /// Any section that is not one of the "official" types, or directly supported by broodrep
    Custom([u8; 4]),
}

impl ReplaySection {
    /// Returns whether a section is "modern", meaning it was added in SC:R's replay format and
    /// includes a section ID + size before the actual data.
    pub fn is_modern(&self) -> bool {
        matches!(
            self,
            ReplaySection::Skins
                | ReplaySection::Limits
                | ReplaySection::Bfix
                | ReplaySection::CustomColors
                | ReplaySection::Gcfg
                | ReplaySection::ShieldBattery
                | ReplaySection::Custom(_)
        )
    }

    pub fn size_hint(&self) -> Option<usize> {
        match self {
            ReplaySection::Header => Some(SIZE_HEADER),
            ReplaySection::PlayerNames => Some(SIZE_PLAYER_NAMES),
            ReplaySection::Skins => Some(SIZE_SKINS),
            ReplaySection::Limits => Some(SIZE_LIMITS),
            ReplaySection::Bfix => Some(SIZE_BFIX),
            ReplaySection::CustomColors => Some(SIZE_CUSTOM_COLORS),
            ReplaySection::Gcfg => Some(SIZE_GCFG),
            _ => None,
        }
    }
}

impl From<&[u8; 4]> for ReplaySection {
    fn from(value: &[u8; 4]) -> Self {
        match value {
            b"SKIN" => ReplaySection::Skins,
            b"LMTS" => ReplaySection::Limits,
            b"BFIX" => ReplaySection::Bfix,
            b"CCLR" => ReplaySection::CustomColors,
            b"GCFG" => ReplaySection::Gcfg,
            b"Sbat" => ReplaySection::ShieldBattery,
            id => ReplaySection::Custom(*id),
        }
    }
}

impl From<[u8; 4]> for ReplaySection {
    fn from(value: [u8; 4]) -> Self {
        (&value).into()
    }
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

impl fmt::Display for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Engine::StarCraft => write!(f, "StarCraft"),
            Engine::BroodWar => write!(f, "Brood War"),
            Engine::Unknown(value) => write!(f, "Unknown ({})", value),
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

impl GameSpeed {
    /// Returns the duration per logical step for this game speed.
    /// These timing values are based on StarCraft's actual frame timings.
    pub fn time_per_step(self) -> std::time::Duration {
        let millis = match self {
            GameSpeed::Slowest => 167,
            GameSpeed::Slower => 111,
            GameSpeed::Slow => 83,
            GameSpeed::Normal => 67,
            GameSpeed::Fast => 56,
            GameSpeed::Faster => 48,
            GameSpeed::Fastest => 42,
        };
        std::time::Duration::from_millis(millis)
    }
}

impl fmt::Display for GameSpeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GameSpeed::Slowest => write!(f, "Slowest"),
            GameSpeed::Slower => write!(f, "Slower"),
            GameSpeed::Slow => write!(f, "Slow"),
            GameSpeed::Normal => write!(f, "Normal"),
            GameSpeed::Fast => write!(f, "Fast"),
            GameSpeed::Faster => write!(f, "Faster"),
            GameSpeed::Fastest => write!(f, "Fastest"),
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

impl fmt::Display for GameType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GameType::None => write!(f, "None"),
            GameType::Melee => write!(f, "Melee"),
            GameType::FreeForAll => write!(f, "Free For All"),
            GameType::OneOnOne => write!(f, "One on One"),
            GameType::CaptureTheFlag => write!(f, "Capture The Flag"),
            GameType::Greed => write!(f, "Greed"),
            GameType::Slaughter => write!(f, "Slaughter"),
            GameType::SuddenDeath => write!(f, "Sudden Death"),
            GameType::Ladder => write!(f, "Ladder"),
            GameType::UseMapSettings => write!(f, "Use Map Settings"),
            GameType::TeamMelee => write!(f, "Team Melee"),
            GameType::TeamFreeForAll => write!(f, "Team Free For All"),
            GameType::TeamCaptureTheFlag => write!(f, "Team Capture The Flag"),
            GameType::TopVsBottom => write!(f, "Top vs Bottom"),
            GameType::Unknown(value) => write!(f, "Unknown ({})", value),
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

impl fmt::Display for PlayerType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlayerType::Inactive => write!(f, "Inactive"),
            PlayerType::Computer => write!(f, "Computer"),
            PlayerType::Human => write!(f, "Human"),
            PlayerType::RescuePassive => write!(f, "Rescue Passive"),
            PlayerType::Unused => write!(f, "Unused"),
            PlayerType::ComputerControlled => write!(f, "Computer Controlled"),
            PlayerType::Open => write!(f, "Open"),
            PlayerType::Neutral => write!(f, "Neutral"),
            PlayerType::Closed => write!(f, "Closed"),
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

impl From<u8> for Race {
    fn from(value: u8) -> Self {
        match value {
            0 => Race::Zerg,
            1 => Race::Terran,
            2 => Race::Protoss,
            _ => Race::Random,
        }
    }
}

impl fmt::Display for Race {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Race::Zerg => write!(f, "Zerg"),
            Race::Terran => write!(f, "Terran"),
            Race::Protoss => write!(f, "Protoss"),
            Race::Random => write!(f, "Random"),
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
    const SB_DATA: &[u8] = include_bytes!("../testdata/sb_data.rep");

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

    #[test]
    fn replay_sections_legacy() {
        let mut cursor = Cursor::new(LEGACY);
        let replay = Replay::new(&mut cursor).unwrap();

        assert_eq!(
            replay.section_offsets.get(&ReplaySection::Header),
            Some(&16)
        );
        assert_eq!(
            replay.section_offsets.get(&ReplaySection::Commands),
            Some(&227)
        );
        assert_eq!(
            replay.section_offsets.get(&ReplaySection::MapData),
            Some(&482)
        );
        assert_eq!(
            replay.section_offsets.get(&ReplaySection::PlayerNames),
            Some(&74769)
        );
        assert_eq!(replay.section_offsets.get(&ReplaySection::Skins), None);
        assert_eq!(replay.section_offsets.get(&ReplaySection::Limits), None);
        assert_eq!(replay.section_offsets.get(&ReplaySection::Bfix), None);
        assert_eq!(
            replay.section_offsets.get(&ReplaySection::CustomColors),
            None
        );
        assert_eq!(replay.section_offsets.get(&ReplaySection::Gcfg), None);
        assert_eq!(
            replay.section_offsets.get(&ReplaySection::ShieldBattery),
            None
        );
    }

    #[test]
    fn replay_sections_scr_121() {
        let mut cursor = Cursor::new(SCR_121);
        let replay = Replay::new(&mut cursor).unwrap();

        assert_eq!(
            replay.section_offsets.get(&ReplaySection::Header),
            Some(&20)
        );
        assert_eq!(
            replay.section_offsets.get(&ReplaySection::Commands),
            Some(&225)
        );
        assert_eq!(
            replay.section_offsets.get(&ReplaySection::MapData),
            Some(&288)
        );
        assert_eq!(
            replay.section_offsets.get(&ReplaySection::PlayerNames),
            Some(&44123)
        );
        assert_eq!(
            replay.section_offsets.get(&ReplaySection::Skins),
            Some(&44222)
        );
        assert_eq!(
            replay.section_offsets.get(&ReplaySection::Limits),
            Some(&44282)
        );
        assert_eq!(
            replay.section_offsets.get(&ReplaySection::Bfix),
            Some(&44330)
        );
        assert_eq!(
            replay.section_offsets.get(&ReplaySection::CustomColors),
            Some(&44358)
        );
        assert_eq!(replay.section_offsets.get(&ReplaySection::Gcfg), None);
        assert_eq!(
            replay.section_offsets.get(&ReplaySection::ShieldBattery),
            None
        );
    }

    // TODO(tec27): Would be nice to have a test with SB data v0 as well
    #[test]
    fn shieldbattery_section_v1() {
        let mut cursor = Cursor::new(SB_DATA);
        let mut replay = Replay::new(&mut cursor).unwrap();

        assert_eq!(
            replay.section_offsets.get(&ReplaySection::ShieldBattery),
            Some(&33281)
        );

        let data = replay.get_shieldbattery_section();
        assert!(data.is_ok());
        let data = data.unwrap();
        assert!(data.is_some());
        let data = data.unwrap();

        assert_eq!(data.starcraft_exe_build, 13515);
        assert_eq!(data.shieldbattery_version, "10.1.0");
        assert_eq!(data.team_game_main_players, [0, 0, 0, 0]);
        assert_eq!(
            data.starting_races,
            [
                Race::Protoss,
                Race::Terran,
                Race::Zerg,
                Race::Terran,
                Race::Terran,
                Race::Zerg,
                Race::Terran,
                Race::Terran,
                Race::Zerg,
                Race::Zerg,
                Race::Zerg,
                Race::Zerg
            ]
        );
        assert_eq!(data.game_id, 56542772156747381282200559102402795521);
        assert_eq!(data.user_ids, [101, 112, 1, 113, 0, 0, 0, 0]);
        assert_eq!(data.game_logic_version, Some(3));
    }

    #[test]
    fn shieldbattery_section_missing() {
        let mut cursor = Cursor::new(SCR_121);
        let mut replay = Replay::new(&mut cursor).unwrap();

        assert_eq!(
            replay.section_offsets.get(&ReplaySection::ShieldBattery),
            None
        );

        let data = replay.get_shieldbattery_section();
        assert!(data.is_ok());
        let data = data.unwrap();
        assert!(data.is_none());
    }
}
