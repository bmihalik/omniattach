//! LFE-STEP-5 first slice: explicit-grid, native-text PDF table conversion.
#[cfg(feature = "pdf")]
#[path = "lfe_table/engine.rs"]
mod engine;
#[cfg(feature = "pdf")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use omniattach_documents::{inspect_pdf, Options};
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: pdf-table INPUT.pdf PAGE X0,X1,... Y0,Y1,... (ascending PDF user-space coordinates)".into());
    }
    let page: u32 = args[1].parse()?;
    if page == 0 { return Err("PAGE is one-based".into()); }
    let axis = |raw: &str| -> Result<Vec<f64>, Box<dyn std::error::Error>> {
        if raw.len() > 8192 { return Err("grid argument too long".into()); }
        raw.split(',').map(|v| v.trim().parse::<f64>().map_err(Into::into)).collect()
    };
    let xs = axis(&args[2])?;
    let ys = axis(&args[3])?;
    let inspection = inspect_pdf(&args[0], &Options::default())?;
    // Read the stable inspection inventory, without changing ordinary extraction.
    let value: serde_json::Value = serde_json::from_str(&inspection.to_json()?)?;
    let elements = value.get("elements").and_then(|v| v.as_array()).ok_or("inspection has no elements array")?;
    let mut runs = Vec::new();
    for element in elements {
        if element["anchor"]["page"].as_u64() != Some(u64::from(page)) || element["kind"].as_str() != Some("text") { continue; }
        let bounds = &element["bounds"];
        let Some(left) = bounds["left"].as_f64() else { continue; };
        let bottom = bounds["bottom"].as_f64().ok_or("missing text bottom")?;
        let right = bounds["right"].as_f64().ok_or("missing text right")?;
        let top = bounds["top"].as_f64().ok_or("missing text top")?;
        let text = element["text"].as_str().ok_or("undecodable text on selected page")?;
        runs.push(engine::Run { bounds: [left,bottom,right,top], text: text.into(), gap: element["word_gap_before"].as_bool().unwrap_or(false), sequence: element["sequence"].as_u64().unwrap_or(0) as usize });
    }
    if runs.is_empty() { return Err("no positioned decoded text on selected page".into()); }
    let markdown = engine::convert(&xs, &ys, &runs)?;
    eprintln!("LFE-STEP-5 explicit grid: page {page}; source {}; first row is header; estimated text bounds", args[0]);
    print!("{markdown}");
    Ok(())
}
#[cfg(not(feature = "pdf"))]
fn main() { eprintln!("pdf-table requires --features pdf"); std::process::exit(1); }
