use crate::Result;
use serde::{Deserialize, Serialize};
use std::{fs::File, io::Read, path::Path};

const OLE_CFBF_MAGIC: &[u8; 8] = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1";

/// CFBF stream names that identify the legacy Office family. Names are UTF-16LE in directory entries.
const CFBF_STREAMS: [(&str, Format); 3] = [
    ("WordDocument", Format::Doc),
    ("PowerPoint Document", Format::Ppt),
    ("Workbook", Format::Xls),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Text,
    Markdown,
    Html,
    Xml,
    Csv,
    Tsv,
    Json,
    JsonLines,
    Pdf,
    Docx,
    Pptx,
    Xlsx,
    Odt,
    Odp,
    Ods,
    Epub,
    Mobi,
    Azw3,
    Doc,
    Ppt,
    Xls,
    Rtf,
    Audio,
    Video,
    Image,
    Zip,
    Unknown,
}
impl Format {
    pub fn from_extension(extension: &str) -> Self {
        match extension
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str()
        {
            "txt" | "log" | "rst" | "rs" | "py" | "c" | "h" | "cpp" | "sh" | "toml" | "yaml"
            | "yml" => Self::Text,
            "md" | "markdown" | "mdown" => Self::Markdown,
            "html" | "htm" => Self::Html,
            "xml" => Self::Xml,
            "csv" => Self::Csv,
            "tsv" => Self::Tsv,
            "json" => Self::Json,
            "jsonl" | "ndjson" => Self::JsonLines,
            "pdf" => Self::Pdf,
            "docx" => Self::Docx,
            "pptx" => Self::Pptx,
            "xlsx" => Self::Xlsx,
            "odt" => Self::Odt,
            "odp" => Self::Odp,
            "ods" => Self::Ods,
            "epub" => Self::Epub,
            "mobi" => Self::Mobi,
            "azw3" => Self::Azw3,
            "doc" => Self::Doc,
            "ppt" => Self::Ppt,
            "xls" => Self::Xls,
            "rtf" => Self::Rtf,
            "wav" | "mp3" | "flac" | "ogg" | "opus" | "m4a" | "aac" | "aiff" => Self::Audio,
            "mp4" | "mkv" | "webm" | "mov" | "avi" | "mpeg" | "mpg" | "m4v" => Self::Video,
            "png" | "jpg" | "jpeg" | "tif" | "tiff" | "bmp" | "webp" | "pnm" => Self::Image,
            "zip" => Self::Zip,
            _ => Self::Unknown,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionBasis {
    Signature,
    Extension,
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Detection {
    pub format: Format,
    pub basis: DetectionBasis,
    pub extension_hint: Format,
    pub warnings: Vec<String>,
}
/// Conservative prefix/extension routing, not a replacement for the document crate detector.
pub fn detect(path: impl AsRef<Path>) -> Result<Detection> {
    let path = path.as_ref();
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(crate::Error::InvalidInput("not a regular file".into()));
    }
    let mut prefix = Vec::new();
    file.take(8192).read_to_end(&mut prefix)?;
    Ok(detect_prefix(
        &prefix,
        path.extension().and_then(|v| v.to_str()).unwrap_or(""),
    ))
}

/// Legacy family named by a CFBF stream marker in the prefix; `None` if absent or ambiguous.
fn cfbf_stream_format(bytes: &[u8]) -> Option<Format> {
    let found: Vec<Format> = CFBF_STREAMS
        .iter()
        .filter(|(name, _)| {
            let needle: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
            bytes.windows(needle.len()).any(|w| w == needle.as_slice())
        })
        .map(|(_, format)| *format)
        .collect();
    match found.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

pub fn detect_prefix(bytes: &[u8], extension: &str) -> Detection {
    let hint = Format::from_extension(extension);
    let ole = bytes.starts_with(OLE_CFBF_MAGIC);
    let signature = if bytes.starts_with(b"%PDF-") {
        Some(Format::Pdf)
    } else if bytes.starts_with(b"{\\rtf") {
        Some(Format::Rtf)
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(b"\xff\xd8\xff")
        || bytes.starts_with(b"II*\0")
        || bytes.starts_with(b"MM\0*")
    {
        Some(Format::Image)
    } else if bytes.starts_with(b"fLaC")
        || bytes.starts_with(b"ID3")
        || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE"))
    {
        Some(Format::Audio)
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"AVI ") {
        Some(Format::Video)
    } else if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") {
        Some(Format::Zip)
    } else {
        None
    };
    let legacy_stream = if ole { cfbf_stream_format(bytes) } else { None };
    let (format, basis) = match signature {
        Some(Format::Zip)
            if matches!(
                hint,
                Format::Docx
                    | Format::Pptx
                    | Format::Xlsx
                    | Format::Odt
                    | Format::Odp
                    | Format::Ods
                    | Format::Epub
            ) =>
        {
            (hint, DetectionBasis::Extension)
        }
        Some(format) => (format, DetectionBasis::Signature),
        None if ole && legacy_stream.is_some() => (
            legacy_stream.unwrap_or(Format::Unknown),
            DetectionBasis::Signature,
        ),
        None if ole && matches!(hint, Format::Doc | Format::Ppt | Format::Xls) => {
            (hint, DetectionBasis::Extension)
        }
        None if ole => (Format::Unknown, DetectionBasis::Unknown),
        None if hint != Format::Unknown => (hint, DetectionBasis::Extension),
        None => (Format::Unknown, DetectionBasis::Unknown),
    };
    let mut warnings = Vec::new();
    if signature == Some(Format::Zip) && format != Format::Zip {
        warnings
            .push("ZIP document family is extension-hinted; contents were not inspected".into());
    }
    if ole
        && basis == DetectionBasis::Extension
        && matches!(format, Format::Doc | Format::Ppt | Format::Xls)
    {
        warnings.push(
            "OLE CFBF document family is extension-hinted; no stream marker found in the inspected prefix".into(),
        );
    }
    if ole && format == Format::Unknown && hint != Format::Unknown {
        warnings.push(format!(
            "OLE CFBF signature conflicts with non-OLE extension hint {hint:?}"
        ));
    }
    if basis == DetectionBasis::Signature && hint != Format::Unknown && format != hint {
        warnings.push(format!(
            "signature {format:?} differs from extension hint {hint:?}"
        ));
    }
    if matches!(format, Format::Mobi | Format::Azw3) {
        warnings.push("MOBI/AZW3 is extension-hinted; ebook records are validated during conversion".into());
    }
    Detection {
        format,
        basis,
        extension_hint: hint,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ole_cfbf_magic_detects_legacy_office_and_rejects_mislabeled_extensions() {
        let ole = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1rest";
        for (ext, expected) in [
            ("doc", Format::Doc),
            ("ppt", Format::Ppt),
            ("xls", Format::Xls),
        ] {
            let d = detect_prefix(ole, ext);
            assert_eq!(d.format, expected);
            assert_eq!(d.basis, DetectionBasis::Extension);
            assert!(d.warnings.iter().any(|w| w.contains("OLE CFBF")));
        }
        let mislabeled = detect_prefix(ole, "txt");
        assert_eq!(mislabeled.format, Format::Unknown);
        assert_eq!(mislabeled.basis, DetectionBasis::Unknown);
        assert!(mislabeled.warnings.iter().any(|w| w.contains("OLE CFBF signature conflicts")));
    }

    #[test]
    fn cfbf_stream_names_override_a_wrong_extension() {
        let mut bytes = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1".to_vec();
        bytes.extend("PowerPoint Document".encode_utf16().flat_map(u16::to_le_bytes));
        let d = detect_prefix(&bytes, "doc");
        assert_eq!(d.format, Format::Ppt);
        assert_eq!(d.basis, DetectionBasis::Signature);
        assert!(d.warnings.iter().any(|w| w.contains("differs from extension hint")));
    }

    #[test]
    fn ebook_extensions_are_hints_not_content_validation() {
        for (ext, expected) in [("MOBI", Format::Mobi), ("AZW3", Format::Azw3)] {
            let d = detect_prefix(b"not a book", ext);
            assert_eq!(d.format, expected);
            assert_eq!(d.basis, DetectionBasis::Extension);
            assert!(!d.warnings.is_empty());
        }
        assert_eq!(detect_prefix(b"%PDF-1.7", "azw3").format, Format::Pdf);
    }
}
