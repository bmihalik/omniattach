#![cfg(feature = "pdf")]
use lopdf::{Object, Stream, dictionary};
use omniattach_documents::*;
use std::io::Cursor;

// A single stored DEFLATE block keeps synthetic fixtures dependency-free.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let len = u16::try_from(data.len()).unwrap();
    let mut out = vec![0x78, 0x01, 0x01];
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(data);
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    out.extend_from_slice(&((b << 16) | a).to_be_bytes());
    out
}
fn raster(color: &str, filtered: &[u8], params: Object) -> Stream {
    Stream::new(
        dictionary! {"Type" => "XObject", "Subtype" => "Image", "Width" => 2,
            "Height" => 2, "BitsPerComponent" => 8, "ColorSpace" => color,
            "Filter" => "FlateDecode", "DecodeParms" => params},
        zlib_stored(filtered),
    )
}
fn gray() -> Stream {
    raster("DeviceGray", &[1, 250, 16, 2, 11, 210],
        Object::Dictionary(dictionary! {"Predictor" => 15, "Columns" => 2}))
}
fn fixture(image: Stream) -> Vec<u8> {
    let mut pdf = lopdf::Document::with_version("1.5");
    let pages = pdf.new_object_id();
    let image_id = pdf.add_object(image);
    let font = pdf.add_object(dictionary! {"Type" => "Font", "Subtype" => "Type1",
        "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding"});
    let resources = pdf.add_object(dictionary! {"Font" => dictionary! {"F1" => font},
        "XObject" => dictionary! {"Im" => image_id}});
    let content = pdf.add_object(Stream::new(dictionary! {},
        b"BT /F1 12 Tf 1 0 0 1 10 90 Tm (before) Tj ET /Im Do BT /F1 12 Tf 1 0 0 1 10 60 Tm (between) Tj ET /Im Do BT /F1 12 Tf 1 0 0 1 10 30 Tm (after) Tj ET".to_vec()));
    let page = pdf.add_object(dictionary! {"Type" => "Page", "Parent" => pages,
        "Resources" => resources, "Contents" => content,
        "MediaBox" => vec![Object::Integer(0), Object::Integer(0), Object::Integer(120), Object::Integer(120)]});
    pdf.objects.insert(pages, Object::Dictionary(dictionary! {"Type" => "Pages",
        "Kids" => vec![Object::Reference(page)], "Count" => 1}));
    let root = pdf.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages});
    pdf.trailer.set("Root", root);
    let mut bytes = Vec::new();
    pdf.save_to(&mut bytes).unwrap();
    bytes
}
fn paragraph_text(doc: &Document) -> Vec<&str> {
    doc.blocks.iter().filter(|b| matches!(b.kind, BlockKind::Paragraph))
        .map(|b| b.text.as_str()).collect()
}

#[test]
fn predictor15_pdf_round_trips_gray_rgb_pixels_and_reuses_asset() {
    let rgb = raster("DeviceRGB", &[1, 250, 10, 80, 11, 210, 216, 4, 21, 190, 175, 95, 66, 11],
        Object::Dictionary(dictionary! {"Predictor" => 15, "Columns" => 2, "Colors" => 3, "BitsPerComponent" => 8}));
    for (image, expected, color) in [
        (gray(), vec![250, 10, 5, 220], png::ColorType::Grayscale),
        (rgb, vec![250, 10, 80, 5, 220, 40, 15, 200, 255, 100, 30, 10], png::ColorType::Rgb),
    ] {
        let bytes = fixture(image);
        let doc = extract_pdf_with_images("predictor.pdf", &bytes, &Options::default()).unwrap();
        assert_eq!(paragraph_text(&doc), vec!["before", "between", "after"]);
        assert_eq!(doc.assets.len(), 1);
        assert_eq!(doc.blocks.len(), 5);
        let asset = &doc.assets[0];
        assert_eq!(asset.media_type, "image/png");
        assert_eq!(asset.source.page, Some(1));
        for index in [1, 3] {
            assert!(matches!(&doc.blocks[index].kind, BlockKind::Image { asset_id } if asset_id == &asset.id));
        }
        assert_eq!(doc.to_markdown().matches(&format!("]({})", asset.filename)).count(), 2);
        assert!(!doc.diagnostics.iter().any(|d| d.code == "PDF_IMAGE_SKIPPED"));
        let mut reader = png::Decoder::new(Cursor::new(&asset.bytes)).read_info().unwrap();
        let mut output = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut output).unwrap();
        assert_eq!((frame.width, frame.height), (2, 2));
        assert_eq!(frame.color_type, color);
        assert_eq!(frame.bit_depth, png::BitDepth::Eight);
        assert_eq!(&output[..frame.buffer_size()], expected.as_slice());
        let restored: Document = serde_json::from_str(&serde_json::to_string(&doc).unwrap()).unwrap();
        assert_eq!(restored, doc);
    }
}
#[test]
fn rejected_predictor_images_keep_text_boundaries_without_dangling_links() {
    let mut bad_filter = gray();
    bad_filter.set_content(zlib_stored(&[5, 250, 16, 2, 11, 210]));
    let mut truncated = gray();
    truncated.set_content(zlib_stored(&[1, 250, 16, 2, 11]));
    let mut oversized = gray();
    oversized.set_content(zlib_stored(&[1, 250, 16, 2, 11, 210, 0]));
    let mut wrong_columns = gray();
    wrong_columns.dict.set("DecodeParms", dictionary! {"Predictor" => 15, "Columns" => 3});
    let mut masked = gray();
    masked.dict.set("SMask", Object::Null);
    let mut array_params = gray();
    array_params.dict.set("DecodeParms", Object::Array(vec![Object::Dictionary(
        dictionary! {"Predictor" => 15, "Columns" => 2})]));
    for image in [bad_filter, truncated, oversized, wrong_columns, masked, array_params] {
        let doc = extract_pdf_with_images("rejected.pdf", &fixture(image), &Options::default()).unwrap();
        assert_eq!(paragraph_text(&doc), vec!["before", "between", "after"]);
        assert_eq!(doc.blocks.len(), 3);
        assert!(doc.assets.is_empty());
        assert!(!doc.to_markdown().contains("!["));
        assert_eq!(doc.diagnostics.iter().filter(|d| d.code == "PDF_IMAGE_SKIPPED").count(), 2);
    }
}
#[test]
fn ordinary_pdf_extraction_does_not_opt_in_to_predictor_export() {
    let bytes = fixture(gray());
    let ordinary = extract_as("ordinary.pdf", &bytes, Format::Pdf, &Options::default()).unwrap();
    assert!(ordinary.assets.is_empty());
    assert!(!ordinary.blocks.iter().any(|b| matches!(b.kind, BlockKind::Image { .. })));
    assert!(!ordinary.to_markdown().contains("!["));
    let text = paragraph_text(&ordinary).join(" ");
    for word in ["before", "between", "after"] {
        assert!(text.contains(word));
    }
}
