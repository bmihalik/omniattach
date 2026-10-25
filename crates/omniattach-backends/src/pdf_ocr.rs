use crate::{BackendContext, Error, Result, TextUnit, Tool, parse_tesseract_tsv};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct PdfPageOcr {
    pub rasterizer: Tool,
    pub tesseract: Tool,
    pub language: String,
    pub dpi: u32,
}
impl Default for PdfPageOcr {
    fn default() -> Self {
        Self {
            rasterizer: Tool::new("pdftoppm"),
            tesseract: Tool::new("tesseract"),
            language: "eng".into(),
            dpi: 300,
        }
    }
}

impl PdfPageOcr {
    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = language.into();
        self
    }

    pub fn with_dpi(mut self, dpi: u32) -> Self {
        self.dpi = dpi;
        self
    }

    /// Renders one PDF page (one-based) and returns word-level OCR units with bounding boxes and native confidence.
    pub fn recognize_page_words(
        &self,
        source: &Path,
        page: u32,
        ctx: &BackendContext<'_>,
    ) -> Result<Vec<TextUnit>> {
        if !(72..=1200).contains(&self.dpi) {
            return Err(Error::Configuration(
                "PDF OCR DPI must be in 72..=1200".into(),
            ));
        }
        if self.language.trim().is_empty() {
            return Err(Error::Configuration(
                "PDF OCR language must be nonempty".into(),
            ));
        }
        let scratch = tempfile::tempdir()?;
        let prefix = scratch.path().join("page");
        let render = self
            .rasterizer
            .command(scratch.path())
            .args([
                "-r",
                &self.dpi.to_string(),
                "-f",
                &page.to_string(),
                "-l",
                &page.to_string(),
                "-singlefile",
                "-png",
            ])
            .arg(source.as_os_str())
            .arg(prefix.as_os_str());
        ctx.runner.run(&render, &ctx.run_options)?;
        let image = scratch.path().join("page.png");
        if !image.is_file() {
            return Err(Error::InvalidOutput(
                "pdftoppm produced no page image".into(),
            ));
        }
        let ocr = self
            .tesseract
            .command(scratch.path())
            .arg(image.as_os_str())
            .args(["stdout", "-l", &self.language, "tsv"]);
        let output = ctx.runner.run(&ocr, &ctx.run_options)?;
        let mut words = parse_tesseract_tsv(output.stdout_text()?)?;
        for word in &mut words {
            word.location.page = Some(page);
        }
        Ok(words)
    }

    /// Renders one PDF page (one-based) and returns recognized plain text joined from TSV words. Empty text means no words were found.
    pub fn recognize_page(
        &self,
        source: &Path,
        page: u32,
        ctx: &BackendContext<'_>,
    ) -> Result<String> {
        let words = self.recognize_page_words(source, page, ctx)?;
        Ok(words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CommandSpec, ProcessOutput, RunOptions, Runner};
    use std::{path::PathBuf, time::Duration};

    const SAMPLE_TSV: &str = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
5\t1\t1\t1\t1\t1\t10\t20\t30\t40\t95.5\tScanned\n\
5\t1\t1\t1\t1\t2\t50\t20\t35\t40\t92.0\twords\n";

    struct FakeRunner {
        calls: std::sync::Mutex<Vec<Vec<String>>>,
    }
    impl Runner for FakeRunner {
        fn run(&self, spec: &CommandSpec, _: &RunOptions) -> Result<ProcessOutput> {
            let program = spec
                .program
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let args: Vec<String> = spec
                .args
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            self.calls.lock().unwrap().push(
                std::iter::once(program.clone())
                    .chain(args.clone())
                    .collect(),
            );
            let stdout = match program.as_str() {
                "pdftoppm" => {
                    std::fs::write(spec.cwd.join("page.png"), b"png")?;
                    Vec::new()
                }
                "tesseract" => SAMPLE_TSV.as_bytes().to_vec(),
                other => panic!("unexpected program {other}"),
            };
            Ok(ProcessOutput {
                stdout,
                stderr: Vec::new(),
                elapsed: Duration::ZERO,
            })
        }
    }

    #[test]
    fn renders_requested_page_then_runs_tesseract() {
        let runner = FakeRunner {
            calls: std::sync::Mutex::new(Vec::new()),
        };
        let ctx = BackendContext::new(&runner);
        let words = PdfPageOcr::default()
            .with_language("hun+eng")
            .with_dpi(200)
            .recognize_page_words(&PathBuf::from("scan.pdf"), 3, &ctx)
            .unwrap();
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "Scanned");
        assert_eq!(words[0].location.page, Some(3));
        assert_eq!(words[0].confidence, Some(95.5));
        assert!(words[0].location.bbox.is_some());
        assert_eq!(words[1].text, "words");
        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0][0], "pdftoppm");
        assert!(calls[0].windows(2).any(|w| w == ["-r", "200"]));
        assert!(calls[0].windows(2).any(|w| w == ["-f", "3"]));
        assert!(calls[0].windows(2).any(|w| w == ["-l", "3"]));
        assert_eq!(calls[1][0], "tesseract");
        assert!(calls[1].contains(&"stdout".to_string()));
        assert!(calls[1].contains(&"tsv".to_string()));
        assert!(calls[1].windows(2).any(|w| w == ["-l", "hun+eng"]));
    }

    #[test]
    fn rejects_invalid_dpi_or_empty_language() {
        let runner = FakeRunner {
            calls: std::sync::Mutex::new(Vec::new()),
        };
        let ctx = BackendContext::new(&runner);
        assert!(
            PdfPageOcr::default()
                .with_dpi(10)
                .recognize_page(&PathBuf::from("scan.pdf"), 1, &ctx)
                .is_err()
        );
        assert!(
            PdfPageOcr::default()
                .with_language("  ")
                .recognize_page(&PathBuf::from("scan.pdf"), 1, &ctx)
                .is_err()
        );
    }
}
