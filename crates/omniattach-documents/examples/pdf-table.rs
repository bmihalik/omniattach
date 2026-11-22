//! Explicit-grid table conversion through the typed library API.
#[cfg(feature = "pdf")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use omniattach_documents::{Options, inspect_pdf, pdf_tables::{PdfTableGrid, convert_inspection}};
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len()!=4 { return Err("usage: pdf-table INPUT.pdf PAGE X0,X1,... Y0,Y1,... (ascending PDF user-space coordinates)".into()); }
    let page: u32=args[1].parse()?;
    let axis=|raw:&str|->Result<Vec<f64>,Box<dyn std::error::Error>> {
        if raw.len()>8192 { return Err("grid argument too long".into()); }
        raw.split(',').map(|v|v.trim().parse::<f64>().map_err(Into::into)).collect()
    };
    let grid=PdfTableGrid { page,x_boundaries:axis(&args[2])?,y_boundaries:axis(&args[3])? };
    let options=Options::default();
    let report=inspect_pdf(&args[0],&options)?;
    let result=convert_inspection(&report,&grid,&options)?;
    eprintln!("LFE-STEP-5 explicit grid: page {page}; source {}; first row is header; estimated text bounds; source SHA-256 {}",result.source_name,result.source_sha256);
    print!("{}",result.markdown); Ok(())
}
#[cfg(not(feature = "pdf"))]
fn main() { eprintln!("pdf-table requires --features pdf"); std::process::exit(1); }
