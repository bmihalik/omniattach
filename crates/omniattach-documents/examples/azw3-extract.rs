//! Experimental AZW3 projection using the shared library extraction path.
use omniattach_documents::{Format, Options, extract_as};
use std::{fs::File, io::Read};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .ok_or("usage: azw3-extract INPUT [json|markdown|text]")?;
    let output = args.next().unwrap_or_else(|| "json".into());
    if args.next().is_some() || !matches!(output.as_str(), "json" | "markdown" | "text") {
        return Err("usage: azw3-extract INPUT [json|markdown|text]".into());
    }
    let options = Options::default();
    let mut bytes = Vec::new();
    File::open(&input)?
        .take(options.limits.input_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)?;
    let document = extract_as(&input, &bytes, Format::Azw3, &options)?;
    eprintln!(
        "Experimental AZW3 projection: inspect JSON diagnostics; completeness and reading order are not guaranteed."
    );
    match output.as_str() {
        "markdown" => println!("{}", document.to_markdown()),
        "text" => println!("{}", document.to_text()),
        _ => println!("{}", serde_json::to_string_pretty(&document)?),
    }
    Ok(())
}
