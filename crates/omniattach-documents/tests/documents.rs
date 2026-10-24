use omniattach_documents::*;
fn read(name: &str, text: &str) -> Document {
    extract_bytes(name, text.as_bytes(), &Options::default()).unwrap()
}

#[test]
fn unicode_lines_and_crlf_spans() {
    let text = "Árvíztűrő\r\n\r\nTükörfúrógép\n";
    let doc = read("a.txt", text);
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(doc.blocks[1].source.line_start, Some(3));
    for b in &doc.blocks {
        let s = &b.source;
        assert_eq!(
            &text[s.decoded_byte_start.unwrap()..s.decoded_byte_end.unwrap()],
            b.text
        );
    }
}
#[test]
fn strict_encoding_and_explicit_fallback() {
    assert!(extract_bytes("a.txt", &[0xe1], &Options::default()).is_err());
    let options = Options {
        encoding: Some("windows-1250".into()),
        ..Options::default()
    };
    assert_eq!(
        extract_bytes("a.txt", &[0xe1], &options).unwrap().to_text(),
        "á"
    );
    assert_eq!(
        extract_bytes("a.txt", &[0xff, 0xfe, 0x41, 0], &Options::default())
            .unwrap()
            .to_text(),
        "A"
    );
}
#[test]
fn code_is_not_prose() {
    let doc = read("a.rs", "fn main() { println!(\"```\"); }\n");
    assert!(matches!(&doc.blocks[0].kind, BlockKind::Code { language: Some(s) } if s == "rs"));
    assert!(doc.to_markdown().starts_with("````rs\n"));
}
#[test]
fn heading_hierarchy_and_parents() {
    let doc = read("a.md", "# First\n\nA\n\n## Second\n\nB\n\n# Third\n\nC");
    assert_eq!(doc.blocks[3].section_path, vec!["First", "Second"]);
    assert_eq!(
        doc.blocks[3].parent_id.as_deref(),
        Some(doc.blocks[2].id.as_str())
    );
    assert_eq!(doc.blocks[5].section_path, vec!["Third"]);
    assert_eq!(doc.blocks[4].parent_id, None);
    for b in &doc.blocks {
        let s = &b.source;
        assert!(s.decoded_byte_start.unwrap() <= s.decoded_byte_end.unwrap());
    }
}
#[test]
fn markdown_table_and_code() {
    let doc = read(
        "a.md",
        "# A\n\n| x | y |\n|---|---|\n| one | two |\n\n```rust\nlet a = 1;\n```\n",
    );
    assert_eq!(doc.tables.len(), 1);
    assert_eq!(doc.tables[0].columns, ["x", "y"]);
    assert_eq!(doc.tables[0].rows[0][1].display(), "two");
    assert!(doc.to_text().contains("let a = 1;"));
    assert!(doc.to_markdown().contains("| one | two |"));
}
#[test]
fn markdown_lists_quotes_and_rules() {
    let doc = read("a.md", "- one\n- two\n\n> quote\n\n---\n");
    assert!(
        doc.blocks
            .iter()
            .any(|b| matches!(b.kind, BlockKind::ListItem { .. }) && b.text == "one")
    );
    assert!(
        doc.blocks
            .iter()
            .any(|b| matches!(b.kind, BlockKind::Quote) && b.text == "quote")
    );
    assert!(
        doc.blocks
            .iter()
            .any(|b| matches!(b.kind, BlockKind::ThematicBreak))
    );
}
#[test]
fn csv_quotes_ragged_rows_and_nulls() {
    let doc = read("a.csv", "a,b\n\"x,y\",\"multi\nline\"\nshort\n1,2,3\n");
    let table = &doc.tables[0];
    assert_eq!(table.columns, ["a", "b", "column_3"]);
    assert_eq!(table.rows[0][0].display(), "x,y");
    assert_eq!(table.rows[0][1].display(), "multi\nline");
    assert_eq!(table.rows[1][1].raw, None);
    assert!(
        table
            .to_csv_with(&CsvOptions {
                null: "NULL".into(),
                ..Default::default()
            })
            .unwrap()
            .contains("short,NULL,NULL")
    );
}
#[test]
fn headerless_tsv_and_formula_presentation() {
    let options = Options {
        csv_has_headers: false,
        ..Default::default()
    };
    let doc = extract_bytes("a.tsv", b"=SUM(A1)\t42\n", &options).unwrap();
    let table = &doc.tables[0];
    assert!(!table.has_header);
    assert!(
        table
            .to_csv_with(&CsvOptions {
                escape_formulas: true,
                ..Default::default()
            })
            .unwrap()
            .contains("'=SUM(A1)")
    );
    assert_eq!(table.rows[0][0].display(), "=SUM(A1)");
}
#[test]
fn html_ignores_executable_content_and_keeps_table() {
    let doc = read(
        "a.html",
        "<title>Title</title><h1>Hi</h1><p>A <em>B</em><script>secret</script></p><p hidden>hidden</p><table><tr><th>x</th></tr><tr><td>1</td></tr></table>",
    );
    assert_eq!(doc.title.as_deref(), Some("Title"));
    assert!(doc.to_text().contains("A B"));
    assert!(!doc.to_text().contains("secret"));
    assert!(!doc.to_text().contains("hidden"));
    assert_eq!(doc.tables[0].rows[0][0].display(), "1");
}
#[test]
fn markdown_render_does_not_emit_executable_html() {
    let doc = read("a.txt", "<script>alert(1)</script>");
    assert!(doc.to_markdown().contains("&lt;script&gt;"));
}
fn mapping() -> XmlTableSpec {
    XmlTableSpec::new(
        "/x:root/x:item",
        vec![
            XmlColumn::new("id", "@id"),
            XmlColumn::new("name", "x:name"),
            XmlColumn::new("missing", "x:missing"),
        ],
    )
    .namespace("x", "urn:test")
}
#[test]
fn xml_namespaces_attributes_missing_empty_and_mixed_content() {
    let doc = extract_xml_table("a.xml", br#"<root xmlns="urn:test"><item id="01"><name>A<b>B</b>C</name><missing/></item><item id="02"><name>D</name></item></root>"#, mapping(), &Options::default()).unwrap();
    let rows = &doc.tables[0].rows;
    assert_eq!(rows[0][1].display(), "ABC");
    assert_eq!(rows[0][2].raw.as_deref(), Some(""));
    assert_eq!(rows[1][2].raw, None);
    assert!(rows[1][0].source.path.as_ref().unwrap().contains("[2]/@id"));
}
#[test]
fn xml_repeated_values_need_policy() {
    let xml = b"<root><item><v>a</v><v>b</v></item></root>";
    let spec = XmlTableSpec::new("/root/item", vec![XmlColumn::new("v", "v")]);
    assert!(extract_xml_table("a.xml", xml, spec.clone(), &Options::default()).is_err());
    let mut joined = spec.clone();
    joined.columns[0].repeated = Repeated::Join(";".into());
    assert_eq!(
        extract_xml_table("a.xml", xml, joined, &Options::default())
            .unwrap()
            .tables[0]
            .rows[0][0]
            .display(),
        "a;b"
    );
    let mut json = spec;
    json.columns[0].repeated = Repeated::Json;
    assert_eq!(
        extract_xml_table("a.xml", xml, json, &Options::default())
            .unwrap()
            .tables[0]
            .rows[0][0]
            .display(),
        "[\"a\",\"b\"]"
    );
}
#[test]
fn xml_mapping_validation_on_empty_result() {
    let spec = XmlTableSpec::new("/root/item", vec![XmlColumn::new("v", "unbound:v")]);
    assert!(extract_xml_table("a.xml", b"<root/>", spec, &Options::default()).is_err());
    assert!(matches!(
        extract_bytes("a.xml", b"<root/>", &Options::default()),
        Err(Error::MappingRequired)
    ));
}
#[test]
fn reject_xml_entity_declarations() {
    let spec = XmlTableSpec::new("/root/item", vec![XmlColumn::new("v", ".")]);
    let xml = br#"<!DOCTYPE root [<!ENTITY evil SYSTEM "file:///etc/passwd">]><root><item>&evil;</item></root>"#;
    assert!(extract_xml_table("a.xml", xml, spec, &Options::default()).is_err());
}
#[test]
fn json_preserves_original_numbers_and_yaml_is_text() {
    let doc = read("a.json", "{\"id\":9007199254740993,\"v\":1.2300}");
    assert!(doc.to_text().contains("1.2300"));
    assert_eq!(read("a.yaml", "a: 1\n").to_text(), "a: 1");
    assert!(extract_bytes("a.jsonl", b"{}\nnot json\n", &Options::default()).is_err());
}
#[test]
fn roundtrip_serialization_and_stable_ids() {
    let doc = read("a.md", "# Hello\n\nBody");
    let decoded: Document = serde_json::from_str(&doc.to_json().unwrap()).unwrap();
    assert_eq!(decoded, doc);
    assert_eq!(
        read("b.md", "# Hello\n\nBody").blocks[0].id,
        doc.blocks[0].id
    );
}
#[test]
fn limits_and_unsupported() {
    let mut options = Options::default();
    options.limits.input_bytes = 2;
    assert!(matches!(
        extract_bytes("a.txt", b"long", &options),
        Err(Error::LimitExceeded(_))
    ));
    options.limits.input_bytes = 100;
    options.limits.cells = 1;
    assert!(extract_bytes("a.csv", b"a,b\n1,2", &options).is_err());
    assert!(matches!(
        extract_bytes("a.mp4", b"", &Options::default()),
        Err(Error::UnsupportedFormat(_))
    ));
}
#[test]
fn extraction_has_no_source_side_effects() {
    let root = std::env::temp_dir().join(format!("omniattach-test-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("input.txt");
    std::fs::write(&path, "hello").unwrap();
    assert_eq!(
        extract(&path, &Options::default()).unwrap().to_text(),
        "hello"
    );
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    std::fs::remove_dir_all(root).unwrap();
}
#[cfg(not(feature = "office"))]
#[test]
fn disabled_office_is_explicit() {
    assert!(matches!(
        extract_bytes("a.docx", b"", &Options::default()),
        Err(Error::FeatureDisabled(_))
    ));
}
#[cfg(not(feature = "pdf"))]
#[test]
fn disabled_pdf_is_explicit() {
    assert!(matches!(
        extract_bytes("a.pdf", b"", &Options::default()),
        Err(Error::FeatureDisabled(_))
    ));
}
#[test]
fn binary_disguised_as_text_and_utf32_are_rejected() {
    assert!(extract_bytes("a.txt", b"hello\0binary", &Options::default()).is_err());
    assert!(
        extract_bytes(
            "a.txt",
            &[0xff, 0xfe, 0, 0, 65, 0, 0, 0],
            &Options::default()
        )
        .is_err()
    );
}
#[test]
fn ragged_table_padding_is_bounded() {
    let mut options = Options::default();
    options.limits.cells = 8;
    assert!(matches!(
        extract_bytes("a.csv", b"a\n1\n2\n3\n4,5,6", &options),
        Err(Error::LimitExceeded(_))
    ));
}
#[test]
fn selectors_do_not_ignore_default_namespaces() {
    let spec = XmlTableSpec::new("/root/item", vec![XmlColumn::new("v", ".")]);
    let doc = extract_xml_table(
        "a.xml",
        br#"<root xmlns="urn:x"><item>not unqualified</item></root>"#,
        spec,
        &Options::default(),
    )
    .unwrap();
    assert!(doc.tables[0].rows.is_empty());
    assert!(doc.diagnostics.iter().any(|d| d.code == "NO_RECORDS"));
}
#[test]
fn xml_invalid_deeper_selector_errors_even_without_children() {
    let spec = XmlTableSpec::new("/root/item", vec![XmlColumn::new("v", "missing/bad:v")]);
    assert!(extract_xml_table("a.xml", b"<root/>", spec, &Options::default()).is_err());
}
