use std::{
    io::{Read, Take},
    time::{Duration, Instant},
};

use thiserror::Error;

#[derive(Debug, Copy, Clone)]
pub struct DecompressionConfig {
    /// Maximum bytes to decompress (default: 100MB)
    pub max_decompressed_size: u64,
    /// Maximum compression ratio allowed (default: 500:1)
    pub max_compression_ratio: f64,
    /// Maximum time to spend decompressing (default: 30 seconds)
    pub max_decompression_time: Option<Duration>,
}

impl Default for DecompressionConfig {
    fn default() -> Self {
        Self {
            max_decompressed_size: 100 * 1024 * 1024, // 100MB
            max_compression_ratio: 500.0,
            max_decompression_time: Some(Duration::from_secs(30)),
        }
    }
}

#[derive(Debug, Error)]
pub enum DecompressionError {
    #[error("Decompressed size limit exceeded")]
    SizeLimitExceeded,
    #[error("Compression ratio too high")]
    CompressionRatioExceeded,
    #[error("Decompression timeout")]
    TimeoutExceeded,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// A wrapper around decompression implementations that implement [Read], providing various
/// mechanisms for limiting decompression to avoid things like zip bombs. For best results, a config
/// should be used that takes into account the characteristics of the data being decompressed.
pub struct SafeDecompressor<R: Read> {
    inner: Take<R>,
    max_decompressed_size: u64,
    max_ratio: f64,
    max_time: Option<Duration>,
    input_size: Option<u64>,

    start_time: Option<Instant>,
    bytes_read: u64,
}

impl<R: Read> SafeDecompressor<R> {
    /// Constructs a new SafeDecompressor wrapping the given [Read] implementation. `input_size` is
    /// the size of the compressed input data in bytes, if known. If not specified, compression
    /// ratio limits will not apply.
    pub fn new(reader: R, config: DecompressionConfig, input_size: Option<u64>) -> Self {
        Self {
            inner: reader.take(config.max_decompressed_size),
            max_decompressed_size: config.max_decompressed_size,
            max_ratio: config.max_compression_ratio,
            max_time: config.max_decompression_time,
            input_size,

            start_time: None,
            bytes_read: 0,
        }
    }
}

impl<R: Read> Read for SafeDecompressor<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if let Some(max_time) = self.max_time {
            if self.start_time.is_none() {
                self.start_time = Some(Instant::now());
            }
            if self.start_time.map(|t| t.elapsed()).unwrap_or_default() > max_time {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    DecompressionError::TimeoutExceeded,
                ));
            }
        }

        let bytes_read = self.inner.read(buf)?;
        self.bytes_read = self.bytes_read.saturating_add(bytes_read as u64);

        if bytes_read == 0 && self.bytes_read == self.max_decompressed_size {
            // EOF and we've reached the max the Take will allow, try to read 1 more byte to see if
            // there was more data
            self.inner.set_limit(1);
            let mut buf = [0; 1];
            if let Ok(1) = self.inner.read(&mut buf) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    DecompressionError::SizeLimitExceeded,
                ));
            }
        }

        if let Some(input_size) = self.input_size {
            let ratio = self.bytes_read as f64 / input_size as f64;
            if ratio > self.max_ratio {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    DecompressionError::CompressionRatioExceeded,
                ));
            }
        }

        Ok(bytes_read)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use explode::ExplodeReader;
    use flate2::bufread::ZlibDecoder;

    use super::*;

    const IMPLODE_BOMB: &[u8] = include_bytes!("../testdata/all_zeroes_1MB.impode");

    #[test]
    fn implode_bomb_size() {
        let config = DecompressionConfig {
            max_decompressed_size: 1000 * 1024, // slightly less than 1MB
            max_compression_ratio: f64::MAX,
            ..Default::default()
        };
        let mut safe_reader = SafeDecompressor::new(
            ExplodeReader::new(IMPLODE_BOMB),
            config,
            Some(IMPLODE_BOMB.len() as u64),
        );
        let mut out = Vec::new();
        let result = safe_reader.read_to_end(&mut out);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        let err = err.downcast::<DecompressionError>().unwrap();
        assert!(matches!(err, DecompressionError::SizeLimitExceeded));
    }

    #[test]
    fn implode_bomb_ratio() {
        let config = DecompressionConfig {
            max_compression_ratio: 100.0,
            ..Default::default()
        };
        let mut safe_reader = SafeDecompressor::new(
            ExplodeReader::new(IMPLODE_BOMB),
            config,
            Some(IMPLODE_BOMB.len() as u64),
        );
        let mut out = Vec::new();
        let result = safe_reader.read_to_end(&mut out);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        let err = err.downcast::<DecompressionError>().unwrap();
        assert!(matches!(err, DecompressionError::CompressionRatioExceeded));
    }

    fn create_zlib_bomb() -> Vec<u8> {
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        let data = vec![0u8; 1024 * 1024];
        encoder.write_all(&data).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn zlib_bomb_size() {
        let config = DecompressionConfig {
            max_decompressed_size: 1000 * 1024, // slightly less than 1MB
            max_compression_ratio: f64::MAX,
            ..Default::default()
        };
        let data = create_zlib_bomb();
        let mut safe_reader =
            SafeDecompressor::new(ZlibDecoder::new(&data[..]), config, Some(data.len() as u64));
        let mut out = Vec::new();
        let result = safe_reader.read_to_end(&mut out);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        let err = err.downcast::<DecompressionError>().unwrap();
        assert!(matches!(err, DecompressionError::SizeLimitExceeded));
    }

    #[test]
    fn zlib_bomb_ratio() {
        let config = DecompressionConfig {
            max_compression_ratio: 1000.0,
            ..Default::default()
        };
        let data = create_zlib_bomb();
        let mut safe_reader =
            SafeDecompressor::new(ZlibDecoder::new(&data[..]), config, Some(data.len() as u64));
        let mut out = Vec::new();
        let result = safe_reader.read_to_end(&mut out);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        let err = err.downcast::<DecompressionError>().unwrap();
        assert!(matches!(err, DecompressionError::CompressionRatioExceeded));
    }
}
