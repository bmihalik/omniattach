#[cfg(feature = "pdf")]
fn main() -> omniattach_documents::Result<()> {
    let usage = || {
        omniattach_documents::Error::InvalidInput("usage: pdf-inspect [--names] INPUT.pdf".into())
    };
    let mut args = std::env::args().skip(1);
    let first = args.next().ok_or_else(usage)?;
    let names = first == "--names";
    let path = if names {
        args.next().ok_or_else(usage)?
    } else {
        first
    };
    if args.next().is_some() {
        return Err(usage());
    }
    let report =
        omniattach_documents::inspect_pdf(path, &omniattach_documents::Options::default())?;
    let json = if names {
        report.naming_plan()?.to_json()?
    } else {
        report.to_json()?
    };
    println!("{json}");
    Ok(())
}
