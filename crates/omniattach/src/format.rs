use crate::Result;
use serde::{Deserialize, Serialize};
use std::{fs::File, io::Read, path::Path};

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
pub fn detect_prefix(bytes: &[u8], extension: &str) -> Detection {
    let hint = Format::from_extension(extension);
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
        None if hint != Format::Unknown => (hint, DetectionBasis::Extension),
        None => (Format::Unknown, DetectionBasis::Unknown),
    };
    let mut warnings = Vec::new();
    if signature == Some(Format::Zip) && format != Format::Zip {
        warnings
            .push("ZIP document family is extension-hinted; contents were not inspected".into());
    }
    if basis == DetectionBasis::Signature && hint != Format::Unknown && format != hint {
        warnings.push(format!(
            "signature {format:?} differs from extension hint {hint:?}"
        ));
    }
    Detection {
        format,
        basis,
        extension_hint: hint,
        warnings,
    }
}
