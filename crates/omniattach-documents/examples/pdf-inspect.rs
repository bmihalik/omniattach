#[cfg(feature = "pdf")]
fn main() -> omniattach_documents::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or_else(|| {
        omniattach_documents::Error::InvalidInput("usage: pdf-inspect INPUT.pdf".into())
    })?;
    if args.next().is_some() {
        return Err(omniattach_documents::Error::InvalidInput(
            "usage: pdf-inspect INPUT.pdf".into(),
        ));
    }
    let report =
        omniattach_documents::inspect_pdf(path, &omniattach_documents::Options::default())?;
    println!("{}", report.to_json()?);
    Ok(())
}
#[cfg(not(feature = "pdf"))]
fn main() {
    eprintln!("pdf-inspect requires --features pdf");
    std::process::exit(2);
}
