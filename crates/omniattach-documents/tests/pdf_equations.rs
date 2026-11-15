#![cfg(feature = "pdf")]
use lopdf::{dictionary, Object, Stream};
use omniattach_documents::{pdf_equations::*, *};

fn pdf() -> Vec<u8> {
    let mut pdf = lopdf::Document::with_version("1.5");
    let pages = pdf.new_object_id();
    let image = pdf.add_object(Stream::new(dictionary! {"Type"=>"XObject", "Subtype"=>"Image", "Width"=>1, "Height"=>1, "ColorSpace"=>"DeviceRGB", "BitsPerComponent"=>8}, vec![255, 0, 0]));
    let resources = pdf.add_object(dictionary! {"XObject"=>dictionary! {"Im"=>image}});
    let content = pdf.add_object(Stream::new(dictionary! {}, b"q 10 0 0 10 0 0 cm /Im Do Q q 10 0 0 10 20 0 cm /Im Do Q".to_vec()));
    let page = pdf.add_object(dictionary! {"Type"=>"Page", "Parent"=>pages, "Contents"=>content});
    pdf.objects.insert(pages, Object::Dictionary(dictionary! {"Type"=>"Pages", "Kids"=>vec![Object::Reference(page)], "Count"=>1, "Resources"=>resources, "MediaBox"=>vec![Object::Integer(0),Object::Integer(0),Object::Integer(100),Object::Integer(100)]}));
    let catalog = pdf.add_object(dictionary! {"Type"=>"Catalog", "Pages"=>pages});
    pdf.trailer.set("Root", catalog);
    let mut bytes = Vec::new(); pdf.save_to(&mut bytes).unwrap(); bytes
}
fn images() -> Document { extract_pdf_with_images("math.pdf", &pdf(), &Options::default()).unwrap() }
fn ids(doc: &Document) -> Vec<String> { doc.blocks.iter().filter(|b| matches!(&b.kind, BlockKind::Image { .. })).map(|b| b.id.clone()).collect() }

#[test]
fn selected_occurrence_renders_latex_and_retains_source_and_shared_asset() {
    let doc = images(); let selected = vec![ids(&doc)[0].clone()]; let mut calls = 0;
    let mut model = |block: &Block, asset: &ExtractedAsset| {
        calls += 1; assert_eq!(block.source.page, Some(1)); assert_eq!(asset.media_type, "image/png");
        Ok(r"\[\frac{a+b}{c-d}\]".to_string())
    };
    let result = recognize_image_equations(doc.clone(), &selected, &mut model, &EquationLimits::default()).unwrap();
    assert_eq!(calls, 1); assert_eq!(result.document(), &doc); assert_eq!(result.document().assets.len(), 1);
    let md = result.to_markdown(); assert_eq!(md.matches("![").count(), 1); assert!(md.contains("$$\n\\frac{a+b}{c-d}\n$$"));
    assert_eq!(result.equations()[0].block_id, selected[0]); assert_eq!(result.equations()[0].source, doc.blocks[0].source);
    let json: serde_json::Value = serde_json::from_str(&result.to_json().unwrap()).unwrap();
    assert_eq!(json["schema_version"], "omniattach/pdf-equations/0.1"); assert_eq!(json["equations"][0]["status"], "recognized");
}
#[test]
fn reusable_callback_handles_distinct_occurrences_of_one_asset() {
    let doc = images(); let selected = ids(&doc); let mut calls = 0;
    let mut model = |_: &Block, _: &ExtractedAsset| { calls += 1; Ok("x=1".into()) };
    let result = recognize_image_equations(doc, &selected, &mut model, &EquationLimits::default()).unwrap();
    assert_eq!(calls, 2); assert_eq!(result.equations().len(), 2); assert_ne!(result.equations()[0].block_id, result.equations()[1].block_id);
    assert_eq!(result.equations()[0].asset_id, result.equations()[1].asset_id); assert_eq!(result.to_markdown().matches("x=1").count(), 2);
}
#[test]
fn callback_errors_and_empty_math_keep_image_fallback() {
    for text in [None, Some(""), Some(r"\[ \]")] {
        let doc = images(); let expected = doc.to_markdown(); let selected = ids(&doc);
        let mut model = |_: &Block, _: &ExtractedAsset| match text { Some(s) => Ok(s.to_string()), None => Err(Error::Parse("model failed".into())) };
        let result = recognize_image_equations(doc, &selected, &mut model, &EquationLimits::default()).unwrap();
        assert_eq!(result.to_markdown(), expected);
        assert!(result.equations().iter().all(|e| matches!(&e.outcome, EquationOutcome::Fallback { .. })));
        assert_eq!(result.document().diagnostics.iter().filter(|d| d.code == "PDF_EQUATION_FALLBACK").count(), 2);
    }
}
#[test]
fn invalid_and_duplicate_selection_fail_before_inference() {
    let doc = images(); let first = ids(&doc)[0].clone();
    let mut model = |_: &Block, _: &ExtractedAsset| -> Result<String> { panic!("must not run") };
    for selected in [vec![first.clone(), "unknown".into()], vec![first.clone(), first]] {
        assert!(matches!(recognize_image_equations(doc.clone(), &selected, &mut model, &EquationLimits::default()), Err(Error::InvalidInput(_))));
    }
}
#[test]
fn image_and_output_limits_have_safe_fallbacks() {
    let doc = images(); let selected = ids(&doc);
    let mut never = |_: &Block, _: &ExtractedAsset| -> Result<String> { panic!("oversized input") };
    let limits = EquationLimits { max_image_bytes: 0, ..Default::default() };
    let result = recognize_image_equations(doc.clone(), &selected, &mut never, &limits).unwrap();
    assert_eq!(result.to_markdown(), doc.to_markdown());
    let mut model = |_: &Block, _: &ExtractedAsset| Ok("long output".into());
    let limits = EquationLimits { max_latex_bytes: 1, ..Default::default() };
    let result = recognize_image_equations(doc.clone(), &selected, &mut model, &limits).unwrap();
    assert_eq!(result.to_markdown(), doc.to_markdown());
    let limits = EquationLimits { max_equations: 0, ..Default::default() };
    assert!(matches!(recognize_image_equations(doc, &selected, &mut never, &limits), Err(Error::LimitExceeded(_))));
}
#[test]
fn missing_or_unsupported_asset_does_not_invoke_model() {
    let mut model = |_: &Block, _: &ExtractedAsset| -> Result<String> { panic!("unsupported image") };
    let mut doc = images(); let selected = ids(&doc); doc.assets[0].media_type = "image/jp2".into();
    assert!(recognize_image_equations(doc, &selected, &mut model, &EquationLimits::default()).unwrap().equations().iter().all(|e| matches!(&e.outcome, EquationOutcome::Fallback { .. })));
    let mut doc = images(); doc.assets.clear();
    assert!(recognize_image_equations(doc, &selected, &mut model, &EquationLimits::default()).is_ok());
}
#[test]
fn empty_selection_and_ordinary_extraction_are_unchanged_and_no_ocr_runs() {
    let bytes = pdf(); let original = bytes.clone();
    let normal = extract_as("math.pdf", &bytes, Format::Pdf, &Options::default()).unwrap();
    let options = Options { pdf_ocr: Some(PdfOcr(std::sync::Arc::new(|_| panic!("OCR")))), ..Default::default() };
    let mut never = |_: &Block, _: &ExtractedAsset| -> Result<String> { panic!("not selected") };
    let result = extract_pdf_with_equations("math.pdf", &bytes, &options, &[], &mut never, &EquationLimits::default()).unwrap();
    assert_eq!(result.document().to_markdown(), result.to_markdown()); assert!(result.equations().is_empty());
    assert_eq!(normal, extract_as("math.pdf", &bytes, Format::Pdf, &Options::default()).unwrap()); assert_eq!(bytes, original);
}
