//! Pure naming plans; no filesystem access, text rewriting or model inference.
use crate::{Error, PdfElementKind, PdfInspection, Result, Source};
use serde::Serialize;
use std::path::Path;
const MAX_SLUG: usize = 64;

/// Transliterate a cross-platform basename, remove its final extension and sanitize to ASCII.
pub fn sanitize_document_stem(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let stem = Path::new(base)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let bounded: String = stem.chars().take(256).collect();
    let ascii = deunicode::deunicode_with_tofu(&bounded, "-");
    let mut slug = String::new();
    for b in ascii.bytes() {
        if slug.len() == MAX_SLUG {
            break;
        }
        if b.is_ascii_alphanumeric() {
            slug.push(b.to_ascii_lowercase() as char);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        return "document".into();
    }
    let numbered = |prefix: &str| {
        slug.strip_prefix(prefix)
            .is_some_and(|n| n.len() == 1 && matches!(n.as_bytes()[0], b'1'..=b'9'))
    };
    if matches!(slug.as_str(), "con" | "prn" | "aux" | "nul") || numbered("com") || numbered("lpt")
    {
        format!("document-{slug}")
    } else {
        slug
    }
}
/// Default eight-character source hash prefix; full hashes remain in the plan's Source.
pub fn lfe_document_prefix(source: &Source) -> Result<String> {
    prefix(source, 8)
}
fn prefix(source: &Source, hash_len: usize) -> Result<String> {
    if !(8..=64).contains(&hash_len)
        || source.sha256.len() != 64
        || !source.sha256.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(Error::InvalidInput(
            "LFE naming requires a 64-digit SHA-256 and hash prefix length in 8..=64".into(),
        ));
    }
    Ok(format!(
        "{}-{}",
        sanitize_document_stem(&source.name),
        source.sha256[..hash_len].to_ascii_lowercase()
    ))
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LfeElementName {
    index: usize,
    element_sequence: usize,
    kind: PdfElementKind,
    stem: String,
}
impl LfeElementName {
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn element_sequence(&self) -> usize {
        self.element_sequence
    }
    pub fn kind(&self) -> PdfElementKind {
        self.kind
    }
    pub fn stem(&self) -> &str {
        &self.stem
    }
    /// Extension without a leading dot; naming alone does not establish a file's format.
    pub fn file_name(&self, extension: &str) -> Result<String> {
        if extension.is_empty()
            || extension.len() > 10
            || !extension.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            return Err(Error::InvalidInput(
                "LFE extension must contain 1..=10 ASCII alphanumeric characters without a dot"
                    .into(),
            ));
        }
        Ok(format!("{}.{}", self.stem, extension.to_ascii_lowercase()))
    }
    /// Original images and rendered equation/table crops share the images/ directory.
    pub fn image_path(&self, extension: &str) -> Result<String> {
        let extension = extension.to_ascii_lowercase();
        if !matches!(
            extension.as_str(),
            "png" | "jpg" | "jpeg" | "jp2" | "webp" | "gif" | "bmp" | "tif" | "tiff"
        ) {
            return Err(Error::InvalidInput(
                "unsupported LFE image filename extension".into(),
            ));
        }
        Ok(format!("images/{}", self.file_name(&extension)?))
    }
}
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LfeNamingPlan {
    schema_version: String,
    generator: String,
    inspection_schema: String,
    source: Source,
    document_prefix: String,
    hash_prefix_chars: usize,
    elements: Vec<LfeElementName>,
}
impl LfeNamingPlan {
    pub fn from_inspection(report: &PdfInspection) -> Result<Self> {
        Self::from_inspection_with_hash_len(report, 8)
    }
    pub fn from_inspection_with_hash_len(report: &PdfInspection, hash_len: usize) -> Result<Self> {
        if report.schema_version != "omniattach/pdf-inspection/0.1" {
            return Err(Error::InvalidInput(
                "unsupported LFE inspection schema".into(),
            ));
        }
        let document_prefix = prefix(&report.source, hash_len)?;
        let mut elements = Vec::new();
        let mut index = 0usize;
        for (position, element) in report.elements.iter().enumerate() {
            if element.sequence != position + 1
                || element.anchor.page == 0
                || element.anchor.page as usize > report.page_count
            {
                return Err(Error::InvalidInput(
                    "LFE naming requires contiguous ordered sequences and valid one-based pages"
                        .into(),
                ));
            }
            if element.kind == PdfElementKind::Text {
                continue;
            }
            index = index
                .checked_add(1)
                .ok_or_else(|| Error::LimitExceeded("LFE naming index overflow".into()))?;
            elements.push(LfeElementName {
                index,
                element_sequence: element.sequence,
                kind: element.kind,
                stem: format!("{document_prefix}-fe-{index:06}"),
            });
        }
        Ok(Self {
            schema_version: "omniattach/lfe-names/0.1".into(),
            generator: format!("omniattach-documents/{}", env!("CARGO_PKG_VERSION")),
            inspection_schema: report.schema_version.clone(),
            source: report.source.clone(),
            document_prefix,
            hash_prefix_chars: hash_len,
            elements,
        })
    }
    pub fn source(&self) -> &Source {
        &self.source
    }
    pub fn document_prefix(&self) -> &str {
        &self.document_prefix
    }
    pub fn elements(&self) -> &[LfeElementName] {
        &self.elements
    }
    /// Check before combining plans in one export namespace; identical plans are idempotent.
    pub fn check_collisions(&self, other: &Self) -> Result<()> {
        if self.document_prefix == other.document_prefix
            && !self.elements.is_empty()
            && !other.elements.is_empty()
            && self != other
        {
            return Err(Error::InvalidInput("LFE name collision: use a longer hash prefix or separate export namespaces; do not overwrite files".into()));
        }
        Ok(())
    }
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}
impl PdfInspection {
    /// Name non-text candidates without mutating the inspection or creating files.
    pub fn naming_plan(&self) -> Result<LfeNamingPlan> {
        LfeNamingPlan::from_inspection(self)
    }
}
