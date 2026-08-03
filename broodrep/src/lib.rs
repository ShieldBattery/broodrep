use std::{
    collections::{HashMap, HashSet},
    fmt,
    io::{Cursor, Read, Seek, SeekFrom},
    ops::ControlFlow,
};

use byteorder::{LittleEndian as LE, ReadBytesExt as _};
use chrono::{DateTime, NaiveDateTime};
use encoding_rs::{EUC_KR, WINDOWS_1252};
use explode::ExplodeReader;
use flate2::Decompress;
use thiserror::Error;

pub use crate::commands::{
    Command, CommandCategory, CommandKind, CommandParseConfig, CommandParseError, ReplayCommand,
};
pub use crate::compression::{DecompressionConfig, DecompressionError};
use crate::compression::{SafeDecompressor, decompress_zlib_into};
pub use crate::shieldbattery::{ShieldBatteryData, ShieldBatteryDataError};

pub mod commands;
mod compression;
pub mod shieldbattery;

#[derive(Error, Debug)]
pub enum BroodrepError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error("malformed header: {0}")]
    MalformedHeader(&'static str),
    #[error("problem decompressing data: {0}")]
    Decompression(#[from] DecompressionError),
    #[error("shieldbattery data error: {0}")]
    ShieldBatteryData(#[from] shieldbattery::ShieldBatteryDataError),
    #[error("command parse error: {0}")]
    CommandParse(#[from] commands::CommandParseError),
}

/// A StarCraft replay, parsed from a [Read] implementation. Only the header will be parsed eagerly,
/// all other sections are processed/parsed on demand.
pub struct Replay<R: Read + Seek> {
    inner: R,
    decompression_config: DecompressionConfig,
    /// Total length of the replay file in bytes.
    file_len: u64,
    /// Offsets from the beginning of the file to the header for a particular section. For modern
    /// sections, this will be the offset of the raw data size. For legacy sections, it's the offset
    /// of the section header.
    section_offsets: HashMap<ReplaySection, u64>,
    /// Dynamically-sized sections that are present in the replay but contain no data. In this case
    /// the file contains only their size specifier (with a value of 0), and the section itself
    /// (including its block header) is omitted entirely.
    empty_sections: HashSet<ReplaySection>,
    /// Uncompressed sizes for dynamically-sized sections, as declared by the size specifier
    /// section preceding them. Needed to tell raw chunks apart from compressed ones when reading.
    declared_sizes: HashMap<ReplaySection, u32>,
    /// Deferred legacy-section walk when it is not needed to locate modern sections. Legacy
    /// replays have no modern sections, while 1.21+ replays provide their offset directly, so
    /// header-only users of either format do not need to scan every legacy chunk up front.
    pending_legacy_walk: Option<(u64, u64)>,
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

/// Legacy section data is split into chunks of at most this many (uncompressed) bytes.
const MAX_CHUNK_SIZE: usize = 0x2000;
/// Maximum initial buffer capacity to trust from sizes declared within a replay file (buffers can
/// still grow past this while reading, this just avoids huge speculative allocations).
const MAX_SECTION_PREALLOC: usize = 8 * 1024 * 1024;

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
        let file_len = reader.seek(SeekFrom::End(0))?;
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
        // 1.21+ replays store the absolute offset of the first modern section directly after the
        // magic bytes, so that readers don't have to walk the dynamically-sized legacy sections to
        // find them. In older formats this offset is not present (even though the Modern non-1.21
        // version does have modern sections).
        let modern_start = if format == ReplayFormat::Modern121 {
            let offset = u64::from(reader.read_u32::<LE>()?);
            (offset >= reader.stream_position()? && offset <= file_len).then_some(offset)
        } else {
            None
        };

        let mut section_offsets = HashMap::new();
        let mut empty_sections = HashSet::new();
        let mut declared_sizes = HashMap::new();

        section_offsets.insert(ReplaySection::Header, reader.stream_position()?);
        let replay_header =
            Self::read_legacy_section(&mut reader, format, config, Some(SIZE_HEADER), file_len)?;
        let replay_header =
            Self::parse_replay_header(&replay_header, TextEncoding::for_format(format))?;

        let legacy_start = reader.stream_position()?;
        let defer_legacy_walk = match format {
            ReplayFormat::Legacy => true,
            ReplayFormat::Modern => false,
            ReplayFormat::Modern121 => modern_start.is_some(),
        };

        // Walk the legacy sections. These are non-essential, so if the walk fails at some point we
        // keep whatever sections were successfully located before the failure.
        let legacy_limit = modern_start.unwrap_or(file_len);
        let legacy_end = (!defer_legacy_walk).then(|| {
            Self::walk_legacy_sections(
                &mut reader,
                legacy_limit,
                &mut section_offsets,
                &mut empty_sections,
                &mut declared_sizes,
            )
        });

        // Walk the modern sections. 1.21+ replays tell us where these start directly; for older
        // modern replays they begin right after the legacy sections (if that walk succeeded).
        let modern_walk_start = match format {
            ReplayFormat::Legacy => None,
            ReplayFormat::Modern => legacy_end
                .as_ref()
                .and_then(|result| result.as_ref().ok().copied()),
            ReplayFormat::Modern121 => modern_start.or_else(|| {
                legacy_end
                    .as_ref()
                    .and_then(|result| result.as_ref().ok().copied())
            }),
        };
        if let Some(start) = modern_walk_start {
            let _ = || -> Result<(), BroodrepError> {
                let mut pos = start;
                while pos + 8 <= file_len {
                    reader.seek(SeekFrom::Start(pos))?;
                    let mut section_id = [0u8; 4];
                    reader.read_exact(&mut section_id)?;
                    let size = u64::from(reader.read_u32::<LE>()?);
                    let end = pos + 8 + size;
                    if end > file_len {
                        // Truncated/corrupt section, and we can't trust the rest of the chain
                        break;
                    }
                    // NOTE(tec27): No SC:R replay should ever have duplicate sections, but if some
                    // other client writes one, we keep the first occurrence
                    section_offsets.entry(section_id.into()).or_insert(pos + 4);
                    pos = end;
                }
                Ok(())
            }();
        }

        Ok(Replay {
            inner: reader,
            decompression_config: config,
            file_len,
            format,
            section_offsets,
            empty_sections,
            declared_sizes,
            pending_legacy_walk: defer_legacy_walk.then_some((legacy_start, legacy_limit)),
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

    /// Reads the raw bytes of a replay section, or [None] if it is not present. The bytes are
    /// decompressed if the section uses known compression.
    ///
    /// This operation is intentionally uncached: every call seeks to the section and returns a new
    /// allocation. Applications that need the same section repeatedly should retain the returned
    /// bytes or build an application-specific processed replay type.
    pub fn read_raw_section(
        &mut self,
        section: ReplaySection,
    ) -> Result<Option<Vec<u8>>, BroodrepError> {
        if !section.is_modern() && section != ReplaySection::Header {
            self.ensure_legacy_sections_indexed()?;
        }
        if self.empty_sections.contains(&section) {
            return Ok(Some(Vec::new()));
        }
        let offset = match self.section_offsets.get(&section) {
            Some(o) => *o,
            None => return Ok(None),
        };

        if section.is_modern() {
            // TODO(tec27): Blizzard's modern sections (LMTS, SKIN, BFIX, CCLR, GCFG) wrap their
            // payload in the same checksum + num_chunks + chunk format as legacy sections. This
            // code returns the raw bytes including that inner header, so callers (like get_limits)
            // must parse it themselves. We should generalize the inner header parsing here for
            // Blizzard sections, while keeping raw passthrough for custom sections (like Sbat).
            self.inner.seek(SeekFrom::Start(offset))?;
            let size = self.inner.read_u32::<LE>()?;
            if u64::from(size) > self.file_len.saturating_sub(offset.saturating_add(4)) {
                return Err(BroodrepError::MalformedHeader(
                    "section extends past end of file",
                ));
            }
            let mut data = vec![0; size as usize];
            self.inner.read_exact(&mut data)?;
            Ok(Some(data))
        } else {
            // The expected uncompressed size comes from either the fixed size for this section
            // type, or the size section that preceded it in the file
            let expected_size = section
                .size_hint()
                .or_else(|| self.declared_sizes.get(&section).map(|&s| s as usize));
            self.inner.seek(SeekFrom::Start(offset))?;
            let bytes = Self::read_legacy_section(
                &mut self.inner,
                self.format,
                self.decompression_config,
                expected_size,
                self.file_len,
            )?;
            Ok(Some(bytes))
        }
    }

    /// Compatibility alias for [`Replay::read_raw_section`].
    pub fn get_raw_section(
        &mut self,
        section: ReplaySection,
    ) -> Result<Option<Vec<u8>>, BroodrepError> {
        self.read_raw_section(section)
    }

    /// Returns the game limits from the LMTS section. Returns default (classic BW 1.16) limits
    /// if the section is not present (pre-1.21 replays).
    pub fn get_limits(&mut self) -> Result<GameLimits, BroodrepError> {
        let data = match self.read_raw_section(ReplaySection::Limits)? {
            Some(d) => d,
            None => return Ok(GameLimits::default()),
        };
        // Blizzard modern sections wrap their payload in the same header format as legacy
        // sections: SectionHeader (checksum + num_chunks), then each chunk has size(4) +
        // data(size). Parse through that to get the actual limits payload.
        let mut reader = std::io::Cursor::new(&data);
        let header = SectionHeader::read(&mut reader)?;
        let mut payload = Vec::with_capacity(SIZE_LIMITS);
        for _ in 0..header.num_chunks {
            let chunk_size = reader.read_u32::<LE>()? as usize;
            let pos = reader.position() as usize;
            if pos + chunk_size > data.len() {
                return Ok(GameLimits::default());
            }
            payload.extend_from_slice(&data[pos..pos + chunk_size]);
            reader.set_position((pos + chunk_size) as u64);
        }
        if payload.len() < SIZE_LIMITS {
            return Ok(GameLimits::default());
        }
        let mut cursor = std::io::Cursor::new(&payload);
        Ok(GameLimits {
            images: cursor.read_u32::<LE>()?,
            sprites: cursor.read_u32::<LE>()?,
            lone_sprites: cursor.read_u32::<LE>()?,
            units: cursor.read_u32::<LE>()?,
            bullets: cursor.read_u32::<LE>()?,
            orders: cursor.read_u32::<LE>()?,
            fow_sprites: cursor.read_u32::<LE>()?,
        })
    }

    /// Returns the parsed ShieldBattery data section, if present.
    pub fn get_shieldbattery_section(
        &mut self,
    ) -> Result<Option<ShieldBatteryData>, BroodrepError> {
        let offset = match self.section_offsets.get(&ReplaySection::ShieldBattery) {
            Some(&offset) => offset,
            None => return Ok(None),
        };

        // Sbat is an extensible modern section, but the typed parser only needs the fixed prefix
        // it currently understands. Avoid allocating and reading arbitrary trailing extension data.
        self.inner.seek(SeekFrom::Start(offset))?;
        let size = self.inner.read_u32::<LE>()?;
        if u64::from(size) > self.file_len.saturating_sub(offset.saturating_add(4)) {
            return Err(BroodrepError::MalformedHeader(
                "section extends past end of file",
            ));
        }
        let mut data = [0u8; shieldbattery::PARSED_PREFIX_SIZE];
        let prefix_len = (size as usize).min(data.len());
        self.inner.read_exact(&mut data[..prefix_len])?;

        Ok(Some(shieldbattery::parse_shieldbattery_section(
            &data[..prefix_len],
        )?))
    }

    /// Reads and parses the commands from the replay using the default command limits, or returns
    /// [None] if the commands section is not present.
    ///
    /// This operation is intentionally uncached and repeats section decompression and parsing on
    /// every call. Retain the returned commands when they will be queried more than once.
    pub fn read_commands(&mut self) -> Result<Option<Vec<ReplayCommand>>, BroodrepError> {
        self.read_commands_with_config(CommandParseConfig::default())
    }

    /// Reads and parses the commands from the replay subject to `config`.
    pub fn read_commands_with_config(
        &mut self,
        config: CommandParseConfig,
    ) -> Result<Option<Vec<ReplayCommand>>, BroodrepError> {
        let data = match self.read_raw_section(ReplaySection::Commands)? {
            Some(d) => d,
            None => return Ok(None),
        };
        Ok(Some(commands::parse_commands_with_config(
            &data,
            TextEncoding::for_format(self.format),
            config,
        )?))
    }

    /// Reads the command section and invokes `visitor` once per parsed command without retaining a
    /// complete command list. Return [`ControlFlow::Break`] to stop parsing deliberately.
    ///
    /// The decompressed command bytes are still held for the duration of this call, but individual
    /// commands are dropped after the visitor returns unless the visitor retains them.
    pub fn visit_commands<F>(
        &mut self,
        visitor: F,
    ) -> Result<Option<ControlFlow<()>>, BroodrepError>
    where
        F: FnMut(ReplayCommand) -> ControlFlow<()>,
    {
        self.visit_commands_with_config(CommandParseConfig::default(), visitor)
    }

    /// Visits commands subject to `config` without collecting a complete command list.
    pub fn visit_commands_with_config<F>(
        &mut self,
        config: CommandParseConfig,
        visitor: F,
    ) -> Result<Option<ControlFlow<()>>, BroodrepError>
    where
        F: FnMut(ReplayCommand) -> ControlFlow<()>,
    {
        let data = match self.read_raw_section(ReplaySection::Commands)? {
            Some(d) => d,
            None => return Ok(None),
        };
        Ok(Some(commands::visit_commands_with_config(
            &data,
            TextEncoding::for_format(self.format),
            config,
            visitor,
        )?))
    }

    /// Compatibility alias for [`Replay::read_commands`].
    pub fn get_commands(&mut self) -> Result<Option<Vec<ReplayCommand>>, BroodrepError> {
        self.read_commands()
    }

    /// Compatibility alias for [`Replay::read_commands_with_config`].
    pub fn get_commands_with_config(
        &mut self,
        config: CommandParseConfig,
    ) -> Result<Option<Vec<ReplayCommand>>, BroodrepError> {
        self.read_commands_with_config(config)
    }

    fn ensure_legacy_sections_indexed(&mut self) -> Result<(), BroodrepError> {
        let Some((start, limit)) = self.pending_legacy_walk.take() else {
            return Ok(());
        };

        self.inner.seek(SeekFrom::Start(start))?;
        // Match eager initialization behavior: legacy auxiliary sections are non-essential, so a
        // malformed walk keeps whatever offsets were discovered before the failure.
        let _ = Self::walk_legacy_sections(
            &mut self.inner,
            limit,
            &mut self.section_offsets,
            &mut self.empty_sections,
            &mut self.declared_sizes,
        );
        Ok(())
    }

    fn walk_legacy_sections(
        reader: &mut R,
        limit: u64,
        section_offsets: &mut HashMap<ReplaySection, u64>,
        empty_sections: &mut HashSet<ReplaySection>,
        declared_sizes: &mut HashMap<ReplaySection, u32>,
    ) -> Result<u64, BroodrepError> {
        // Dynamically sized legacy sections (commands, map data) have a section before them that
        // specifies their total uncompressed size. If that size is 0, the section itself is
        // omitted from the file entirely (not even its block header is written).
        for section in [ReplaySection::Commands, ReplaySection::MapData] {
            let size = Self::read_size_section(reader)?;
            if size > 0 {
                let pos = reader.stream_position()?;
                Self::skip_legacy_section(reader, limit)?;
                section_offsets.insert(section, pos);
                declared_sizes.insert(section, size);
            } else {
                empty_sections.insert(section);
            }
        }

        // The player names section is only present if there is room before the modern sections/end
        // of file. Legacy replays do not contain it at all.
        let pos = reader.stream_position()?;
        if pos < limit {
            // TODO(tec27): Read this and update header player names as needed.
            Self::skip_legacy_section(reader, limit)?;
            section_offsets.insert(ReplaySection::PlayerNames, pos);
        }

        Ok(reader.stream_position()?)
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
        SectionHeader::read(reader)
    }

    /// Reads a legacy-style section (checksum + chunk count header, followed by chunks),
    /// decompressing chunks as necessary. `expected_size` is the total uncompressed size of the
    /// section, if known (either a fixed size for the section type, or the value from the size
    /// section preceding it): chunks whose stored size matches their expected uncompressed size
    /// are stored raw rather than compressed, so this is needed to read such chunks correctly.
    fn read_legacy_section(
        reader: &mut R,
        format: ReplayFormat,
        config: DecompressionConfig,
        expected_size: Option<usize>,
        file_len: u64,
    ) -> Result<Vec<u8>, BroodrepError> {
        let header = Self::read_section_header(reader)?;
        let chunks_start = reader.stream_position()?;
        // Sanity check the chunk count before trusting it (each chunk needs at least a 4-byte
        // size), so that garbage data can't cause excessive looping
        if u64::from(header.num_chunks) > file_len.saturating_sub(chunks_start) / 4 {
            return Err(BroodrepError::MalformedHeader(
                "invalid section chunk count",
            ));
        }
        let mut data = Vec::with_capacity(expected_size.unwrap_or(0).min(MAX_SECTION_PREALLOC));
        let mut compressed = Vec::new();
        let mut zlib = None;
        let mut decompression_scratch = [0u8; MAX_CHUNK_SIZE];
        let mut remaining = file_len.saturating_sub(chunks_start);
        for _ in 0..header.num_chunks {
            let size = reader.read_u32::<LE>()?;
            remaining = remaining.saturating_sub(4);
            // Bounds check against the file size so that garbage sizes can't cause huge
            // allocations (which panic/abort rather than erroring)
            if u64::from(size) > remaining {
                return Err(BroodrepError::MalformedHeader(
                    "section extends past end of file",
                ));
            }
            remaining -= u64::from(size);

            // A chunk is stored raw (not compressed) exactly when its stored size matches its
            // expected uncompressed size
            let expected_chunk_size =
                expected_size.map(|total| total.saturating_sub(data.len()).min(MAX_CHUNK_SIZE));
            if expected_chunk_size == Some(size as usize) {
                // Read known raw chunks straight into the output buffer. This avoids both a
                // short-lived allocation and a second copy through the compressed scratch buffer.
                let start = data.len();
                let end =
                    start
                        .checked_add(size as usize)
                        .ok_or(BroodrepError::MalformedHeader(
                            "section size overflows address space",
                        ))?;
                data.resize(end, 0);
                reader.read_exact(&mut data[start..end])?;
                continue;
            }

            compressed.resize(size as usize, 0);
            reader.read_exact(&mut compressed)?;

            let is_raw = match expected_chunk_size {
                Some(_) => false,
                // Without a known uncompressed size, fall back to heuristics: legacy compression
                // (implode) has no reliable signature, but zlib streams start with 0x78
                None => match format {
                    ReplayFormat::Legacy => false,
                    ReplayFormat::Modern | ReplayFormat::Modern121 => {
                        size <= 4 || compressed[0] != 0x78
                    }
                },
            };

            if is_raw {
                data.extend_from_slice(&compressed);
            } else {
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
                        decompress_zlib_into(
                            zlib.get_or_insert_with(|| Decompress::new(true)),
                            &compressed,
                            &mut data,
                            &mut decompression_scratch,
                            config,
                        )?;
                    }
                }
            }
        }

        Ok(data)
    }

    /// Reads a "size" section: a legacy section whose 4-byte payload specifies the uncompressed
    /// length of the dynamically-sized section that follows it. If the returned size is 0, the
    /// following section is omitted from the file entirely (not even its block header is written).
    fn read_size_section(reader: &mut R) -> Result<u32, BroodrepError> {
        let header = Self::read_section_header(reader)?;
        if header.num_chunks != 1 {
            return Err(BroodrepError::MalformedHeader("invalid size section"));
        }
        let chunk_size = reader.read_u32::<LE>()?;
        if chunk_size != 4 {
            // A 4-byte payload always has the same stored size as its uncompressed size, so it is
            // never compressed
            return Err(BroodrepError::MalformedHeader("invalid size section"));
        }
        Ok(reader.read_u32::<LE>()?)
    }

    /// Reads the header and then skips over a section without parsing it, ensuring the section
    /// doesn't extend past `limit` (an absolute file offset).
    fn skip_legacy_section(reader: &mut R, limit: u64) -> Result<(), BroodrepError> {
        let header = Self::read_section_header(reader)?;
        let mut pos = reader.stream_position()?;
        // Sanity check the chunk count before trusting it (each chunk needs at least a 4-byte
        // size), so that garbage data can't cause excessive looping
        if u64::from(header.num_chunks) > limit.saturating_sub(pos) / 4 {
            return Err(BroodrepError::MalformedHeader(
                "invalid section chunk count",
            ));
        }
        for _ in 0..header.num_chunks {
            let size = reader.read_u32::<LE>()?;
            pos += 4;
            let end = pos + u64::from(size);
            if end > limit {
                return Err(BroodrepError::MalformedHeader(
                    "section extends past its bounds",
                ));
            }
            reader.seek(SeekFrom::Start(end))?;
            pos = end;
        }
        Ok(())
    }

    fn parse_replay_header(
        data: &[u8],
        encoding: TextEncoding,
    ) -> Result<ReplayHeader, BroodrepError> {
        let mut cursor = Cursor::new(data);
        let engine = cursor.read_u8()?.into();
        let frames = cursor.read_u32::<LE>()?;

        cursor.seek(SeekFrom::Current(3))?; // replay_campaign_mission + 0x48 (lobby init command)

        let start_time = cursor.read_u32::<LE>()?;

        cursor.seek(SeekFrom::Current(12))?; // player bytes

        let mut title = [0u8; 28];
        cursor.read_exact(&mut title)?;
        let title = encoding.decode(&title);

        let map_width = cursor.read_u16::<LE>()?;
        let map_height = cursor.read_u16::<LE>()?;
        cursor.seek(SeekFrom::Current(1))?; // unused/padding?
        let available_slots = cursor.read_u8()?;
        let speed = cursor.read_u8()?.try_into()?;
        cursor.seek(SeekFrom::Current(1))?; // unused/padding?
        let game_type = cursor.read_u16::<LE>()?.into();
        let game_sub_type = cursor.read_u16::<LE>()?;

        cursor.seek(SeekFrom::Current(8))?; // unknown

        let mut host_name = [0u8; 24];
        cursor.read_exact(&mut host_name)?;
        let host_name = encoding.decode(&host_name);

        cursor.seek(SeekFrom::Current(1))?; // unknown

        let mut map_name = [0u8; 32];
        cursor.read_exact(&mut map_name)?;
        let map_name = encoding.decode(&map_name);

        cursor.seek(SeekFrom::Current(32))?; // unknown

        let players = (0..12)
            .map(|_i| {
                let slot_id = cursor.read_u16::<LE>()?;
                cursor.seek(SeekFrom::Current(2))?; // unknown
                let network_id = cursor.read_u8()?;
                cursor.seek(SeekFrom::Current(3))?; // unknown
                let player_type: PlayerType = cursor.read_u8()?.try_into()?;
                let race: Race = cursor.read_u8()?.into();
                let team = cursor.read_u8()?;
                let mut name = [0u8; 25];
                cursor.read_exact(&mut name)?;
                let name = encoding.decode(&name);

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

/// The character encoding used to decode strings in a replay.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, Hash)]
pub enum TextEncoding {
    /// Strings are decoded as UTF-8, with invalid sequences replaced by U+FFFD. Modern (SC:R)
    /// replays always write UTF-8 strings.
    #[default]
    Utf8,
    /// The encoding is unknown; legacy replays wrote strings using the host machine's system
    /// codepage without recording which one it was. Each string is decoded as UTF-8 if it
    /// validates as such, otherwise as cp949 (Korean being by far the most common non-Western
    /// locale for Brood War) if it validates as such, otherwise as windows-1252.
    Legacy,
}

impl TextEncoding {
    /// Returns the appropriate encoding for strings in a replay of the given format.
    pub fn for_format(format: ReplayFormat) -> Self {
        match format {
            ReplayFormat::Legacy => TextEncoding::Legacy,
            ReplayFormat::Modern | ReplayFormat::Modern121 => TextEncoding::Utf8,
        }
    }

    /// Decodes a (possibly nul-terminated) byte slice into a string, ignoring anything from the
    /// first nul byte (if present) onward.
    pub fn decode(self, bytes: &[u8]) -> String {
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        let bytes = &bytes[..end];
        match self {
            TextEncoding::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
            TextEncoding::Legacy => {
                if let Ok(s) = str::from_utf8(bytes) {
                    s.to_owned()
                } else if let Some(s) =
                    // NOTE(tec27): encoding_rs' EUC_KR is the WHATWG version, which actually
                    // matches windows-949 (i.e. cp949, EUC-KR + extensions)
                    EUC_KR.decode_without_bom_handling_and_without_replacement(bytes)
                {
                    s.into_owned()
                } else {
                    // windows-1252 decoding never fails (all byte values are mapped), so this is
                    // the final fallback
                    WINDOWS_1252
                        .decode_without_bom_handling(bytes)
                        .0
                        .into_owned()
                }
            }
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

impl SectionHeader {
    fn read(reader: &mut impl byteorder::ReadBytesExt) -> Result<Self, BroodrepError> {
        Ok(Self {
            checksum: reader.read_u32::<LE>()?,
            num_chunks: reader.read_u32::<LE>()?,
        })
    }
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

/// Game object limits from the LMTS replay section. These control the sizes of various internal
/// arrays in SC:R and affect unit tag encoding (11-bit indices for ≤1700 units, 13-bit for >1700).
/// Classic (pre-1.21) replays don't have this section and use hardcoded 1.16 limits.
#[derive(Debug, Clone, Copy)]
pub struct GameLimits {
    pub images: u32,
    pub sprites: u32,
    pub lone_sprites: u32,
    pub units: u32,
    pub bullets: u32,
    pub orders: u32,
    pub fow_sprites: u32,
}

impl Default for GameLimits {
    /// Returns the classic BW 1.16 limits (pre-Remastered).
    fn default() -> Self {
        Self {
            images: 5000,
            sprites: 2500,
            lone_sprites: 256,
            units: 1700,
            bullets: 100,
            orders: 2000,
            fow_sprites: 512,
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
    /// Returns an iterator over all of the actively-playing slots in the game (humans and
    /// computers, not including observers). Note that this excludes slot types that don't
    /// actually participate in the game (see [Player::is_active]); use [ReplayHeader::slots] if
    /// you need those as well.
    pub fn players(&self) -> impl Iterator<Item = &Player> {
        self.slots
            .iter()
            .filter(|p| p.is_active() && !p.is_observer())
    }

    /// Returns an iterator over all of the filled observer slots in the game.
    pub fn observers(&self) -> impl Iterator<Item = &Player> {
        self.slots
            .iter()
            .filter(|p| p.is_active() && p.is_observer())
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

    /// Returns true if this [Player] is actively participating in the game as a human or computer
    /// player. UMS maps can also contain other kinds of occupied slots (e.g. inactive or
    /// rescuable "Computer" slots) which are not actual participants.
    pub fn is_active(&self) -> bool {
        !self.is_empty() && matches!(self.player_type, PlayerType::Human | PlayerType::Computer)
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
    const SCR_MAPNAME32: &[u8] = include_bytes!("../testdata/scr_mapname32.rep");
    const LEGACY_UTF8: &[u8] = include_bytes!("../testdata/legacy_utf8.rep");
    const LEGACY_CP1252: &[u8] = include_bytes!("../testdata/legacy_cp1252.rep");
    const LEGACY_CP949: &[u8] = include_bytes!("../testdata/legacy_cp949.rep");
    const SB_GAME_ID: &[u8] = include_bytes!("../testdata/sb_game_id.rep");
    const SCR_EMPTY_COMMANDS: &[u8] = include_bytes!("../testdata/scr_empty_commands.rep");
    const LEGACY_UMS_INACTIVE: &[u8] = include_bytes!("../testdata/legacy_ums_inactive.rep");
    const LEGACY_RAW_COMMANDS: &[u8] = include_bytes!("../testdata/legacy_raw_commands.rep");
    const LONG_HUNTERS: &[u8] = include_bytes!("../testdata/long_hunters.rep");

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
            "\u{0007}제3세계(Third World) \u{0005}1.0"
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
        let mut replay = Replay::new(&mut cursor).unwrap();

        assert!(replay.pending_legacy_walk.is_some());
        assert_eq!(replay.section_offsets.get(&ReplaySection::Commands), None);
        replay.ensure_legacy_sections_indexed().unwrap();
        assert!(replay.pending_legacy_walk.is_none());

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
        // Legacy replays don't contain a player names section (the map data section runs right up
        // to the end of the file)
        assert_eq!(
            replay.section_offsets.get(&ReplaySection::PlayerNames),
            None
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
        let mut replay = Replay::new(&mut cursor).unwrap();

        assert!(replay.pending_legacy_walk.is_some());
        assert_eq!(replay.section_offsets.get(&ReplaySection::Commands), None);
        replay.ensure_legacy_sections_indexed().unwrap();
        assert!(replay.pending_legacy_walk.is_none());

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
        // As a UUID: 019878ca-6a88-7ebb-9b93-20d6e6bd892a (a valid UUIDv7)
        assert_eq!(data.game_id, 0x019878ca_6a88_7ebb_9b93_20d6e6bd892a);
        assert_eq!(data.user_ids, [101, 112, 1, 113, 0, 0, 0, 0]);
        assert_eq!(data.game_logic_version, Some(3));
    }

    #[test]
    fn shieldbattery_typed_read_ignores_future_extension_bytes() {
        const SBAT_SIZE_OFFSET: usize = 33281;
        let mut bytes = SB_DATA.to_vec();
        let extension_size = 1024 * 1024;
        let original_size = u32::from_le_bytes(
            bytes[SBAT_SIZE_OFFSET..SBAT_SIZE_OFFSET + 4]
                .try_into()
                .unwrap(),
        );
        bytes[SBAT_SIZE_OFFSET..SBAT_SIZE_OFFSET + 4]
            .copy_from_slice(&(original_size + extension_size).to_le_bytes());
        bytes.resize(bytes.len() + extension_size as usize, 0xaa);

        let mut replay = Replay::new(Cursor::new(bytes)).unwrap();
        let data = replay.get_shieldbattery_section().unwrap().unwrap();
        assert_eq!(data.shieldbattery_version, "10.1.0");

        let cursor = replay.into_inner();
        assert_eq!(
            cursor.position(),
            SBAT_SIZE_OFFSET as u64 + 4 + shieldbattery::PARSED_PREFIX_SIZE as u64
        );
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

    #[test]
    fn commands_legacy() {
        let cursor = Cursor::new(LEGACY);
        let mut replay = Replay::new(cursor).unwrap();

        let commands = replay.get_commands().unwrap().unwrap();
        assert!(!commands.is_empty());

        // First command: frame 34, player 0, Select 7 units
        assert_eq!(commands[0].frame, 34);
        assert_eq!(commands[0].player_id, 0);
        assert_eq!(
            commands[0].command,
            Command::Select {
                unit_tags: vec![0x0e48, 0x0e49, 0x0e4a, 0x0e38, 0x0e4d, 0x0e4e, 0x0e4b]
            }
        );

        // Second command: frame 141, player 1, Chat "rip rap nib nab"
        assert_eq!(commands[1].frame, 141);
        assert_eq!(commands[1].player_id, 1);
        match &commands[1].command {
            Command::Chat {
                sender_slot,
                message,
            } => {
                assert_eq!(*sender_slot, 3);
                assert_eq!(message, "rip rap nib nab");
            }
            other => panic!("expected Chat, got {:?}", other),
        }

        // Third command: frame 174, player 0, TargetedOrder
        assert_eq!(commands[2].frame, 174);
        assert_eq!(commands[2].player_id, 0);
        assert_eq!(
            commands[2].command,
            Command::TargetedOrder {
                x: 0x0783,
                y: 0x0ed2,
                target_unit_tag: 0x0e4f,
                target_unit_type: 0x00e4,
                order: 0x08,
                queued: false,
            }
        );

        // Fourth command: frame 192, player 1, Select 4 units
        assert_eq!(commands[3].frame, 192);
        assert_eq!(commands[3].player_id, 1);
        assert_eq!(
            commands[3].command,
            Command::Select {
                unit_tags: vec![0x0e39, 0x0e3a, 0x0e3b, 0x0e3c]
            }
        );

        // Check that there's a Train command for player 1 (unit type 7)
        let train_cmds: Vec<_> = commands
            .iter()
            .filter(|c| matches!(c.command, Command::Train { .. }))
            .collect();
        assert_eq!(train_cmds.len(), 2);
        assert_eq!(train_cmds[0].command, Command::Train { unit_type: 0x0007 });
        assert_eq!(train_cmds[0].player_id, 1);

        // Check that there's a UnitMorph command
        let morph_cmds: Vec<_> = commands
            .iter()
            .filter(|c| matches!(c.command, Command::UnitMorph { .. }))
            .collect();
        assert_eq!(morph_cmds.len(), 1);
        assert_eq!(
            morph_cmds[0].command,
            Command::UnitMorph { unit_type: 0x0029 }
        );

        // All frames should be monotonically non-decreasing
        for w in commands.windows(2) {
            assert!(
                w[0].frame <= w[1].frame,
                "frames not monotonic: {} > {} at commands {:?} and {:?}",
                w[0].frame,
                w[1].frame,
                w[0],
                w[1]
            );
        }

        // All frames should be within the replay's total frames
        let max_frame = commands.iter().map(|c| c.frame).max().unwrap();
        assert!(
            max_frame <= replay.frames(),
            "max frame {max_frame} exceeds replay frames {}",
            replay.frames()
        );
    }

    #[test]
    fn replay_command_visitor_matches_collected_commands() {
        let mut collecting_replay = Replay::new(Cursor::new(LEGACY)).unwrap();
        let expected = collecting_replay.read_commands().unwrap().unwrap();

        let mut visiting_replay = Replay::new(Cursor::new(LEGACY)).unwrap();
        let mut visited = Vec::new();
        let outcome = visiting_replay
            .visit_commands(|command| {
                visited.push(command);
                ControlFlow::Continue(())
            })
            .unwrap();

        assert_eq!(outcome, Some(ControlFlow::Continue(())));
        assert_eq!(visited, expected);
    }

    #[test]
    fn commands_scr_121() {
        let cursor = Cursor::new(SCR_121);
        let mut replay = Replay::new(cursor).unwrap();

        let commands = replay.get_commands().unwrap().unwrap();
        assert!(!commands.is_empty());

        // First command: frame 170, player 0, Select121 with 1 unit
        // Wire format: u16 tag (0x2cfe) + u16 unknown (0x0000)
        assert_eq!(commands[0].frame, 170);
        assert_eq!(commands[0].player_id, 0);
        assert_eq!(
            commands[0].command,
            Command::Select121 {
                unit_tags: vec![0x2cfe]
            }
        );

        // Second command: frame 304, player 0, Select121
        assert_eq!(commands[1].frame, 304);
        assert_eq!(commands[1].player_id, 0);
        assert_eq!(
            commands[1].command,
            Command::Select121 {
                unit_tags: vec![0x2cd7]
            }
        );

        // Third command: frame 461, player 0, Cheat
        assert_eq!(commands[2].frame, 461);
        assert_eq!(commands[2].player_id, 0);
        assert_eq!(commands[2].command, Command::Cheat { flags: 0x00000001 });

        assert_eq!(commands.len(), 3);
    }

    #[test]
    fn commands_long_multiplayer_fixture_fit_default_limits() {
        let mut replay = Replay::new(Cursor::new(LONG_HUNTERS)).unwrap();

        assert_eq!(replay.frames(), 56_209);
        let player_network_ids: std::collections::HashSet<_> =
            replay.players().map(|player| player.network_id).collect();
        let commands = replay.read_commands().unwrap().unwrap();
        assert_eq!(commands.len(), 36_167);
        assert!(
            commands
                .iter()
                .all(|command| player_network_ids.contains(&command.player_id))
        );
    }

    #[test]
    fn commands_empty_replay() {
        let cursor = Cursor::new(LEGACY_EMPTY);
        let mut replay = Replay::new(cursor).unwrap();

        // This replay has no commands, so the file omits the commands section entirely (only its
        // size specifier, with a value of 0, is present)
        assert_eq!(
            replay.get_raw_section(ReplaySection::Commands).unwrap(),
            Some(vec![])
        );
        assert_eq!(replay.get_commands().unwrap(), Some(vec![]));

        // The map data section should still be located (and readable) despite the omitted
        // commands section before it
        let map_data = replay.get_raw_section(ReplaySection::MapData).unwrap();
        assert!(map_data.is_some_and(|d| !d.is_empty()));
    }

    #[test]
    fn commands_scr_old() {
        let cursor = Cursor::new(SCR_OLD);
        let mut replay = Replay::new(cursor).unwrap();

        let commands = replay.get_commands().unwrap();
        // Should parse without errors regardless of content
        assert!(commands.is_some());
    }

    #[test]
    fn command_type_ids() {
        // Verify that Command::type_id() returns the correct values
        assert_eq!(Command::Select { unit_tags: vec![] }.type_id(), 0x09);
        assert_eq!(
            Command::RightClick {
                x: 0,
                y: 0,
                target_unit_tag: 0,
                target_unit_type: 0,
                queued: false,
            }
            .type_id(),
            0x14
        );
        assert_eq!(Command::Train { unit_type: 0 }.type_id(), 0x1f);
        assert_eq!(Command::Stop { queued: false }.type_id(), 0x1a);
        assert_eq!(Command::HoldPosition { queued: false }.type_id(), 0x2b);
        assert_eq!(
            Command::Build {
                order: 0,
                x: 0,
                y: 0,
                unit_type: 0
            }
            .type_id(),
            0x0c
        );
        assert_eq!(
            Command::Unknown {
                type_id: 0xff,
                data: vec![]
            }
            .type_id(),
            0xff
        );
    }

    #[test]
    fn parse_commands_unit_test() {
        // Manually construct a simple command stream and verify parsing
        let data: Vec<u8> = vec![
            // Frame block: frame 10, 5 bytes of action data
            0x0a, 0x00, 0x00, 0x00, // frame 10
            0x05, // 5 bytes of action data
            0x00, // player 0
            0x09, // Select
            0x01, // count = 1
            0x42, 0x00, // unit tag 0x0042
        ];

        let commands = commands::parse_commands(&data, TextEncoding::Utf8).unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].frame, 10);
        assert_eq!(commands[0].player_id, 0);
        assert_eq!(
            commands[0].command,
            Command::Select {
                unit_tags: vec![0x0042]
            }
        );
    }

    #[test]
    fn parse_commands_multiple_actions_per_frame() {
        // Two commands in one frame block
        let data: Vec<u8> = vec![
            0x05, 0x00, 0x00, 0x00, // frame 5
            0x08, // 8 bytes of action data
            // First action: player 0, Stop (queued=false)
            0x00, 0x1a, 0x00, // Second action: player 1, KeepAlive
            0x01, 0x05, // Third action: player 0, HoldPosition (queued=true)
            0x00, 0x2b, 0x01,
        ];

        let commands = commands::parse_commands(&data, TextEncoding::Utf8).unwrap();
        assert_eq!(commands.len(), 3);

        assert_eq!(commands[0].frame, 5);
        assert_eq!(commands[0].player_id, 0);
        assert_eq!(commands[0].command, Command::Stop { queued: false });

        assert_eq!(commands[1].frame, 5);
        assert_eq!(commands[1].player_id, 1);
        assert_eq!(commands[1].command, Command::KeepAlive);

        assert_eq!(commands[2].frame, 5);
        assert_eq!(commands[2].player_id, 0);
        assert_eq!(commands[2].command, Command::HoldPosition { queued: true });
    }

    #[test]
    fn parse_commands_unknown_type() {
        // Unknown command type should consume rest of frame block
        let data: Vec<u8> = vec![
            0x01, 0x00, 0x00, 0x00, // frame 1
            0x06, // 6 bytes of action data
            0x00, // player 0
            0xfe, // unknown command type
            0xaa, 0xbb, 0xcc, 0xdd, // 4 bytes of data
        ];

        let commands = commands::parse_commands(&data, TextEncoding::Utf8).unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].frame, 1);
        assert_eq!(commands[0].player_id, 0);
        assert_eq!(
            commands[0].command,
            Command::Unknown {
                type_id: 0xfe,
                data: vec![0xaa, 0xbb, 0xcc, 0xdd]
            }
        );
    }

    #[test]
    fn limits_scr_121() {
        let cursor = Cursor::new(SCR_121);
        let mut replay = Replay::new(cursor).unwrap();
        let limits = replay.get_limits().unwrap();

        // SC:R replays should have non-default limits from the LMTS section
        assert!(
            limits.units >= 1700,
            "SC:R unit limit should be at least 1700, got {}",
            limits.units
        );
        assert!(
            limits.units <= 3400,
            "SC:R unit limit should be at most 3400, got {}",
            limits.units
        );
    }

    #[test]
    fn limits_legacy_returns_defaults() {
        let cursor = Cursor::new(LEGACY);
        let mut replay = Replay::new(cursor).unwrap();
        let limits = replay.get_limits().unwrap();

        // Legacy replays don't have LMTS, should return classic BW defaults
        assert_eq!(limits.units, 1700);
        assert_eq!(limits.images, 5000);
        assert_eq!(limits.sprites, 2500);
    }

    #[test]
    fn map_name_full_32_bytes() {
        // This replay's map name is 29 characters, which the old 26-byte read truncated to
        // "| iCCup | Fighting Spirit "
        let mut cursor = Cursor::new(SCR_MAPNAME32);
        let replay = Replay::new(&mut cursor).unwrap();
        assert_eq!(replay.map_name(), "| iCCup | Fighting Spirit 1.3");
    }

    #[test]
    fn legacy_encoding_utf8() {
        // A legacy replay whose strings contain valid multibyte UTF-8 (the colored map name
        // fills the entire 32-byte field, so it cuts off mid-"Legends")
        let mut cursor = Cursor::new(LEGACY_UTF8);
        let replay = Replay::new(&mut cursor).unwrap();
        assert_eq!(replay.game_title(), "asdasd");
        assert_eq!(replay.host_name(), "fakename");
        assert_eq!(
            replay.map_name(),
            "\u{3}T\u{6}ëäm \u{3}M\u{6}icro \u{3}A\u{6}rënä \u{2}Lege"
        );
    }

    #[test]
    fn legacy_encoding_cp1252() {
        // The same map as `legacy_encoding_utf8`, but saved by a host using the windows-1252
        // codepage. Note that this string is invalid as cp949 (0xEB 0x6E in "rën" is not a valid
        // pair), which is what allows the cp1252 fallback to kick in.
        let mut cursor = Cursor::new(LEGACY_CP1252);
        let replay = Replay::new(&mut cursor).unwrap();
        assert_eq!(replay.host_name(), "2Pacalypse-");
        assert_eq!(
            replay.map_name(),
            "\u{3}T\u{6}ëäm \u{3}M\u{6}icro \u{3}A\u{6}rënä \u{2}Legends"
        );
    }

    #[test]
    fn legacy_encoding_cp949() {
        // A legacy replay saved by a Korean-locale host: the map name is 투혼 (Fighting Spirit)
        // in cp949, which is invalid as UTF-8
        let mut cursor = Cursor::new(LEGACY_CP949);
        let replay = Replay::new(&mut cursor).unwrap();
        assert_eq!(replay.game_title(), "441");
        assert_eq!(replay.host_name(), "Question");
        assert_eq!(replay.map_name(), "\u{7}투혼 \u{5}1.3");
    }

    #[test]
    fn text_encoding_decode() {
        // ASCII is identical in all supported encodings
        assert_eq!(TextEncoding::Utf8.decode(b"asdf\0zzz"), "asdf");
        assert_eq!(TextEncoding::Legacy.decode(b"asdf\0zzz"), "asdf");
        // Valid multibyte UTF-8 decodes as UTF-8 even for legacy replays
        assert_eq!(TextEncoding::Legacy.decode("Tëäm".as_bytes()), "Tëäm");
        // 투혼 in cp949 (invalid as UTF-8)
        assert_eq!(
            TextEncoding::Legacy.decode(&[0xc5, 0xf5, 0xc8, 0xa5]),
            "투혼"
        );
        // "Tëäm ... rën" in cp1252 (invalid as UTF-8 and as cp949)
        assert_eq!(
            TextEncoding::Legacy.decode(&[0x54, 0xeb, 0xe4, 0x6d, 0x20, 0x72, 0xeb, 0x6e]),
            "Tëäm rën"
        );
        // Modern replays never guess: invalid UTF-8 is replaced, not reinterpreted (0xc8 0xa5
        // happens to be a valid UTF-8 sequence, so it survives)
        assert_eq!(
            TextEncoding::Utf8.decode(&[0xc5, 0xf5, 0xc8, 0xa5]),
            "\u{fffd}\u{fffd}ȥ"
        );
    }

    #[test]
    fn shieldbattery_game_id_rfc4122() {
        let mut cursor = Cursor::new(SB_GAME_ID);
        let mut replay = Replay::new(&mut cursor).unwrap();
        let data = replay.get_shieldbattery_section().unwrap().unwrap();
        // ShieldBattery's own parser reads this as 81e31e1f-8500-4803-a60c-88b40f3d5836 (a valid
        // UUIDv4; the old little-endian read produced the bytes exactly reversed)
        assert_eq!(data.game_id, 0x81e31e1f_8500_4803_a60c_88b40f3d5836);
    }

    #[test]
    fn scr_empty_commands_replay() {
        // An SC:R replay saved at the very start of a game (an autosave): it contains no
        // commands, and dynamically-sized sections with no data are omitted from the file
        // entirely. The section walk used to get misaligned by one block on these, losing every
        // modern section and panicking (via a huge allocation from garbage sizes) when reading
        // the player names section.
        let mut cursor = Cursor::new(SCR_EMPTY_COMMANDS);
        let mut replay = Replay::new(&mut cursor).unwrap();
        assert_eq!(replay.format, ReplayFormat::Modern121);

        // The commands section is present but empty
        assert_eq!(
            replay.get_raw_section(ReplaySection::Commands).unwrap(),
            Some(vec![])
        );
        assert_eq!(replay.get_commands().unwrap(), Some(vec![]));

        // All modern sections are located
        for section in [
            ReplaySection::Skins,
            ReplaySection::Limits,
            ReplaySection::Bfix,
            ReplaySection::CustomColors,
            ReplaySection::Gcfg,
        ] {
            assert!(
                replay.get_raw_section(section).unwrap().is_some(),
                "{section:?} should be present"
            );
        }
        let limits = replay.get_limits().unwrap();
        assert!((1700..=3400).contains(&limits.units));

        let sb = replay.get_shieldbattery_section().unwrap().unwrap();
        assert!(!sb.shieldbattery_version.is_empty());
        assert_ne!(sb.game_id, 0);

        // The player names section is present and readable
        let names = replay
            .get_raw_section(ReplaySection::PlayerNames)
            .unwrap()
            .unwrap();
        assert!(!names.is_empty());

        // Map data is unaffected
        assert!(
            replay
                .get_raw_section(ReplaySection::MapData)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn legacy_raw_command_chunks() {
        // This legacy replay's commands section is stored raw: its single chunk's stored size
        // (98) equals its declared uncompressed size, meaning it is not compressed. Feeding it to
        // the implode decoder (as older versions did unconditionally for legacy sections) fails
        // with "literal flag not zero or one".
        let mut cursor = Cursor::new(LEGACY_RAW_COMMANDS);
        let mut replay = Replay::new(&mut cursor).unwrap();
        assert_eq!(replay.format, ReplayFormat::Legacy);
        assert_eq!(replay.map_name(), "| iCCup | Andromeda 1.0");

        let commands = replay.get_commands().unwrap().unwrap();
        assert!(!commands.is_empty());
    }

    #[test]
    fn ums_inactive_slots_excluded_from_players() {
        // A UMS replay where the map defines 7 inactive slots named "Computer": these are not
        // actual game participants and shouldn't be returned from players()
        let mut cursor = Cursor::new(LEGACY_UMS_INACTIVE);
        let replay = Replay::new(&mut cursor).unwrap();

        let players = replay.players().collect::<Vec<_>>();
        assert_eq!(players.len(), 1);
        assert_eq!(players[0].name, "2Pacalypse-");
        assert_eq!(players[0].player_type, PlayerType::Human);
        assert_eq!(replay.observers().count(), 0);

        // The inactive slots are still available through slots()
        let inactive = replay
            .slots()
            .iter()
            .filter(|s| s.player_type == PlayerType::Inactive && !s.is_empty())
            .count();
        assert_eq!(inactive, 7);
    }
}
