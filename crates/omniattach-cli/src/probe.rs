use crate::{CliError, Result, Runtime};
use serde_json::{Value, json};
use std::{fs::File, io::Read, path::Path};
/// Conservative signature/extension inspection, usable without conversion support.
pub fn probe(path: &Path, limit: u64, runtime: &Runtime) -> Result<Value> {
    runtime.check()?;
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() { return Err(CliError::input("input is not a regular file")); }
    if metadata.len() > limit { return Err(CliError::limit("input bytes")); }
    let mut prefix = Vec::new();
    File::open(path)?.take(8192).read_to_end(&mut prefix)?;
    runtime.check()?;
    let hint = extension_format(path.extension().and_then(|s| s.to_str()).unwrap_or(""));
    let signature = if prefix.starts_with(b"%PDF-") { Some("pdf") }
    else if prefix.starts_with(b"{\\rtf") { Some("rtf") }
    else if prefix.starts_with(b"\x89PNG\r\n\x1a\n") || prefix.starts_with(b"\xff\xd8\xff") || prefix.starts_with(b"II*\0") || prefix.starts_with(b"MM\0*") { Some("image") }
    else if prefix.starts_with(b"fLaC") || prefix.starts_with(b"ID3") || (prefix.starts_with(b"RIFF") && prefix.get(8..12) == Some(b"WAVE")) { Some("audio") }
    else if prefix.starts_with(b"RIFF") && prefix.get(8..12) == Some(b"AVI ") { Some("video") }
    else if prefix.starts_with(b"PK\x03\x04") || prefix.starts_with(b"PK\x05\x06") { Some("zip") }
    else { None };
    let office = matches!(hint, "docx" | "pptx" | "xlsx" | "odt" | "odp" | "ods" | "epub");
    let (format, basis) = match signature {
        Some("zip") if office => (hint, "extension"),
        Some(format) => (format, "signature"),
        None if hint != "unknown" => (hint, "extension"),
        _ => ("unknown", "unknown"),
    };
    let mut warnings = Vec::new();
    if signature == Some("zip") && office { warnings.push("ZIP family is extension-hinted; package contents not inspected".to_owned()); }
    if basis == "signature" && hint != "unknown" && format != hint { warnings.push(format!("signature {format} differs from extension hint {hint}")); }
    if matches!(format, "mobi" | "azw3") { warnings.push("MOBI/AZW3 is extension-hinted; ebook records are validated during conversion".into()); }
    Ok(json!({"schema":"omniattach-cli/probe/0.1","source":path,"bytes":metadata.len(),"detection":{"format":format,"basis":basis,"extension_hint":hint,"warnings":warnings},"conversion_support_checked":false}))
}
pub fn extension_format(extension: &str) -> &'static str {
    match extension.trim_start_matches('.').to_ascii_lowercase().as_str() {
        "txt" | "log" | "rst" | "rs" | "py" | "c" | "h" | "cpp" | "sh" | "toml" | "yaml" | "yml" => "text",
        "md" | "markdown" | "mdown" => "markdown",
        "html" | "htm" => "html",
        "xml" => "xml",
        "csv" => "csv",
        "tsv" => "tsv",
        "json" => "json",
        "jsonl" | "ndjson" => "json_lines",
        "pdf" => "pdf",
        "docx" => "docx",
        "pptx" => "pptx",
        "xlsx" => "xlsx",
        "odt" => "odt",
        "odp" => "odp",
        "ods" => "ods",
        "epub" => "epub",
        "mobi" => "mobi",
        "azw3" => "azw3",
        "doc" => "doc",
        "ppt" => "ppt",
        "xls" => "xls",
        "rtf" => "rtf",
        "wav" | "mp3" | "flac" | "ogg" | "opus" | "m4a" | "aac" | "aiff" => "audio",
        "mp4" | "mkv" | "webm" | "mov" | "avi" | "mpeg" | "mpg" | "m4v" => "video",
        "png" | "jpg" | "jpeg" | "tif" | "tiff" | "bmp" | "webp" | "pnm" => "image",
        "zip" => "zip",
        _ => "unknown",
    }
}
