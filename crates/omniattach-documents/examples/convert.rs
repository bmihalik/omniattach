use omniattach_documents::{Options, XmlTableSpec, extract};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .ok_or("usage: convert INPUT [markdown|text|json|csv] [XML_MAPPING.json]")?;
    let output = args.next().unwrap_or_else(|| "markdown".into());
    let mut options = Options::default();
    if let Some(mapping) = args.next() {
        options.xml_table = Some(serde_json::from_str::<XmlTableSpec>(
            &std::fs::read_to_string(mapping)?,
        )?);
    }
    let doc = extract(input, &options)?;
    for diagnostic in &doc.diagnostics {
        eprintln!("{}: {}", diagnostic.code, diagnostic.message);
    }
    let text = match output.as_str() {
        "markdown" | "md" => doc.to_markdown(),
        "text" | "txt" => doc.to_text(),
        "json" => doc.to_json()?,
        "csv" => doc
            .tables
            .first()
            .ok_or("document contains no table")?
            .to_csv()?,
        _ => return Err("output must be markdown, text, json or csv".into()),
    };
    print!("{text}");
    Ok(())
}
