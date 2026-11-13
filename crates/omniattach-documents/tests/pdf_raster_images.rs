#![cfg(feature = "pdf")]
use lopdf::{Object, Stream, dictionary};
use omniattach_documents::*;
use std::io::Cursor;

fn fixture(color: &str, filter: Option<&str>, pixels: &[u8], width: i64, height: i64, special: Option<&str>, unique: bool) -> Vec<u8> {
    let mut pdf = lopdf::Document::with_version("1.5");
    let pages = pdf.new_object_id();
    let mut dict = dictionary! {"Type" => "XObject", "Subtype" => "Image", "Width" => width, "Height" => height, "BitsPerComponent" => 8, "ColorSpace" => color};
    if let Some(filter) = filter { dict.set("Filter", filter); }
    if let Some(key) = special { dict.set(key, Object::Null); }
    let stream = Stream::new(dict, pixels.to_vec());
    let one = pdf.add_object(stream.clone());
    let two = if unique { pdf.add_object(stream) } else { one };
    let resources = pdf.add_object(dictionary! {"XObject" => dictionary! {"One" => one, "Two" => two}});
    let content = pdf.add_object(Stream::new(dictionary! {}, b"/One Do /Two Do".to_vec()));
    let page = pdf.add_object(dictionary! {"Type" => "Page", "Parent" => pages, "Resources" => resources, "Contents" => content, "MediaBox" => vec![Object::Integer(0), Object::Integer(0), Object::Integer(100), Object::Integer(100)]});
    pdf.objects.insert(pages, Object::Dictionary(dictionary! {"Type" => "Pages", "Kids" => vec![Object::Reference(page)], "Count" => 1}));
    let root = pdf.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages});
    pdf.trailer.set("Root", root);
    let mut bytes = Vec::new();
    pdf.save_to(&mut bytes).unwrap();
    bytes
}
#[test]
fn raw_and_flate_gray_rgb_round_trip_pixels_and_reuse_asset() {
    let cases: [(&str, u32, &[u8], &[u8]); 2] = [
        ("DeviceRGB", 2, &[255, 0, 0, 0, 255, 0], &[120, 156, 251, 207, 192, 192, 240, 159, 1, 0, 7, 254, 1, 255]),
        ("DeviceGray", 3, &[0, 127, 255], &[120, 156, 99, 168, 255, 15, 0, 2, 0, 1, 127]),
    ];
    for (color, width, raw, flate) in cases {
        for filter in [None, Some("FlateDecode")] {
            let pdf = fixture(color, filter, if filter.is_some() { flate } else { raw }, i64::from(width), 1, None, false);
            let doc = extract_pdf_with_images("raster.pdf", &pdf, &Options::default()).unwrap();
            assert_eq!(doc.assets.len(), 1);
            assert_eq!(doc.blocks.len(), 2);
            let asset = &doc.assets[0];
            assert_eq!(asset.media_type, "image/png");
            assert!(asset.filename.ends_with("-fe-000001.png"));
            assert_eq!(doc.to_markdown().matches(&format!("]({})", asset.filename)).count(), 2);
            let mut reader = png::Decoder::new(Cursor::new(&asset.bytes)).read_info().unwrap();
            let mut decoded = vec![0; reader.output_buffer_size()];
            let frame = reader.next_frame(&mut decoded).unwrap();
            assert_eq!((frame.width, frame.height), (width, 1));
            assert_eq!(frame.bit_depth, png::BitDepth::Eight);
            assert_eq!(frame.color_type, if color == "DeviceRGB" { png::ColorType::Rgb } else { png::ColorType::Grayscale });
            assert_eq!(&decoded[..frame.buffer_size()], raw);
        }
    }
}
#[test]
fn malformed_or_unsupported_rasters_are_diagnosed_without_links() {
    for pdf in [
        fixture("DeviceCMYK", None, &[0; 8], 2, 1, None, false),
        fixture("DeviceRGB", None, &[0; 5], 2, 1, None, false),
        fixture("DeviceRGB", Some("FlateDecode"), b"bad", 2, 1, None, false),
        fixture("DeviceRGB", None, &[0; 6], 2, 1, Some("DecodeParms"), false),
        fixture("DeviceRGB", None, &[0; 6], 2, 1, Some("SMask"), false),
    ] {
        let doc = extract_pdf_with_images("unsupported.pdf", &pdf, &Options::default()).unwrap();
        assert!(doc.assets.is_empty());
        assert!(!doc.to_markdown().contains("!["));
        assert!(doc.diagnostics.iter().any(|d| d.code == "PDF_IMAGE_SKIPPED"));
    }
}
#[test]
fn dimensions_and_cumulative_decoded_bytes_are_bounded() {
    let mut options = Options::default();
    options.limits.archive_bytes = 128;
    let huge = fixture("DeviceRGB", None, &[], 100000, 100000, None, false);
    assert!(matches!(extract_pdf_with_images("huge.pdf", &huge, &options), Err(Error::LimitExceeded(_))));
    options.limits.archive_bytes = 4096;
    let raw = vec![0; 3000];
    let shared = fixture("DeviceRGB", None, &raw, 1000, 1, None, false);
    assert!(extract_pdf_with_images("shared.pdf", &shared, &options).is_ok());
    let distinct = fixture("DeviceRGB", None, &raw, 1000, 1, None, true);
    assert!(matches!(extract_pdf_with_images("distinct.pdf", &distinct, &options), Err(Error::LimitExceeded(message)) if message.contains("decoded raster")));
}
