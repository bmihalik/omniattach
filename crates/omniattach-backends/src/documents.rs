use crate::{
    ArtifactBundle, AssetKind, Backend, BackendContext, Capability, Error, Location, Manifest,
    Result, Tool, parse_pdf_pages, parse_tesseract_tsv, workspace::Workspace,
};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Poppler {
    pub tool: Tool,
    pub layout: bool,
}
impl Default for Poppler {
    fn default() -> Self {
        Self {
            tool: Tool::new("pdftotext"),
            layout: false,
        }
    }
}
impl Backend for Poppler {
    fn id(&self) -> &str {
        "poppler"
    }
    fn capability(&self) -> Capability {
        Capability::PdfText
    }
    fn convert(&self, source: &Path, ctx: &BackendContext<'_>) -> Result<ArtifactBundle> {
        let ws = Workspace::new(source, ctx.scratch_parent.as_deref(), &ctx.limits)?;
        let mut manifest = Manifest::new(self.id(), ws.source.clone());
        let mut command = self
            .tool
            .command(ws.root())
            .args(["-enc", "UTF-8", "-eol", "unix"]);
        if self.layout {
            command = command.arg("-layout");
        }
        let output = ctx.run(&command.arg(ws.input.as_os_str()).arg("-"), &mut manifest)?;
        manifest.text = parse_pdf_pages(output.stdout_text()?);
        if manifest.text.is_empty() {
            manifest.warn(
                "NO_TEXT",
                "PDF contains no extracted text; OCR may be needed",
            );
        }
        let empty: Vec<_> = manifest
            .text
            .iter()
            .filter(|p| p.text.is_empty())
            .filter_map(|p| p.location.page)
            .collect();
        if !empty.is_empty() {
            manifest.warn(
                "EMPTY_PAGES",
                &format!("No extracted text on pages {empty:?}"),
            );
        }
        manifest.warn(
            "TEXT_ONLY",
            "No layout regions, table recognition, image extraction or OCR",
        );
        ws.finish(manifest, &ctx.limits)
    }
}

#[derive(Debug, Clone)]
pub struct Tesseract {
    pub tool: Tool,
    pub language: String,
    pub page_segmentation: Option<u8>,
}
impl Default for Tesseract {
    fn default() -> Self {
        Self {
            tool: Tool::new("tesseract"),
            language: "eng".into(),
            page_segmentation: None,
        }
    }
}
impl Backend for Tesseract {
    fn id(&self) -> &str {
        "tesseract"
    }
    fn capability(&self) -> Capability {
        Capability::Ocr
    }
    fn convert(&self, source: &Path, ctx: &BackendContext<'_>) -> Result<ArtifactBundle> {
        let ws = Workspace::new(source, ctx.scratch_parent.as_deref(), &ctx.limits)?;
        let mut manifest = Manifest::new(self.id(), ws.source.clone());
        let mut command = self
            .tool
            .command(ws.root())
            .arg(ws.input.as_os_str())
            .args(["stdout", "-l", &self.language]);
        if let Some(psm) = self.page_segmentation {
            command = command.args(["--psm", &psm.to_string()]);
        }
        let output = ctx.run(&command.arg("tsv"), &mut manifest)?;
        manifest.text = parse_tesseract_tsv(output.stdout_text()?)?;
        if manifest.text.is_empty() {
            manifest.warn("NO_OCR_WORDS", "OCR produced no nonempty word records");
        }
        manifest.metadata = serde_json::json!({"language": self.language, "confidence_scale": "tesseract_native_0_100", "coordinates": "input_image_pixels"});
        manifest.warn(
            "WORD_LEVEL",
            "TSV word order retained; paragraphs and reading-order layout are not reconstructed",
        );
        ws.finish(manifest, &ctx.limits)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum OfficeFormat {
    Docx,
    Pptx,
    Xlsx,
    Odt,
    Odp,
    Ods,
    Pdf,
}
impl OfficeFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Docx => "docx",
            Self::Pptx => "pptx",
            Self::Xlsx => "xlsx",
            Self::Odt => "odt",
            Self::Odp => "odp",
            Self::Ods => "ods",
            Self::Pdf => "pdf",
        }
    }
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            Self::Pptx => {
                "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            }
            Self::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            Self::Odt => "application/vnd.oasis.opendocument.text",
            Self::Odp => "application/vnd.oasis.opendocument.presentation",
            Self::Ods => "application/vnd.oasis.opendocument.spreadsheet",
            Self::Pdf => "application/pdf",
        }
    }
}
#[derive(Debug, Clone)]
pub struct LibreOffice {
    pub tool: Tool,
    pub format: OfficeFormat,
}
impl LibreOffice {
    pub fn new(format: OfficeFormat) -> Self {
        Self {
            tool: Tool::new("libreoffice"),
            format,
        }
    }
}
fn file_uri(path: &Path) -> String {
    use std::fmt::Write;
    let mut uri = String::from("file://");
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes()
    };
    #[cfg(not(unix))]
    let binding = path.to_string_lossy();
    #[cfg(not(unix))]
    let bytes = binding.as_bytes();
    for &byte in bytes {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            uri.push(byte as char);
        } else {
            write!(uri, "%{byte:02X}").expect("string formatting");
        }
    }
    uri
}
impl Backend for LibreOffice {
    fn id(&self) -> &str {
        "libreoffice"
    }
    fn capability(&self) -> Capability {
        Capability::OfficeConversion
    }
    fn convert(&self, source: &Path, ctx: &BackendContext<'_>) -> Result<ArtifactBundle> {
        let ws = Workspace::new(source, ctx.scratch_parent.as_deref(), &ctx.limits)?;
        std::fs::create_dir(ws.root().join("out"))?;
        let mut manifest = Manifest::new(self.id(), ws.source.clone());
        let profile = file_uri(&ws.root().join("profile"));
        let command = self
            .tool
            .command(ws.root())
            .arg(format!("-env:UserInstallation={profile}"))
            .args([
                "--headless",
                "--norestore",
                "--nodefault",
                "--nofirststartwizard",
                "--convert-to",
                self.format.extension(),
                "--outdir",
            ])
            .arg(ws.root().join("out"))
            .arg(ws.input.as_os_str());
        ctx.run(&command, &mut manifest)?;
        let path = format!("out/input.{}", self.format.extension());
        let bytes = ws.read(&path, &ctx.limits)?;
        if (matches!(self.format, OfficeFormat::Pdf) && !bytes.starts_with(b"%PDF-"))
            || (!matches!(self.format, OfficeFormat::Pdf) && !bytes.starts_with(b"PK\x03\x04"))
        {
            return Err(Error::InvalidOutput(
                "Office output has unexpected file signature".into(),
            ));
        }
        manifest.assets.push(ws.asset(
            &path,
            self.format.media_type(),
            AssetKind::ConvertedDocument,
            Location::default(),
            &ctx.limits,
        )?);
        manifest.warn("CONVERTED_DOCUMENT", "Parse converted bytes with omniattach-documents; original layout and numeric fidelity may change");
        ws.finish(manifest, &ctx.limits)
    }
}
