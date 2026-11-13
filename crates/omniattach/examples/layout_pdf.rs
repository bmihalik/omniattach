#[cfg(feature = "backends")]
fn main() -> omniattach::Result<()> {
    use omniattach::*;
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 || args.len() > 5 {
        return Err(Error::Configuration(
            "usage: layout_pdf mineru-sdk|mineru-legacy|marker INPUT NEW_EXPORT [PROGRAM]".into(),
        ));
    }
    let (engine, route) = match args[1].as_str() {
        "mineru-sdk" | "mineru-legacy" => {
            let profile = if args[1] == "mineru-sdk" {
                backends::MinerUProfile::SdkSave
            } else {
                backends::MinerUProfile::LegacyPipeline
            };
            let mut backend = backends::MinerU::new(profile);
            if let Some(program) = args.get(4) {
                backend.tool = backends::Tool::new(program.clone());
            }
            (
                OmniAttach::builder().with_mineru(backend)?.build(),
                "mineru",
            )
        }
        "marker" => {
            let mut backend = backends::Marker::default();
            if let Some(program) = args.get(4) {
                backend.tool = backends::Tool::new(program.clone());
            }
            (
                OmniAttach::builder().with_marker(backend)?.build(),
                "marker",
            )
        }
        _ => {
            return Err(Error::Configuration(
                "unknown layout backend/profile".into(),
            ));
        }
    };
    let bundle = engine.convert(
        &ConversionRequest::new(args[2].as_str(), Goal::Markdown).with_route(route),
        &FacadeContext::default().with_external(true),
    )?;
    bundle.export(args[3].as_str())?;
    println!("{}", bundle.manifest().to_json()?);
    Ok(())
}
#[cfg(not(feature = "backends"))]
fn main() {
    eprintln!("layout_pdf requires --features backends");
    std::process::exit(2);
}
