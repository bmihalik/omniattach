use omniattach_backends::*;
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        return Err(Error::Configuration(
            "usage: custom_markdown WRAPPER_PROGRAM INPUT".into(),
        ));
    }
    let backend = CustomBackend {
        name: "local-markdown-wrapper".into(),
        tool: Tool::new(&args[0]),
        args: vec!["{input}".into()],
        output: CustomOutput::MarkdownStdout,
    };
    let result = backend.convert(
        std::path::Path::new(&args[1]),
        &BackendContext::new(&ProcessRunner),
    )?;
    println!("{}", result.manifest.to_json()?);
    Ok(())
}
