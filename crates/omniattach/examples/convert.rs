use omniattach::*;
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return Err(Error::Configuration(
            "usage: convert INPUT [NEW_EXPORT_DIRECTORY]".into(),
        ));
    }
    let engine = OmniAttach::new();
    let context = FacadeContext::default().with_progress(NdjsonProgress);
    let result = engine.convert(&ConversionRequest::new(&args[0], Goal::Markdown), &context)?;
    if let Some(path) = args.get(1) {
        result.export(path)?;
    }
    println!("{}", result.manifest().to_json()?);
    Ok(())
}
