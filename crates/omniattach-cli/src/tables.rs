//! Opt-in synchronous table commands through the facade's documents re-export.
#[cfg(feature = "documents")]
use crate::args::TableOutputFormat;
use crate::{
    CliError, Result, Runtime,
    args::{PdfTableArgs, PdfTablesArgs},
};
use std::io::Write;

#[cfg(feature = "documents")]
fn options(args: &PdfTablesArgs) -> Result<omniattach::documents::Options> {
    if args.page == 0 {
        return Err(CliError::usage("page is one-based"));
    }
    let mut options = omniattach::documents::Options::default();
    let cap = usize::try_from(args.max_input_bytes)
        .map_err(|_| CliError::usage("max-input-bytes is too large on this platform"))?;
    options.limits.input_bytes = options.limits.input_bytes.min(cap);
    Ok(options)
}
#[cfg(feature = "documents")]
fn map_error(error: omniattach::documents::Error) -> CliError {
    match error {
        omniattach::documents::Error::LimitExceeded(message) => CliError::limit(message),
        other => CliError::input(other.to_string()),
    }
}
#[cfg(feature = "documents")]
fn emit<T: serde::Serialize>(
    value: &T,
    pretty: bool,
    limit: u64,
    runtime: &Runtime,
    out: &mut dyn Write,
) -> Result<()> {
    let mut buffer = if pretty {
        serde_json::to_vec_pretty(value)?
    } else {
        serde_json::to_vec(value)?
    };
    buffer.push(b'\n');
    if buffer.len() as u64 > limit {
        return Err(CliError::limit("table command output bytes"));
    }
    runtime.check()?;
    out.write_all(&buffer)?;
    Ok(())
}
/// Render and OCR the selected page; returns the source SHA-256 and the page.
#[cfg(feature = "table-ocr")]
fn scan(
    args: &PdfTablesArgs,
    options: &omniattach::documents::Options,
) -> Result<(
    Vec<u8>,
    omniattach::documents::pdf_tables::ocr::PdfTableOcrPage,
)> {
    use omniattach::documents::pdf_tables::ocr::{GrayRaster, OcrWord, PdfTableOcrPage};
    use omniattach_backends::{BackendContext, PdfPageOcr, ProcessRunner};
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(&args.input)?
        .take(options.limits.input_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > options.limits.input_bytes {
        return Err(CliError::limit("input bytes"));
    }
    let runner = ProcessRunner;
    let context = BackendContext::new(&runner);
    let ocr = PdfPageOcr::default()
        .with_language(args.ocr_language.clone())
        .with_dpi(args.ocr_dpi);
    let tool = |e: omniattach_backends::Error| {
        CliError::new("TOOL_UNAVAILABLE", format!("table OCR failed: {e}"), 8)
    };
    let pgm = ocr
        .render_page_gray(&args.input, args.page, &context)
        .map_err(tool)?;
    let raster = GrayRaster::from_pnm(&pgm).map_err(map_error)?;
    let dpi = f64::from(args.ocr_dpi);
    let units = ocr
        .recognize_image_words(&raster.for_ocr(dpi).to_pgm(), &context)
        .map_err(tool)?;
    let words = units
        .into_iter()
        .filter_map(|w| {
            let b = w.location.bbox?;
            Some(OcrWord {
                text: w.text,
                left: b.left,
                top: b.top,
                width: b.width,
                height: b.height,
                confidence: w.confidence,
            })
        })
        .collect();
    Ok((
        bytes,
        PdfTableOcrPage {
            page: args.page,
            dpi,
            raster,
            words,
        },
    ))
}
#[cfg(not(feature = "table-ocr"))]
fn no_ocr(args: &PdfTablesArgs) -> Result<()> {
    if args.ocr {
        return Err(CliError::unsupported(
            "--ocr requires the table-ocr Cargo feature",
        ));
    }
    Ok(())
}

pub(super) fn list(
    args: &PdfTablesArgs,
    pretty: bool,
    runtime: &Runtime,
    out: &mut dyn Write,
) -> Result<()> {
    #[cfg(feature = "documents")]
    {
        runtime.check()?;
        let options = options(args)?;
        #[cfg(feature = "table-ocr")]
        if args.ocr {
            let (bytes, page) = scan(args, &options)?;
            runtime.check()?;
            let result = omniattach::documents::pdf_tables::ocr::discover_ocr_tables(
                &args.input.to_string_lossy(),
                &bytes,
                &page,
                &options,
            )
            .map_err(map_error)?;
            return emit(&result, pretty, args.max_output_bytes, runtime, out);
        }
        #[cfg(not(feature = "table-ocr"))]
        no_ocr(args)?;
        let result = omniattach::documents::pdf_tables::discovery::discover_pdf_tables(
            &args.input,
            args.page,
            &options,
        )
        .map_err(map_error)?;
        emit(&result, pretty, args.max_output_bytes, runtime, out)
    }
    #[cfg(not(feature = "documents"))]
    {
        let _ = (args, pretty, runtime, out);
        Err(CliError::unsupported(
            "pdf-tables requires the documents Cargo feature",
        ))
    }
}
pub(super) fn convert(
    args: &PdfTableArgs,
    pretty: bool,
    runtime: &Runtime,
    out: &mut dyn Write,
) -> Result<()> {
    if args.candidate == 0 {
        return Err(CliError::usage("candidate numbers are one-based"));
    }
    #[cfg(feature = "documents")]
    {
        runtime.check()?;
        let options = options(&args.selection)?;
        #[cfg(not(feature = "table-ocr"))]
        no_ocr(&args.selection)?;
        #[cfg(feature = "table-ocr")]
        let result = if args.selection.ocr {
            let (bytes, page) = scan(&args.selection, &options)?;
            runtime.check()?;
            omniattach::documents::pdf_tables::ocr::convert_ocr_table(
                &args.selection.input.to_string_lossy(),
                &bytes,
                &page,
                args.candidate,
                &options,
            )
        } else {
            omniattach::documents::pdf_tables::discovery::convert_pdf_table(
                &args.selection.input,
                args.selection.page,
                args.candidate,
                &options,
            )
        }
        .map_err(map_error)?;
        #[cfg(not(feature = "table-ocr"))]
        let result = omniattach::documents::pdf_tables::discovery::convert_pdf_table(
            &args.selection.input,
            args.selection.page,
            args.candidate,
            &options,
        )
        .map_err(map_error)?;
        match args.output_format {
            TableOutputFormat::Json => emit(
                &result,
                pretty,
                args.selection.max_output_bytes,
                runtime,
                out,
            ),
            TableOutputFormat::Markdown => {
                if result.markdown.len() as u64 > args.selection.max_output_bytes {
                    return Err(CliError::limit("table command output bytes"));
                }
                runtime.check()?;
                out.write_all(result.markdown.as_bytes())?;
                Ok(())
            }
        }
    }
    #[cfg(not(feature = "documents"))]
    {
        let _ = (args, pretty, runtime, out);
        Err(CliError::unsupported(
            "pdf-table requires the documents Cargo feature",
        ))
    }
}
