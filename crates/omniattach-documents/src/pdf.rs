use crate::{BlockKind, Diagnostic, Document, Error, Options, Result, SourceSpan};

pub(crate) fn extract(doc: &mut Document, bytes: &[u8], options: &Options) -> Result<()> {
    let pdf = lopdf::Document::load_mem_with_options(
        bytes,
        lopdf::LoadOptions {
            max_decompressed_size: Some(
                options.limits.archive_bytes.min(usize::MAX as u64) as usize
            ),
            ..Default::default()
        },
    )
    .map_err(|e| Error::Parse(format!("PDF: {e}")))?;
    if pdf.is_encrypted() {
        return Err(Error::InvalidInput(
            "encrypted PDF is unsupported; decrypt explicitly before extraction".into(),
        ));
    }
    let pages = pdf.get_pages();
    if pages.len() > options.limits.blocks {
        return Err(Error::LimitExceeded("PDF pages".into()));
    }
    for page in pages.keys() {
        let span = SourceSpan {
            page: Some(*page),
            ..Default::default()
        };
        let text = pdf
            .extract_text_with_limit(&[*page], options.limits.archive_bytes as usize)
            .map_err(|e| Error::Parse(format!("PDF page {page}: {e}")))?;
        if text.trim().is_empty() {
            doc.diagnostics.push(Diagnostic {
                code: "PAGE_WITHOUT_TEXT".into(),
                message: format!("Page {page} contains no extractable text; OCR is not included"),
                source: Some(span),
            });
        } else {
            doc.paragraph(BlockKind::Paragraph, text.trim_end().into(), span);
        }
    }
    doc.warn("PDF_TEXT_ONLY", "PDF text/content-stream order only; no OCR, layout reconstruction, headings, tables, equations or images");
    Ok(())
}
