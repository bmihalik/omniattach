//! Requires --features backends and the actual sibling backend sources.
use omniattach::*;
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        return Err(Error::Configuration("usage: external ROUTE INPUT NEW_EXPORT_DIR; routes: poppler,ffmpeg-frames,ffprobe,office-to-docx".into()));
    }
    let goal = match args[0].as_str() {
        "ffmpeg-frames" => Goal::VideoFrames,
        "ffprobe" => Goal::MediaProbe,
        "office-to-docx" => Goal::Normalize,
        _ => Goal::Extract,
    };
    let engine = OmniAttach::builder().with_external_defaults()?.build();
    let context = FacadeContext::default()
        .with_external(true)
        .with_progress(NdjsonProgress);
    let result = engine.convert(
        &ConversionRequest::new(&args[1], goal).with_route(&args[0]),
        &context,
    )?;
    result.export(&args[2])?;
    println!("{}", result.manifest().to_json()?);
    Ok(())
}
