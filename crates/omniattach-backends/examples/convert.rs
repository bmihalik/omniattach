use omniattach_backends::*;
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        return Err(Error::Configuration("usage: convert BACKEND INPUT [EXPORT_DIR]; BACKEND=poppler|ocr|office-pdf|audio|frames|probe".into()));
    }
    let mut registry = Registry::new();
    registry.register(Poppler::default())?;
    registry.register(Tesseract::default())?;
    registry.register(LibreOffice::new(OfficeFormat::Pdf))?;
    registry.register(FfmpegAudio::default())?;
    registry.register(FfmpegFrames::default())?;
    registry.register(Ffprobe::default())?;
    let id = match args[0].as_str() {
        "ocr" => "tesseract",
        "office-pdf" => "libreoffice",
        "audio" => "ffmpeg-audio",
        "frames" => "ffmpeg-frames",
        "probe" => "ffprobe",
        other => other,
    };
    let ctx = BackendContext::new(&ProcessRunner);
    let result = registry.convert(id, &args[1], &ctx)?;
    if let Some(export) = args.get(2) {
        result.export(export)?;
    }
    println!("{}", result.manifest.to_json()?);
    Ok(())
}
