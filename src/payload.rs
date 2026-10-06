use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// One FQ1 transfer packet embedded in a QR symbol.
///
/// Wire format: `FQ1|base64(filename)|sha256|part|parts|part_bytes|base64(data)`.
#[derive(Debug, Clone)]
pub struct FilePacket {
    pub filename: String,
    pub sha256: String,
    pub part: usize,
    pub parts: usize,
    pub data: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum PacketError {
    #[error("not an FQ1 packet")]
    NotFq1,
    #[error("FQ1 packet has {0} fields; expected 7")]
    FieldCount(usize),
    #[error("invalid {field}: {source}")]
    InvalidNumber {
        field: &'static str,
        #[source]
        source: std::num::ParseIntError,
    },
    #[error("invalid base64 {field}: {source}")]
    InvalidBase64 {
        field: &'static str,
        #[source]
        source: base64::DecodeError,
    },
    #[error("filename is not valid UTF-8: {0}")]
    FilenameUtf8(#[from] std::string::FromUtf8Error),
    #[error("part number must be between 1 and the total part count")]
    InvalidPart,
    #[error("SHA-256 must be a 64-character hexadecimal string")]
    InvalidHash,
    #[error("declared chunk length is {declared}, but decoded {actual} bytes")]
    ChunkLength { declared: usize, actual: usize },
}

impl FilePacket {
    pub fn parse(text: &str) -> Result<Self, PacketError> {
        let fields: Vec<_> = text.split('|').collect();
        if fields.first() != Some(&"FQ1") {
            return Err(PacketError::NotFq1);
        }
        if fields.len() != 7 {
            return Err(PacketError::FieldCount(fields.len()));
        }

        let raw_filename =
            STANDARD
                .decode(fields[1])
                .map_err(|source| PacketError::InvalidBase64 {
                    field: "filename",
                    source,
                })?;
        let filename = String::from_utf8(raw_filename)?;
        let sha256 = fields[2].to_ascii_lowercase();
        if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(PacketError::InvalidHash);
        }
        let part = fields[3]
            .parse()
            .map_err(|source| PacketError::InvalidNumber {
                field: "part",
                source,
            })?;
        let parts = fields[4]
            .parse()
            .map_err(|source| PacketError::InvalidNumber {
                field: "parts",
                source,
            })?;
        let declared_len = fields[5]
            .parse()
            .map_err(|source| PacketError::InvalidNumber {
                field: "part_bytes",
                source,
            })?;
        let data = STANDARD
            .decode(fields[6])
            .map_err(|source| PacketError::InvalidBase64 {
                field: "data",
                source,
            })?;
        if part == 0 || parts == 0 || part > parts {
            return Err(PacketError::InvalidPart);
        }
        if data.len() != declared_len {
            return Err(PacketError::ChunkLength {
                declared: declared_len,
                actual: data.len(),
            });
        }

        Ok(Self {
            filename,
            sha256,
            part,
            parts,
            data,
        })
    }
}

#[derive(Debug)]
pub struct FileAssembly {
    pub filename: String,
    pub sha256: String,
    pub parts: usize,
    chunks: BTreeMap<usize, Vec<u8>>,
    pub recovered: Option<Vec<u8>>,
    pub verification_error: Option<String>,
    pub saved_path: Option<PathBuf>,
}

impl FileAssembly {
    pub fn new(packet: &FilePacket) -> Self {
        Self {
            filename: packet.filename.clone(),
            sha256: packet.sha256.clone(),
            parts: packet.parts,
            chunks: BTreeMap::new(),
            recovered: None,
            verification_error: None,
            saved_path: None,
        }
    }

    /// Adds a newly seen packet. Returns true only when it was not a duplicate.
    pub fn insert(&mut self, packet: FilePacket) -> bool {
        if packet.filename != self.filename
            || packet.sha256 != self.sha256
            || packet.parts != self.parts
        {
            self.verification_error =
                Some("Inconsistent FQ1 metadata for this transfer".to_owned());
            return false;
        }
        if self.chunks.insert(packet.part, packet.data).is_some() {
            return false;
        }
        self.try_recover();
        true
    }

    pub fn received_parts(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_complete(&self) -> bool {
        self.recovered.is_some()
    }

    /// Best-effort file type detection from recovered bytes, then the filename extension.
    pub fn file_type(&self) -> String {
        if let Some(data) = &self.recovered {
            if data.starts_with(b"7z\xBC\xAF\x27\x1C") {
                return "7-Zip archive".to_owned();
            }
            if data.starts_with(b"PK\x03\x04") || data.starts_with(b"PK\x05\x06") {
                return "ZIP archive".to_owned();
            }
            if data.starts_with(b"%PDF-") {
                return "PDF document".to_owned();
            }
            if data.starts_with(b"\x89PNG\r\n\x1A\n") {
                return "PNG image".to_owned();
            }
            if data.starts_with(b"\xFF\xD8\xFF") {
                return "JPEG image".to_owned();
            }
            if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
                return "GIF image".to_owned();
            }
            if data.get(4..8) == Some(b"ftyp") {
                return "MP4 media".to_owned();
            }
            if data.starts_with(b"MZ") {
                return "Windows executable".to_owned();
            }
        }
        let extension = Path::new(&self.filename)
            .extension()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .map(str::to_ascii_uppercase);
        extension.map_or_else(
            || "Unknown file".to_owned(),
            |value| format!("{value} file"),
        )
    }

    fn try_recover(&mut self) {
        if self.chunks.len() != self.parts || self.recovered.is_some() {
            return;
        }
        let total_len = self.chunks.values().map(Vec::len).sum();
        let mut output = Vec::with_capacity(total_len);
        for part in 1..=self.parts {
            let Some(chunk) = self.chunks.get(&part) else {
                return;
            };
            output.extend_from_slice(chunk);
        }
        let actual = format!("{:x}", Sha256::digest(&output));
        if actual == self.sha256 {
            self.recovered = Some(output);
            self.verification_error = None;
        } else {
            self.verification_error = Some(format!(
                "SHA-256 mismatch after assembly (expected {}, got {actual})",
                self.sha256
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(part: usize, data: &[u8]) -> String {
        let hash = format!("{:x}", Sha256::digest(b"hello world"));
        format!(
            "FQ1|ZmlsZS5iaW4=|{hash}|{part}|2|{}|{}",
            data.len(),
            STANDARD.encode(data)
        )
    }

    #[test]
    fn reassembles_and_verifies_packets() {
        let first = FilePacket::parse(&packet(1, b"hello ")).unwrap();
        let second = FilePacket::parse(&packet(2, b"world")).unwrap();
        let mut assembly = FileAssembly::new(&first);
        assert!(assembly.insert(first));
        assert!(assembly.insert(second));
        assert_eq!(assembly.recovered.as_deref(), Some(&b"hello world"[..]));
    }
}
