//! Batch crop recognizer. Provision model files explicitly; no Hub constructor.
use bobine::TexTeller;
use serde_json::{Value, json};
use std::{io::{self, Read}, path::Path, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 5 {
        return Err("usage: lfe-formula-bobine ENCODER DECODER TOKENIZER MAX_TOKENS".into());
    }
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let request: Value = serde_json::from_str(&input)?;
    if request["schema_version"] != "omniattach/formula-worker/0.1" {
        return Err("unsupported worker protocol".into());
    }
    let start = Instant::now();
    let mut model = TexTeller::load_from_paths(
        Path::new(&args[1]), Path::new(&args[2]), Path::new(&args[3]),
        &["CPUExecutionProvider".to_string()],
    )?;
    model.max_tokens = args[4].parse()?;
    let load_ms = start.elapsed().as_secs_f64()*1000.0;
    let mut results = Vec::new();
    for image in request["images"].as_array().ok_or("missing images")? {
        let start = Instant::now();
        let prediction = model.recognize(Path::new(image["path"].as_str().ok_or("missing path")?));
        let elapsed_ms = start.elapsed().as_secs_f64()*1000.0;
        results.push(match prediction {
            Ok(latex) => json!({"id":image["id"],"status":"ok","latex":latex,"elapsed_ms":elapsed_ms}),
            Err(error) => json!({"id":image["id"],"status":"error","error":error.to_string(),"elapsed_ms":elapsed_ms}),
        });
    }
    println!("{}", json!({"results":results,"metadata":{
        "backend":"bobine/0.9.1","device":"CPUExecutionProvider","load_ms":load_ms,
        "max_tokens":model.max_tokens,"completion":"unknown; public API returns strings only",
        "postprocessing":"Bobine recognize output without additional cleanup"}}));
    Ok(())
}
