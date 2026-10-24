use omniattach::*;
use serde_json::json;
fn main() -> Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| Error::Configuration("usage: custom INPUT.txt".into()))?;
    // Replace this closure with the actual sibling document extractor in your workspace.
    let adapter = DocumentAdapter::new(
        "example/source-text",
        |path: &std::path::Path, _: &serde_json::Value| {
            std::fs::read_to_string(path).map(|text| json!({"text":text}))
        },
    );
    let engine = OmniAttach::builder()
        .register(
            Route::new(
                "my-documents",
                vec![Format::Text],
                vec![Goal::Extract],
                ExecutionKind::InProcess,
            ),
            adapter,
        )?
        .build();
    let result = engine.convert(
        &ConversionRequest::new(path, Goal::Extract),
        &FacadeContext::default(),
    )?;
    println!("{}", result.manifest().to_json()?);
    Ok(())
}
