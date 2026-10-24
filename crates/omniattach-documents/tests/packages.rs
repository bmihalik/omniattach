#![cfg(feature = "office")]
use omniattach_documents::*;
use std::io::{Cursor, Write};
fn package(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, text) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}
fn read(name: &str, entries: &[(&str, &str)]) -> Document {
    extract_bytes(name, &package(entries), &Options::default()).unwrap()
}
#[test]
fn docx_heading_runs_table_and_member_provenance() {
    let doc = read(
        "a.docx",
        &[(
            "word/document.xml",
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Title</w:t></w:r></w:p><w:p><w:r><w:t>Hello</w:t><w:tab/><w:t>world</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#,
        )],
    );
    assert_eq!(doc.blocks.len(), 3);
    assert!(matches!(
        doc.blocks[0].kind,
        BlockKind::Heading { level: 1 }
    ));
    assert_eq!(doc.blocks[1].text, "Hello\tworld");
    assert_eq!(doc.tables[0].rows[0][1].display(), "B");
    assert_eq!(
        doc.tables[0].rows[0][0].source.member.as_deref(),
        Some("word/document.xml")
    );
}
#[test]
fn pptx_uses_relationship_order_not_filename_order() {
    let doc = read(
        "a.pptx",
        &[
            (
                "ppt/presentation.xml",
                r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="1" r:id="rB"/><p:sldId id="2" r:id="rA"/></p:sldIdLst></p:presentation>"#,
            ),
            (
                "ppt/_rels/presentation.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rA" Target="slides/slide1.xml"/><Relationship Id="rB" Target="slides/slide99.xml"/></Relationships>"#,
            ),
            (
                "ppt/slides/slide1.xml",
                r#"<a:p xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:r><a:t>Second</a:t></a:r></a:p>"#,
            ),
            (
                "ppt/slides/slide99.xml",
                r#"<a:p xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:r><a:t>First</a:t></a:r></a:p>"#,
            ),
        ],
    );
    assert_eq!(doc.blocks[0].text, "First");
    assert_eq!(doc.blocks[0].source.slide, Some(1));
    assert_eq!(doc.blocks[1].text, "Second");
}
const ODF: &str = r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"><office:body>BODY</office:body></office:document-content>"#;
#[test]
fn odt_headings_and_text_spaces() {
    let input = ODF.replace("BODY", "<office:text><text:h text:outline-level=\"2\">Title</text:h><text:p>A<text:s/>B<text:tab/>C</text:p></office:text>");
    let doc = read("a.odt", &[("content.xml", &input)]);
    assert!(matches!(
        doc.blocks[0].kind,
        BlockKind::Heading { level: 2 }
    ));
    assert_eq!(doc.blocks[1].text, "A B\tC");
}
#[test]
fn odp_slide_numbers() {
    let input = ODF.replace("BODY", "<office:presentation><draw:page><text:p>First</text:p></draw:page><draw:page><text:p>Second</text:p></draw:page></office:presentation>");
    let doc = read("a.odp", &[("content.xml", &input)]);
    assert_eq!(doc.blocks[1].source.slide, Some(2));
}
#[test]
fn ods_values_formulas_repetitions_and_blank_tails() {
    let input = ODF.replace("BODY", r#"<office:spreadsheet><table:table table:name="Data"><table:table-row><table:table-cell office:value-type="string"><text:p>ID</text:p></table:table-cell><table:table-cell office:value-type="float" office:value="1.2300" table:formula="of:=1+0.23" table:number-columns-repeated="2"><text:p>1.23</text:p></table:table-cell><table:table-cell table:number-columns-repeated="999999"/></table:table-row><table:table-row table:number-rows-repeated="2"><table:table-cell office:value-type="boolean" office:boolean-value="true"/></table:table-row><table:table-row table:number-rows-repeated="99999999"><table:table-cell/></table:table-row></table:table></office:spreadsheet>"#);
    let doc = read("a.ods", &[("content.xml", &input)]);
    let table = &doc.tables[0];
    assert_eq!(table.rows.len(), 3);
    assert_eq!(table.columns, ["A", "B", "C"]);
    assert_eq!(table.rows[0][1].raw.as_deref(), Some("1.2300"));
    assert_eq!(table.rows[0][1].formula.as_deref(), Some("of:=1+0.23"));
    assert_eq!(table.rows[0][2].source.column, Some(3));
    assert_eq!(table.rows[2][0].source.row, Some(3));
    assert!(matches!(
        table.rows[2][0].value,
        Some(CellValue::Boolean(true))
    ));
}
#[test]
fn ods_nonempty_repetition_budget_is_checked_before_expansion() {
    let input = ODF.replace("BODY", r#"<office:spreadsheet><table:table table:name="Data"><table:table-row table:number-rows-repeated="999999999"><table:table-cell office:value-type="float" office:value="42"/></table:table-row></table:table></office:spreadsheet>"#);
    assert!(matches!(
        extract_bytes(
            "a.ods",
            &package(&[("content.xml", &input)]),
            &Options::default()
        ),
        Err(Error::LimitExceeded(_))
    ));
}
#[test]
fn epub_follows_spine_and_internal_relative_paths() {
    let doc = read(
        "a.epub",
        &[
            (
                "META-INF/container.xml",
                r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OPS/book.opf"/></rootfiles></container>"#,
            ),
            (
                "OPS/book.opf",
                r#"<package xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/"><metadata><dc:title>Book</dc:title></metadata><manifest><item id="a" href="text/a.xhtml"/><item id="b" href="text/b.xhtml"/></manifest><spine><itemref idref="b"/><itemref idref="a"/></spine></package>"#,
            ),
            ("OPS/text/a.xhtml", "<p>Last chapter</p>"),
            ("OPS/text/b.xhtml", "<p>First chapter</p>"),
        ],
    );
    assert_eq!(doc.title.as_deref(), Some("Book"));
    assert_eq!(doc.blocks[0].text, "First chapter");
    assert_eq!(
        doc.blocks[1].source.member.as_deref(),
        Some("OPS/text/a.xhtml")
    );
}
#[test]
fn xlsx_cached_formula_and_actual_cell_addresses() {
    let doc = read(
        "a.xlsx",
        &[
            (
                "[Content_Types].xml",
                r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#,
            ),
            (
                "_rels/.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
            ),
            (
                "xl/workbook.xml",
                r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="B3:C4"/><sheetData><row r="3"><c r="B3" t="inlineStr"><is><t>001</t></is></c><c r="C3"><f>1+2</f><v>3</v></c></row><row r="4"><c r="B4"><v>9007199254740993</v></c></row></sheetData></worksheet>"#,
            ),
        ],
    );
    let table = &doc.tables[0];
    assert_eq!(table.columns, ["B", "C"]);
    assert_eq!(table.rows[0][0].display(), "001");
    assert_eq!(table.rows[0][0].source.row, Some(3));
    assert_eq!(table.rows[0][1].formula.as_deref(), Some("1+2"));
    assert_eq!(table.rows[0][1].display(), "3");
}
#[test]
fn archive_budgets_and_traversal() {
    let mut options = Options::default();
    options.limits.archive_bytes = 2;
    let data = package(&[("word/document.xml", "123")]);
    assert!(matches!(
        extract_bytes("a.docx", &data, &options),
        Err(Error::LimitExceeded(_))
    ));
    let data = package(&[("../evil", "bad"), ("word/document.xml", "<x/>")]);
    assert!(matches!(
        extract_bytes("a.docx", &data, &Options::default()),
        Err(Error::InvalidInput(_))
    ));
}
#[test]
fn explicit_workbook_format_handles_extensionless_ods() {
    let input = ODF.replace("BODY", r#"<office:spreadsheet><table:table table:name="Data"><table:table-row><table:table-cell office:value-type="string"><text:p>value</text:p></table:table-cell></table:table-row></table:table></office:spreadsheet>"#);
    let doc = extract_as(
        "extensionless",
        &package(&[("content.xml", &input)]),
        Format::Workbook,
        &Options::default(),
    )
    .unwrap();
    assert_eq!(doc.tables[0].rows[0][0].display(), "value");
}
