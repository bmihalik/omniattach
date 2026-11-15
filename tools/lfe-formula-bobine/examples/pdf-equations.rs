//! Real model example; isolated from ordinary omniattach dependency resolution.
use bobine::TexTeller;
use omniattach_documents::{pdf_equations::*, *};
use serde_json::json;
use std::{fs::{self, File, OpenOptions}, io::{Read, Write}, path::{Component, Path}};
fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    OpenOptions::new().write(true).create_new(true).open(path)?.write_all(bytes)
}
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let list = args.first().is_some_and(|s| s == "--list");
    if (list && args.len() != 2) || (!list && args.len() < 7) {
        return Err("usage: pdf-equations --list INPUT.pdf | INPUT.pdf NEW_OUT ENCODER DECODER TOKENIZER MAX_TOKENS BLOCK_ID...".into());
    }
    let input = &args[usize::from(list)]; let options = Options::default(); let mut bytes = Vec::new();
    File::open(input)?.take(options.limits.input_bytes.saturating_add(1) as u64).read_to_end(&mut bytes)?;
    let doc = extract_pdf_with_images(input, &bytes, &options)?;
    if list {
        let occurrences: Vec<_> = doc.blocks.iter().filter_map(|b| match &b.kind {
            BlockKind::Image { asset_id } => Some(json!({"block_id":b.id,"asset_id":asset_id,"page":b.source.page})), _ => None,
        }).collect();
        println!("{}", serde_json::to_string_pretty(&occurrences)?); return Ok(());
    }
    let selected = args[6..].to_vec(); let limits = EquationLimits::default();
    let unique: std::collections::BTreeSet<_> = selected.iter().collect();
    if selected.len() > limits.max_equations || unique.len() != selected.len() || selected.iter().any(|id| !doc.blocks.iter().any(|b| b.id == *id && matches!(&b.kind, BlockKind::Image { .. }))) {
        return Err("invalid image-occurrence selection".into());
    }
    let max_tokens = args[5].parse()?;
    if !(1..=512).contains(&max_tokens) { return Err("MAX_TOKENS must be 1..=512".into()); }
    let mut model = TexTeller::load_from_paths(Path::new(&args[2]), Path::new(&args[3]), Path::new(&args[4]), &["CPUExecutionProvider".to_string()])?;
    model.max_tokens = max_tokens;
    let output = Path::new(&args[1]); fs::create_dir(output)?; fs::create_dir(output.join("images"))?;
    for asset in &doc.assets {
        let path = Path::new(&asset.filename);
        if !asset.filename.starts_with("images/") || path.components().count() != 2 || !path.components().all(|c| matches!(c, Component::Normal(_))) {
            return Err("unsafe asset filename".into());
        }
        write_new(&output.join(path), &asset.bytes)?;
    }
    let mut recognize = |_: &Block, asset: &ExtractedAsset| {
        model.recognize(&output.join(&asset.filename)).map_err(|e| Error::Parse(e.to_string()))
    };
    let result = recognize_image_equations(doc, &selected, &mut recognize, &limits)?;
    write_new(&output.join("document.md"), result.to_markdown().as_bytes())?;
    write_new(&output.join("document.json"), result.to_json()?.as_bytes())?;
    println!("{}", json!({"selected":result.equations().len(),"output":output,"completion":"unknown; model returns strings only"}));
    Ok(())
}
