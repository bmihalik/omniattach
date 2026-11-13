#![cfg(feature = "pdf")]
use lopdf::{Document as Pdf, Object, ObjectId, Stream, dictionary};
use omniattach_documents::*;
const GRID: &str =
    "0 0 m 100 0 l 0 20 m 100 20 l 0 40 m 100 40 l 0 0 m 0 40 l 50 0 m 50 40 l 100 0 m 100 40 l";
fn fixture(pages: &[&str], form: Option<&str>) -> (Vec<u8>, ObjectId, ObjectId) {
    let mut pdf = Pdf::with_version("1.5");
    let font=pdf.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica","Encoding"=>"WinAnsiEncoding"});
    // Header-only fixture payload: inspection never decodes or saves pixels.
    let image=pdf.add_object(Stream::new(dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>1,"Height"=>1,"ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8,"Filter"=>"DCTDecode"},vec![0xff,0xd8,0xff,0xd9]));
    let form_id = pdf.new_object_id();
    let mut objects = dictionary! {"Im"=>image};
    if form.is_some() {
        objects.set("Fm", form_id);
    }
    let resources = dictionary! {"Font"=>dictionary! {"F1"=>font},"XObject"=>objects};
    if let Some(text) = form {
        pdf.objects.insert(form_id,Object::Stream(Stream::new(dictionary! {"Type"=>"XObject","Subtype"=>"Form","BBox"=>vec![0.into(),0.into(),100.into(),100.into()],"Resources"=>resources.clone()},text.as_bytes().to_vec())));
    }
    let root = pdf.new_object_id();
    let mut kids = Vec::new();
    for text in pages {
        let content = pdf.add_object(Stream::new(dictionary! {}, text.as_bytes().to_vec()));
        let page = pdf.add_object(dictionary! {"Type"=>"Page","Parent"=>root,"Contents"=>content});
        kids.push(page.into());
    }
    let count = kids.len() as i64;
    pdf.objects.insert(root,Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>kids,"Count"=>count,"Resources"=>resources,"MediaBox"=>vec![0.into(),0.into(),595.into(),842.into()]}));
    let catalog = pdf.add_object(dictionary! {"Type"=>"Catalog","Pages"=>root});
    pdf.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    pdf.save_to(&mut bytes).unwrap();
    (bytes, image, form_id)
}
#[test]
fn text_image_order_and_repeated_image_occurrences_are_retained() {
    let (bytes, image, _) = fixture(
        &[
            "BT /F1 12 Tf 72 700 Td (before) Tj ET q 100 0 0 50 10 20 cm /Im Do Q BT /F1 12 Tf 72 680 Td (after) Tj ET /Im Do",
        ],
        None,
    );
    let r = inspect_pdf_bytes("book.pdf", &bytes, &Options::default()).unwrap();
    assert_eq!(
        r.elements.iter().map(|e| e.kind).collect::<Vec<_>>(),
        vec![
            PdfElementKind::Text,
            PdfElementKind::Image,
            PdfElementKind::Text,
            PdfElementKind::Image
        ]
    );
    assert_eq!(r.elements[1].anchor.object_id, Some(image));
    assert_eq!(r.elements[3].anchor.object_id, Some(image));
    assert_ne!(
        r.elements[1].anchor.operation_visit,
        r.elements[3].anchor.operation_visit
    );
    assert_eq!(r.elements[1].pixel_size, Some([1, 1]));
    let b = r.elements[1].bounds.unwrap();
    assert_eq!((b.left, b.bottom, b.right, b.top), (10., 20., 110., 70.));
    assert!(!b.estimated);
    assert!(r.elements[0].bounds.unwrap().estimated);
    assert_eq!(r.elements[0].text.as_deref(), Some("before"));
}
#[test]
fn forms_retain_occurrence_paths_and_cycles_fail() {
    let (bytes, image, form) = fixture(&["/Fm Do /Fm Do"], Some("q 20 0 0 10 0 0 cm /Im Do Q"));
    let r = inspect_pdf_bytes("forms.pdf", &bytes, &Options::default()).unwrap();
    assert_eq!(r.elements.len(), 2);
    for e in &r.elements {
        assert_eq!(e.anchor.object_id, Some(image));
        assert_eq!(e.anchor.form_path, vec![form]);
    }
    assert_ne!(
        r.elements[0].anchor.operation_visit,
        r.elements[1].anchor.operation_visit
    );
    let (bytes, _, _) = fixture(&["/Fm Do"], Some("/Fm Do"));
    assert!(matches!(
        inspect_pdf_bytes("cycle.pdf", &bytes, &Options::default()),
        Err(Error::InvalidInput(_))
    ));
}
#[test]
fn equations_and_direct_tags_are_hints_without_replacing_text() {
    let (bytes, _, _) = fixture(
        &["/Formula BMC BT /F1 12 Tf 72 700 Td (x = y) Tj ET EMC /Table BMC 0 0 100 40 re S EMC"],
        None,
    );
    let r = inspect_pdf_bytes("hints.pdf", &bytes, &Options::default()).unwrap();
    assert!(
        r.elements
            .iter()
            .any(|e| e.kind == PdfElementKind::EquationCandidate && e.derived_from.is_empty())
    );
    let hint = r
        .elements
        .iter()
        .find(|e| !e.derived_from.is_empty())
        .unwrap();
    let text = &r.elements[hint.derived_from[0] - 1];
    assert_eq!(text.kind, PdfElementKind::Text);
    assert_eq!(text.text.as_deref(), Some("x = y"));
    assert!(hint.text.is_none());
    assert_eq!(
        r.elements
            .iter()
            .filter(|e| e.kind == PdfElementKind::TableCandidate)
            .count(),
        1
    );
}
#[test]
fn only_stroked_grid_paths_produce_geometric_table_hints() {
    for (paint, count) in [("S", 1), ("n", 0), ("f", 0)] {
        let text = format!("{GRID} {paint}");
        let (bytes, _, _) = fixture(&[&text], None);
        let r = inspect_pdf_bytes("grid.pdf", &bytes, &Options::default()).unwrap();
        assert_eq!(
            r.elements
                .iter()
                .filter(|e| e.kind == PdfElementKind::TableCandidate)
                .count(),
            count
        );
    }
    let (bytes, _, _) = fixture(&["0 0 100 40 re S"], None);
    assert!(
        inspect_pdf_bytes("box.pdf", &bytes, &Options::default())
            .unwrap()
            .elements
            .is_empty()
    );
}
#[test]
fn sequence_and_hint_references_are_global_across_pages() {
    let (bytes, _, _) = fixture(
        &["BT /F1 12 Tf (x = y) Tj ET", "BT /F1 12 Tf (a = b) Tj ET"],
        None,
    );
    let r = inspect_pdf_bytes("pages.pdf", &bytes, &Options::default()).unwrap();
    assert_eq!(r.page_count, 2);
    assert_eq!(
        r.elements.iter().map(|e| e.sequence).collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    assert_eq!(r.elements[3].derived_from, vec![3]);
    assert_eq!(
        r.elements.iter().map(|e| e.anchor.page).collect::<Vec<_>>(),
        vec![1, 1, 2, 2]
    );
    let decoded: PdfInspection = serde_json::from_str(&r.to_json().unwrap()).unwrap();
    assert_eq!(decoded, r);
}
#[test]
fn inspection_does_not_change_native_text_or_invoke_ocr() {
    let (bytes, _, _) = fixture(
        &["BT /F1 12 Tf 72 700 Td (A szamito-) Tj 0 -14 Td (gep mukodik.) Tj ET"],
        None,
    );
    let raw = Options {
        dehyphenate_max_tokens: 0,
        ..Default::default()
    };
    let before = extract_bytes("a.pdf", &bytes, &raw).unwrap();
    let inspection = inspect_pdf_bytes("a.pdf", &bytes, &raw).unwrap();
    let after = extract_bytes("a.pdf", &bytes, &raw).unwrap();
    assert_eq!(before, after);
    assert_eq!(after.to_text(), "A szamito-\ngep mukodik.");
    assert_eq!(after.to_markdown(), "A szamito-\ngep mukodik.\n");
    assert_eq!(inspection.elements.len(), 2);
    let ocr = Options {
        pdf_ocr: Some(PdfOcr(std::sync::Arc::new(|_| {
            panic!("inspection must not run OCR")
        }))),
        ..Default::default()
    };
    let (empty, _, _) = fixture(&[""], None);
    assert!(
        inspect_pdf_bytes("empty.pdf", &empty, &ocr)
            .unwrap()
            .elements
            .is_empty()
    );
}
#[test]
fn budgets_and_undecodable_text_are_explicit() {
    let (bytes, _, _) = fixture(&["BT /F1 12 Tf (x = y) Tj ET"], None);
    let mut options = Options::default();
    options.limits.blocks = 1;
    assert!(matches!(
        inspect_pdf_bytes("limit.pdf", &bytes, &options),
        Err(Error::LimitExceeded(_))
    ));
    options = Options::default();
    options.limits.input_bytes = 1;
    assert!(matches!(
        inspect_pdf_bytes("limit.pdf", &bytes, &options),
        Err(Error::LimitExceeded(_))
    ));
    options = Options::default();
    options.limits.xml_nodes = 1;
    assert!(matches!(
        inspect_pdf_bytes("limit.pdf", &bytes, &options),
        Err(Error::LimitExceeded(_))
    ));
    let text = format!("{GRID} S");
    let (grid, _, _) = fixture(&[&text], None);
    options = Options::default();
    options.limits.archive_entries = 1;
    assert!(matches!(
        inspect_pdf_bytes("limit.pdf", &grid, &options),
        Err(Error::LimitExceeded(_))
    ));
    let (bytes, _, _) = fixture(&["BT /Missing 12 Tf (unknown) Tj ET"], None);
    let r = inspect_pdf_bytes("font.pdf", &bytes, &Options::default()).unwrap();
    assert!(r.elements[0].text.is_none());
    assert!(r.diagnostics.iter().any(|d| d.code == "PDF_UNDECODED_TEXT"));
    assert!(inspect_pdf_bytes("bad.pdf", b"not PDF", &Options::default()).is_err());
}
#[test]
fn local_file_api_preserves_source_identity_without_sidecars() {
    use sha2::{Digest, Sha256};
    let (bytes, _, _) = fixture(&["BT /F1 12 Tf (plain text) Tj ET"], None);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.pdf");
    std::fs::write(&path, &bytes).unwrap();
    let r = inspect_pdf(&path, &Options::default()).unwrap();
    assert_eq!(r.source.name, path.to_string_lossy());
    assert_eq!(r.source.sha256, format!("{:x}", Sha256::digest(&bytes)));
    assert_eq!(r.source.byte_length, bytes.len());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "LFE_INSPECTION_SCOPE")
    );
}
