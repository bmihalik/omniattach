use crate::{BackendContext, Error, Result, Tool};
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
    /// Renders one PDF page (one-based) and returns recognized plain text. Empty text means no words were found.
    pub fn recognize_page(
        &self,
        source: &Path,
        page: u32,
        ctx: &BackendContext<'_>,
    ) -> Result<String> {
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
            .args(["stdout", "-l", &self.language]);
        let output = ctx.runner.run(&ocr, &ctx.run_options)?;
        Ok(output.stdout_text()?.trim().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CommandSpec, ProcessOutput, RunOptions, Runner};
    use std::{path::PathBuf, time::Duration};

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
                "tesseract" => b"Scanned words\n".to_vec(),
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
        let text = PdfPageOcr::default()
            .recognize_page(&PathBuf::from("scan.pdf"), 3, &ctx)
            .unwrap();
        assert_eq!(text, "Scanned words");
        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0][0], "pdftoppm");
        assert!(calls[0].windows(2).any(|w| w == ["-f", "3"]));
        assert!(calls[0].windows(2).any(|w| w == ["-l", "3"]));
        assert_eq!(calls[1][0], "tesseract");
        assert!(calls[1].contains(&"stdout".to_string()));
    }
}
