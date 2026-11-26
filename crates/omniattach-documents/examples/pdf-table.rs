//! Development helper: convert a manually chosen grid, bypassing discovery.
//!
//! Useful when adding a detector for a new PDF layout: read candidate
//! boundaries from `pdf-inspect` output, then check the resulting Markdown.
//! For discovered tables use the CLI: `omniattach pdf-tables` / `pdf-table`.
#[cfg(feature = "pdf")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use omniattach_documents::{
        Options, inspect_pdf,
        pdf_tables::{PdfTableGrid, convert_inspection},
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: pdf-table INPUT.pdf PAGE X0,X1,... Y0,Y1,...".into());
    }
    let axis = |raw: &str| -> Result<Vec<f64>, std::num::ParseFloatError> {
        raw.split(',').map(|v| v.trim().parse()).collect()
    };
    let grid = PdfTableGrid {
        page: args[1].parse()?,
        x_boundaries: axis(&args[2])?,
        y_boundaries: axis(&args[3])?,
        cell_spans: Vec::new(),
        rotation: 0,
        text_assignment: Default::default(),
    };
    let options = Options::default();
    let result = convert_inspection(&inspect_pdf(&args[0], &options)?, &grid, &options)?;
    for warning in &result.warnings {
        eprintln!("{warning}");
    }
    print!("{}", result.markdown);
    Ok(())
}
#[cfg(not(feature = "pdf"))]
fn main() {
    eprintln!("pdf-table requires --features pdf");
    std::process::exit(1);
}
