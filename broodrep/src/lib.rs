use std::io::{Read, Seek, SeekFrom};

use byteorder::ReadBytesExt as _;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BroodrepError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error("malformed header: {0}")]
    MalformedHeader(&'static str),
}

pub struct Replay<R: Read + Seek> {
    inner: R,
    format: ReplayFormat,
}

impl<R: Read + Seek> Replay<R> {
    pub fn new(mut reader: R) -> Result<Self, BroodrepError> {
        let format = Self::detect_format(&mut reader)?;
        Ok(Replay {
            inner: reader,
            format,
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
        reader.read(&mut magic)?;
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
}
