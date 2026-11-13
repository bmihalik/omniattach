use omniattach_documents::*;
fn document(path: &str) -> Document {
    use sha2::{Digest, Sha256};
    let mut d = extract_bytes("empty.md", b"", &Options::default()).unwrap();
    d.schema_version = "omniattach-documents/0.2".into();
    let bytes = b"render-only fixture".to_vec();
    let source = SourceSpan {
        page: Some(1),
        ..Default::default()
    };
    d.assets.push(ExtractedAsset {
        id: "asset-1".into(),
        filename: path.into(),
        media_type: "image/png".into(),
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        byte_length: bytes.len(),
        bytes,
        source: source.clone(),
    });
    d.blocks.push(Block {
        id: "image-1".into(),
        parent_id: None,
        kind: BlockKind::Image {
            asset_id: "asset-1".into(),
        },
        text: "Image 1".into(),
        section_path: vec![],
        source,
        keep_together: true,
    });
    d
}
#[test]
fn typed_image_renders_as_link_not_escaped_prose() {
    let d = document("images/book-12345678-fe-000001.png");
    assert_eq!(
        d.to_markdown(),
        "![Image 1](images/book-12345678-fe-000001.png)\n"
    );
    assert_eq!(d.to_text(), "[Image: Image 1]");
    let json: serde_json::Value = serde_json::from_str(&d.to_json().unwrap()).unwrap();
    assert_eq!(json["blocks"][0]["kind"]["type"], "image");
    assert_eq!(json["blocks"][0]["kind"]["asset_id"], "asset-1");
}
#[test]
fn image_urls_and_alt_labels_are_escaped_in_their_own_contexts() {
    let mut d = document("images/a (b)#.png");
    d.blocks[0].text = "a [label]\nsecond line".into();
    assert_eq!(
        d.to_markdown(),
        "![a \\[label\\] second line](images/a%20%28b%29%23.png)\n"
    );
}
#[test]
fn missing_or_unsafe_assets_never_produce_broken_image_links() {
    for path in [
        "../image.png",
        "images/../../outside.png",
        "/tmp/a.png",
        "images\\a.png",
        "images/http:evil.png",
    ] {
        let d = document(path);
        let md = d.to_markdown();
        assert!(md.starts_with("Image unavailable:"));
        assert!(!md.contains("]("));
    }
    let mut d = document("images/a.png");
    d.assets.clear();
    assert!(d.to_markdown().starts_with("Image unavailable:"));
}
#[test]
fn ordinary_markdown_rendering_stays_semantic_and_does_not_turn_text_into_images() {
    let d = extract_bytes("a.txt", b"![not an image](file.png)", &Options::default()).unwrap();
    assert!(d.to_markdown().starts_with("\\!\\["));
}
