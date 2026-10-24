use omniattach_backends::*;
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        return Err(Error::Configuration(
            "usage: transcribe INPUT MODEL [LANGUAGE] [EXPORT_DIR]".into(),
        ));
    }
    let mut backend = WhisperCpp::new(&args[1]);
    if let Some(language) = args.get(2) {
        backend.language = language.into();
    }
    let ctx = BackendContext::new(&ProcessRunner);
    let result = backend.convert(std::path::Path::new(&args[0]), &ctx)?;
    if let Some(export) = args.get(3) {
        result.export(export)?;
    }
    println!("{}", result.manifest.to_json()?);
    Ok(())
}
