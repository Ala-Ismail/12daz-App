//! Bounded streaming input. ZIP members stay inside the reader.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::LazyLock;
use std::sync::atomic::AtomicBool;

use encoding_rs::{Decoder, DecoderResult, Encoding, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252};
use regex::Regex;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::error::{ConversionError, Result, checkpoint};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub expanded_bytes: u64,
    pub members: usize,
    pub token_length: usize,
    pub depth: usize,
    pub entities: usize,
    pub entity_tokens: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            expanded_bytes: 1024 * 1024 * 1024,
            members: 100,
            token_length: 1024 * 1024,
            depth: 64,
            entities: 1_000_000,
            entity_tokens: 2_000_000,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Metadata {
    pub encoding: String,
    pub archive_version: Option<String>,
    pub extra_members: usize,
}

fn input_error(error: impl std::fmt::Display) -> ConversionError {
    ConversionError::message(format!("Cannot read input: {error}"))
}

fn byte_limit() -> ConversionError {
    ConversionError::message("Expanded input exceeds configured byte limit.")
}

pub fn input_hash(path: &Path, cancel: Option<&AtomicBool>) -> Result<String> {
    checkpoint(cancel)?;
    let mut file = File::open(path).map_err(input_error)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        checkpoint(cancel)?;
        let size = file.read(&mut buffer).map_err(input_error)?;
        if size == 0 {
            return Ok(format!("{:x}", digest.finalize()));
        }
        digest.update(&buffer[..size]);
    }
}

enum StrictDecoder {
    Encoding(Box<Decoder>),
    Cp1252(Box<Decoder>),
    Latin1,
    Ascii,
}

impl StrictDecoder {
    fn select(prefix: &[u8], fallback: Option<&str>) -> Result<(Self, String, usize)> {
        let (codec, name, skip) = match prefix {
            [0xff, 0xfe, ..] => (UTF_16LE, "utf-16", 2),
            [0xfe, 0xff, ..] => (UTF_16BE, "utf-16", 2),
            [0xef, 0xbb, 0xbf, ..] => (UTF_8, "utf-8-sig", 3),
            _ => {
                let name = fallback.unwrap_or("utf-8");
                let normalized = name.trim().to_ascii_lowercase().replace('_', "-");
                match normalized.as_str() {
                    "ascii" | "us-ascii" | "646" => return Ok((Self::Ascii, name.into(), 0)),
                    "latin1" | "latin-1" | "iso8859-1" | "iso-8859-1" => {
                        return Ok((Self::Latin1, name.into(), 0));
                    }
                    "utf16" | "utf-16" if !prefix.is_empty() => {
                        return Err(ConversionError::message(format!(
                            "Cannot decode input as {name}: UTF-16 stream does not start with a BOM"
                        )));
                    }
                    "utf16" | "utf-16" | "utf-16-le" | "utf16-le" => (UTF_16LE, name, 0),
                    "utf-16-be" | "utf16-be" => (UTF_16BE, name, 0),
                    "utf8" | "utf-8-sig" => (UTF_8, name, 0),
                    _ => (
                        Encoding::for_label_no_replacement(normalized.as_bytes()).ok_or_else(
                            || ConversionError::message(format!("Unknown encoding: {name}")),
                        )?,
                        name,
                        0,
                    ),
                }
            }
        };
        let decoder = Box::new(codec.new_decoder_without_bom_handling());
        let decoder = if codec == WINDOWS_1252 {
            Self::Cp1252(decoder)
        } else {
            Self::Encoding(decoder)
        };
        Ok((decoder, name.into(), skip))
    }

    fn decode(&mut self, bytes: &[u8], last: bool, name: &str) -> Result<String> {
        let invalid = || {
            ConversionError::message(format!(
                "Cannot decode input as {name}: invalid byte sequence"
            ))
        };
        match self {
            Self::Latin1 => return Ok(bytes.iter().copied().map(char::from).collect()),
            Self::Ascii => {
                if !bytes.is_ascii() {
                    return Err(invalid());
                }
                return Ok(bytes.iter().copied().map(char::from).collect());
            }
            Self::Cp1252(_)
                if bytes
                    .iter()
                    .any(|byte| matches!(byte, 0x81 | 0x8d | 0x8f | 0x90 | 0x9d)) =>
            {
                return Err(invalid());
            }
            _ => {}
        }
        let (Self::Encoding(decoder) | Self::Cp1252(decoder)) = self else {
            unreachable!()
        };
        let capacity = decoder
            .max_utf8_buffer_length_without_replacement(bytes.len())
            .ok_or_else(byte_limit)?;
        let mut result = String::with_capacity(capacity);
        let (status, _) = decoder.decode_to_string_without_replacement(bytes, &mut result, last);
        match status {
            DecoderResult::InputEmpty => Ok(result),
            DecoderResult::Malformed(_, _) => Err(invalid()),
            DecoderResult::OutputFull => Err(byte_limit()),
        }
    }
}

struct BoundedReader<'a> {
    reader: &'a mut dyn Read,
    cancel: Option<&'a AtomicBool>,
    progress: &'a mut dyn FnMut(&str, u64),
    limit: u64,
    bytes_read: u64,
}

impl BoundedReader<'_> {
    fn read_chunk(&mut self, buffer: &mut [u8]) -> Result<usize> {
        checkpoint(self.cancel)?;
        let mut size = 0;
        while size < buffer.len() {
            match self.reader.read(&mut buffer[size..]) {
                Ok(0) => break,
                Ok(read) => size += read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(input_error(error)),
            }
        }
        self.bytes_read += size as u64;
        if self.bytes_read > self.limit {
            return Err(byte_limit());
        }
        (self.progress)("Reading archive", self.bytes_read);
        Ok(size)
    }
}

pub struct DecodedInput<'a> {
    bounded: BoundedReader<'a>,
    decoder: StrictDecoder,
    prefix: Option<String>,
    buffer: Vec<u8>,
    finished: bool,
    pub metadata: Metadata,
}

static VERSION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)//\s*archive_version\s+"([^"]+)""#)
        .expect("the archive-version pattern is valid")
});

impl<'a> DecodedInput<'a> {
    fn new(
        mut bounded: BoundedReader<'a>,
        encoding: Option<&str>,
        extra_members: usize,
    ) -> Result<Self> {
        let mut buffer = vec![0; 65536];
        let size = bounded.read_chunk(&mut buffer[..8192])?;
        let (mut decoder, encoding, skip) = StrictDecoder::select(&buffer[..size], encoding)?;
        let prefix = decoder.decode(&buffer[skip..size], false, &encoding)?;
        let archive_version = VERSION.captures(&prefix).map(|found| found[1].to_owned());
        Ok(Self {
            bounded,
            decoder,
            buffer,
            prefix: Some(prefix),
            finished: false,
            metadata: Metadata {
                encoding,
                archive_version,
                extra_members,
            },
        })
    }
}

impl Iterator for DecodedInput<'_> {
    type Item = Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(prefix) = self.prefix.take() {
            return Some(Ok(prefix));
        }
        if self.finished {
            return None;
        }
        let result = self.bounded.read_chunk(&mut self.buffer).and_then(|size| {
            self.finished = size == 0;
            self.decoder
                .decode(&self.buffer[..size], self.finished, &self.metadata.encoding)
        });
        if result.is_err() {
            self.finished = true;
        }
        Some(result)
    }
}

pub fn open_input<T>(
    path: &Path,
    limits: Limits,
    encoding: Option<&str>,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(&str, u64),
    read: impl FnOnce(DecodedInput<'_>) -> Result<T>,
) -> Result<T> {
    checkpoint(cancel)?;
    let mut raw = File::open(path).map_err(input_error)?;
    let mut signature = Vec::with_capacity(4);
    (&mut raw)
        .take(4)
        .read_to_end(&mut signature)
        .map_err(input_error)?;
    raw.seek(SeekFrom::Start(0)).map_err(input_error)?;
    let limit = limits.expanded_bytes;
    if signature.starts_with(b"PK") {
        let mut archive = ZipArchive::new(raw).map_err(input_error)?;
        if archive.len() > limits.members {
            return Err(ConversionError::message("Archive member limit exceeded."));
        }
        let mut candidates = Vec::new();
        for index in 0..archive.len() {
            checkpoint(cancel)?;
            let entry = archive.by_index_raw(index).map_err(input_error)?;
            if entry.encrypted() {
                return Err(ConversionError::message(
                    "Encrypted archives are not supported.",
                ));
            }
            if !entry.is_dir() && entry.name().to_ascii_lowercase().ends_with(".12da") {
                candidates.push(index);
            }
        }
        let [index] = candidates.as_slice() else {
            return Err(ConversionError::message(
                "Archive must contain exactly one .12da payload.",
            ));
        };
        let extra_members = archive.len() - 1;
        let mut member = archive.by_index(*index).map_err(input_error)?;
        if member.size() > limit {
            return Err(byte_limit());
        }
        let bounded = BoundedReader {
            reader: &mut member,
            cancel,
            progress,
            limit,
            bytes_read: 0,
        };
        return read(DecodedInput::new(bounded, encoding, extra_members)?);
    }
    if [b"\x1f\x8b".as_slice(), b"7z", b"%PDF", b"Rar!"]
        .iter()
        .any(|prefix| signature.starts_with(prefix))
    {
        return Err(ConversionError::message(
            "Unsupported container; expected ZIP or plain 12da text.",
        ));
    }
    let bounded = BoundedReader {
        reader: &mut raw,
        cancel,
        progress,
        limit,
        bytes_read: 0,
    };
    read(DecodedInput::new(bounded, encoding, 0)?)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::sync::atomic::Ordering;

    use super::*;

    fn read(path: &Path, limits: Limits, encoding: Option<&str>) -> Result<(Metadata, String)> {
        open_input(path, limits, encoding, None, &mut |_, _| {}, |mut input| {
            let text = input.by_ref().collect::<Result<String>>()?;
            Ok((input.metadata, text))
        })
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])], method: zip::CompressionMethod) {
        let mut writer = zip::ZipWriter::new(File::create(path).unwrap());
        for &(name, payload) in entries {
            writer
                .start_file(
                    name,
                    zip::write::SimpleFileOptions::default().compression_method(method),
                )
                .unwrap();
            writer.write_all(payload).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn rust_04_plain_and_zip_stream_the_same_payload() {
        let folder = tempfile::tempdir().unwrap();
        let plain = folder.path().join("測量 field.12da");
        let archive = folder.path().join("archive.dat");
        let content = "// Archive_Version \"15.01.08.60\"\nmodel \"Café 測量\"";
        fs::write(&plain, content).unwrap();
        write_zip(
            &archive,
            &[
                ("../field.12DA", content.as_bytes()),
                ("../notes.txt", b"ignored"),
            ],
            zip::CompressionMethod::Deflated,
        );
        let (plain_meta, plain_text) = read(&plain, Limits::default(), None).unwrap();
        let (zip_meta, zip_text) = read(&archive, Limits::default(), None).unwrap();
        assert_eq!(plain_text, content);
        assert_eq!(zip_text, content);
        assert_eq!(zip_meta.archive_version.as_deref(), Some("15.01.08.60"));
        assert_eq!(plain_meta.archive_version, zip_meta.archive_version);
        assert_eq!(zip_meta.extra_members, 1);
        assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 2);
    }

    #[test]
    fn rust_04_strict_incremental_encodings_and_bom_precedence() {
        let folder = tempfile::tempdir().unwrap();
        let plain = folder.path().join("input.12da");
        let content = format!("{}測量", " ".repeat(8191));
        fs::write(&plain, &content).unwrap();
        assert_eq!(read(&plain, Limits::default(), None).unwrap().1, content);
        for little_endian in [true, false] {
            let mut bytes = if little_endian {
                vec![0xff, 0xfe]
            } else {
                vec![0xfe, 0xff]
            };
            for word in "Café 測量".encode_utf16() {
                bytes.extend(if little_endian {
                    word.to_le_bytes()
                } else {
                    word.to_be_bytes()
                });
            }
            fs::write(&plain, bytes).unwrap();
            let (metadata, text) = read(&plain, Limits::default(), Some("cp1252")).unwrap();
            assert_eq!(metadata.encoding, "utf-16");
            assert_eq!(text, "Café 測量");
        }
        fs::write(&plain, b"\xef\xbb\xbfCaf\xc3\xa9").unwrap();
        assert_eq!(
            read(&plain, Limits::default(), None).unwrap().0.encoding,
            "utf-8-sig"
        );
        fs::write(&plain, b"Caf\xe9").unwrap();
        assert!(read(&plain, Limits::default(), None).is_err());
        assert_eq!(
            read(&plain, Limits::default(), Some("cp1252")).unwrap().1,
            "Café"
        );
        fs::write(&plain, [0x81]).unwrap();
        assert!(read(&plain, Limits::default(), Some("cp1252")).is_err());
        assert!(read(&plain, Limits::default(), Some("unknown-encoding")).is_err());
    }

    #[test]
    fn rust_04_rejects_invalid_archives_and_limits() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("archive.12daz");
        for bytes in [b"%PDF broken".as_slice(), b"PK\x03\x04truncated"] {
            fs::write(&path, bytes).unwrap();
            assert!(read(&path, Limits::default(), None).is_err());
        }
        for entries in [
            vec![("notes.txt", b"unrelated".as_slice())],
            vec![("a.12da", b"first"), ("B.12DA", b"second")],
        ] {
            write_zip(&path, &entries, zip::CompressionMethod::Stored);
            assert!(
                read(&path, Limits::default(), None)
                    .unwrap_err()
                    .to_string()
                    .contains("exactly one")
            );
        }
        let bytes = b"unique archive payload";
        write_zip(&path, &[("a.12da", bytes)], zip::CompressionMethod::Stored);
        let limits = Limits {
            expanded_bytes: bytes.len() as u64,
            members: 1,
            ..Limits::default()
        };
        assert!(read(&path, limits, None).is_ok());
        assert!(
            read(
                &path,
                Limits {
                    expanded_bytes: limits.expanded_bytes - 1,
                    ..limits
                },
                None
            )
            .is_err()
        );
        assert!(
            read(
                &path,
                Limits {
                    members: 0,
                    ..limits
                },
                None
            )
            .is_err()
        );
        let mut zip_bytes = fs::read(&path).unwrap();
        let location = zip_bytes
            .windows(bytes.len())
            .position(|window| window == bytes)
            .unwrap();
        zip_bytes[location] ^= 1;
        fs::write(&path, zip_bytes).unwrap();
        assert!(read(&path, Limits::default(), None).is_err());
        fs::write(&path, bytes).unwrap();
        assert!(read(&path, limits, None).is_ok());
        assert!(
            read(
                &path,
                Limits {
                    expanded_bytes: limits.expanded_bytes - 1,
                    ..limits
                },
                None
            )
            .is_err()
        );
    }

    #[test]
    fn rust_04_progress_and_cancellation_with_stable_hash() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("input.12da");
        fs::write(&path, b"abc").unwrap();
        assert_eq!(
            input_hash(&path, None).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let cancel = AtomicBool::new(true);
        assert_eq!(
            input_hash(&path, Some(&cancel)),
            Err(ConversionError::Cancelled)
        );
        cancel.store(false, Ordering::Relaxed);
        fs::write(&path, vec![b' '; 160000]).unwrap();
        let mut observed = Vec::new();
        let result = open_input(
            &path,
            Limits::default(),
            None,
            Some(&cancel),
            &mut |stage, bytes| {
                assert_eq!(stage, "Reading archive");
                observed.push(bytes);
                if bytes > 8192 {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
            |input| input.collect::<Result<String>>(),
        );
        assert_eq!(result, Err(ConversionError::Cancelled));
        assert_eq!(observed, [8192, 8192 + 65536]);
    }
}
